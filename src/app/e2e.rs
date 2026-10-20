//! Scripted end-to-end driver (hidden `--e2e <script>` flag).
//!
//! Drives the real window + GL + game through the same input entry points
//! the OS events use, one script step per frame, so the full runtime path
//! (New Game → 2D map → possession → 3D → modals) can be exercised and
//! captured without a human at the keyboard.
//!
//! Script grammar (one command per line, `#` comments):
//! - `frames N` — run N frames with no new input
//! - `key NAME` — press, hold one frame, release (NAME = winit `KeyCode` name, e.g. `KeyA`, `Enter`)
//! - `down NAME` / `up NAME` — hold / release a key or `MouseLeft`/`MouseRight`/`MouseMiddle`
//! - `type TEXT` — feed TEXT as typed characters (New Game text fields)
//! - `cursor FX FY` — place the cursor at a window fraction (0..1, y down)
//! - `cursor_world X Y` — place the cursor over a world point on the 2D map
//! - `cursor_robot N` — place the cursor over the N-th live robot of the active team on the 2D map
//! - `click left|right` — press, hold one frame, release a mouse button
//! - `mouse DX DY` — inject a raw relative mouse motion
//! - `wheel N` — inject N wheel detents
//! - `shot NAME` — save the next rendered frame to `<out>/NAME.png`
//! - `state NAME` — write a game-state dump to `<out>/NAME.txt`
//! - `quit` — exit the app

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use winit::event::MouseButton;
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::input::keys::Key;

/// A key-ish input target: keyboard key or mouse button.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Target {
    Key(KeyCode),
    Mouse(MouseButton),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Frames(u32),
    Down(Target),
    Up(Target),
    Type(String),
    Cursor(f32, f32),
    CursorWorld(f32, f32),
    CursorRobot(usize),
    Mouse(f64, f64),
    Wheel(f32),
    Shot(String),
    State(String),
    Quit,
}

pub fn parse_target(name: &str) -> Result<Target, String> {
    match name {
        "MouseLeft" | "left" => return Ok(Target::Mouse(MouseButton::Left)),
        "MouseRight" | "right" => return Ok(Target::Mouse(MouseButton::Right)),
        "MouseMiddle" | "middle" => return Ok(Target::Mouse(MouseButton::Middle)),
        _ => {}
    }
    let extra = [
        ("Enter", KeyCode::Enter),
        ("Tab", KeyCode::Tab),
        ("Backspace", KeyCode::Backspace),
        ("Escape", KeyCode::Escape),
    ];
    if let Some((_, code)) = extra.iter().find(|(n, _)| *n == name) {
        return Ok(Target::Key(*code));
    }
    Key::ALL
        .into_iter()
        .find(|k| format!("{k:?}") == name)
        .and_then(Key::key_code)
        .map(Target::Key)
        .ok_or_else(|| format!("unknown key `{name}`"))
}

/// Parse a script into its flat step list (`key`/`click` expand to
/// down + one frame + up).
pub fn parse(src: &str) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    for (n, raw) in src.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let (cmd, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        let err = |m: String| format!("line {}: {m}", n + 1);
        let num = |s: &str| s.parse::<f64>().map_err(|e| err(format!("bad number `{s}`: {e}")));
        let two = || -> Result<(f64, f64), String> {
            let mut it = rest.split_whitespace();
            let a = it.next().ok_or_else(|| err("missing arg".into()))?;
            let b = it.next().ok_or_else(|| err("missing arg".into()))?;
            Ok((num(a)?, num(b)?))
        };
        match cmd {
            "frames" => steps.push(Step::Frames(num(rest)? as u32)),
            "key" | "click" => {
                let t = parse_target(rest).map_err(err)?;
                steps.extend([Step::Down(t), Step::Frames(1), Step::Up(t), Step::Frames(1)]);
            }
            "down" => steps.push(Step::Down(parse_target(rest).map_err(err)?)),
            "up" => steps.push(Step::Up(parse_target(rest).map_err(err)?)),
            "type" => steps.push(Step::Type(rest.to_string())),
            "cursor" => {
                let (x, y) = two()?;
                steps.push(Step::Cursor(x as f32, y as f32));
            }
            "cursor_world" => {
                let (x, y) = two()?;
                steps.push(Step::CursorWorld(x as f32, y as f32));
            }
            "cursor_robot" => steps.push(Step::CursorRobot(num(rest)? as usize)),
            "mouse" => {
                let (x, y) = two()?;
                steps.push(Step::Mouse(x, y));
            }
            "wheel" => steps.push(Step::Wheel(num(rest)? as f32)),
            "shot" => steps.push(Step::Shot(rest.to_string())),
            "state" => steps.push(Step::State(rest.to_string())),
            "quit" => steps.push(Step::Quit),
            other => return Err(err(format!("unknown command `{other}`"))),
        }
    }
    Ok(steps)
}

/// The queued script plus its output directory.
pub struct Driver {
    steps: VecDeque<Step>,
    wait: u32,
    pub out_dir: PathBuf,
}

impl Driver {
    pub fn load(script: &Path, out_dir: PathBuf) -> Result<Self, String> {
        let src = std::fs::read_to_string(script)
            .map_err(|e| format!("cannot read {}: {e}", script.display()))?;
        std::fs::create_dir_all(&out_dir)
            .map_err(|e| format!("cannot create {}: {e}", out_dir.display()))?;
        Ok(Self {
            steps: parse(&src)?.into(),
            wait: 0,
            out_dir,
        })
    }

    /// The steps to apply before this frame: everything up to (and
    /// including) the next `frames N`, which then idles N-1 more frames.
    pub fn next_frame_steps(&mut self) -> Vec<Step> {
        if self.wait > 0 {
            self.wait -= 1;
            return Vec::new();
        }
        let mut out = Vec::new();
        while let Some(step) = self.steps.pop_front() {
            if let Step::Frames(n) = step {
                self.wait = n.saturating_sub(1);
                break;
            }
            // Quit on a frame of its own so a pending `shot` still renders.
            if step == Step::Quit {
                if out.is_empty() {
                    out.push(step);
                } else {
                    self.steps.push_front(step);
                }
                break;
            }
            // A shot captures this frame, before any later step applies.
            let shot = matches!(step, Step::Shot(_));
            out.push(step);
            if shot {
                break;
            }
        }
        if out.is_empty() && self.steps.is_empty() && self.wait == 0 {
            out.push(Step::Quit);
        }
        out
    }
}

pub fn physical(code: KeyCode) -> PhysicalKey {
    PhysicalKey::Code(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_expands_to_press_hold_release() {
        let steps = parse("key KeyA\nframes 3").unwrap();
        let a = Target::Key(KeyCode::KeyA);
        assert_eq!(
            steps,
            vec![Step::Down(a), Step::Frames(1), Step::Up(a), Step::Frames(1), Step::Frames(3)]
        );
    }

    #[test]
    fn frames_batches_steps_and_idles() {
        let mut d = Driver {
            steps: parse("cursor 0.5 0.5\nframes 3\nquit").unwrap().into(),
            wait: 0,
            out_dir: PathBuf::new(),
        };
        assert_eq!(d.next_frame_steps(), vec![Step::Cursor(0.5, 0.5)]);
        assert!(d.next_frame_steps().is_empty());
        assert!(d.next_frame_steps().is_empty());
        assert_eq!(d.next_frame_steps(), vec![Step::Quit]);
    }

    #[test]
    fn shot_ends_its_frame() {
        let mut d = Driver {
            steps: parse("shot a\ncursor 0.1 0.1").unwrap().into(),
            wait: 0,
            out_dir: PathBuf::new(),
        };
        assert_eq!(d.next_frame_steps(), vec![Step::Shot("a".into())]);
        assert_eq!(d.next_frame_steps(), vec![Step::Cursor(0.1, 0.1)]);
    }

    #[test]
    fn unknown_command_reports_its_line() {
        assert!(parse("frames 1\nbogus").unwrap_err().contains("line 2"));
    }
}
