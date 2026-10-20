//! The 36-entry command enum, in `eCommand` order, plus the compiled-in key
//! bindings ([`COMMAND_BINDINGS`], [`FIRE_COMMANDS`]) transcribed from the
//! original `GameData.xml`.

/// One of the 36 fixed (non-per-projectile) commands.
///
/// PARITY: variant order matches `cGameData::s_aszCommandsNames`
/// (GameEditor/cGameData.cpp:17-54) exactly, so `Command as usize` indexes
/// [`COMMAND_BINDINGS`].
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Command {
    Restart,
    EndGame,
    ToggleMap,
    ToggleProfilerRunning,
    ToggleProfilerVisible,
    /// Bound but unhandled: the profiler pane has no keyboard navigation.
    ToggleProfilerInteraction,
    IncreaseBuildingWidth,
    IncreaseBuildingHeight,
    DecreaseBuildingWidth,
    DecreaseBuildingHeight,
    MoveBuildingRight,
    MoveBuildingDown,
    MoveBuildingLeft,
    MoveBuildingUp,
    Info,
    SelectObserverRobot,
    SelectObserverPosition,
    Enter2DMode,
    Exit2DMode,
    SetObserver,
    MoveObserverForwards,
    MoveObserverBackwards,
    MoveObserverRight,
    MoveObserverLeft,
    MoveObserverUp,
    MoveObserverDown,
    MoveObserverSpeed1,
    MoveObserverSpeed2,
    MoveObserverSpeed3,
    IncreaseGameSpeed,
    DecreaseGameSpeed,
    Pause,
    ToggleAIMode,
    Accelerate,
    Decelerate,
    FireProjectiles,
}

/// A binding: key + modifier requirements. PARITY: modifiers test
/// *either*-side keys.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct CommandBinding {
    pub key: crate::input::keys::Key,
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
}

/// Compact const constructor for the binding tables below.
const fn cb(key: crate::input::keys::Key, control: bool, alt: bool, shift: bool) -> CommandBinding {
    CommandBinding {
        key,
        control,
        alt,
        shift,
    }
}

/// The 36 fixed command bindings, indexed by `Command as usize`. PARITY:
/// transcribed verbatim from the original `GameData.xml` (Ctrl+arrows resize
/// the building, Ctrl+Left-mouse drops the observer point, etc.).
pub const COMMAND_BINDINGS: [CommandBinding; 36] = {
    use crate::input::keys::Key;
    [
        cb(Key::F4, false, false, false),          // 0  Restart
        cb(Key::Escape, false, false, false),      // 1  EndGame
        cb(Key::F1, false, false, false),          // 2  ToggleMap
        cb(Key::KeyP, false, false, false),        // 3  ToggleProfilerRunning
        cb(Key::BracketLeft, false, false, false), // 4  ToggleProfilerVisible
        cb(Key::BracketRight, false, false, false), // 5 ToggleProfilerInteraction
        cb(Key::ArrowRight, true, false, false),   // 6  IncreaseBuildingWidth
        cb(Key::ArrowUp, true, false, false),      // 7  IncreaseBuildingHeight
        cb(Key::ArrowLeft, true, false, false),    // 8  DecreaseBuildingWidth
        cb(Key::ArrowDown, true, false, false),    // 9  DecreaseBuildingHeight
        cb(Key::ArrowRight, false, false, false),  // 10 MoveBuildingRight
        cb(Key::ArrowUp, false, false, false),     // 11 MoveBuildingDown
        cb(Key::ArrowLeft, false, false, false),   // 12 MoveBuildingLeft
        cb(Key::ArrowDown, false, false, false),   // 13 MoveBuildingUp
        cb(Key::F2, false, false, false),          // 14 Info
        cb(Key::MouseLeft, false, false, false),   // 15 SelectObserverRobot
        cb(Key::MouseLeft, true, false, false),    // 16 SelectObserverPosition
        cb(Key::KeyA, false, false, false),        // 17 Enter2DMode
        cb(Key::KeyS, false, false, false),        // 18 Exit2DMode
        cb(Key::KeyF, false, false, false),        // 19 SetObserver
        cb(Key::KeyE, false, false, false),        // 20 MoveObserverForwards
        cb(Key::KeyD, false, false, false),        // 21 MoveObserverBackwards
        cb(Key::KeyF, false, false, false),        // 22 MoveObserverRight
        cb(Key::KeyS, false, false, false),        // 23 MoveObserverLeft
        cb(Key::KeyR, false, false, false),        // 24 MoveObserverUp
        cb(Key::KeyV, false, false, false),        // 25 MoveObserverDown
        cb(Key::Digit1, false, false, false),      // 26 MoveObserverSpeed1
        cb(Key::Digit2, false, false, false),      // 27 MoveObserverSpeed2
        cb(Key::Digit3, false, false, false),      // 28 MoveObserverSpeed3
        cb(Key::KeyT, false, false, false),        // 29 IncreaseGameSpeed
        cb(Key::KeyG, false, false, false),        // 30 DecreaseGameSpeed
        cb(Key::KeyQ, false, false, false),        // 31 Pause
        cb(Key::KeyV, false, false, false),        // 32 ToggleAIMode
        cb(Key::KeyE, false, false, false),        // 33 Accelerate
        cb(Key::KeyD, false, false, false),        // 34 Decelerate
        cb(Key::Space, false, false, false),       // 35 FireProjectiles
    ]
};

/// Per-projectile-type fire bindings, in `GameData.projectile_types` order
/// (Bullet → Left mouse, Rocket → Right mouse, Better Bullet → Left mouse).
/// PARITY: transcribed from the original `GameData.xml` `fire_command`s.
pub const FIRE_COMMANDS: [CommandBinding; 3] = {
    use crate::input::keys::Key;
    [
        cb(Key::MouseLeft, false, false, false),  // Bullet
        cb(Key::MouseRight, false, false, false), // Rocket
        cb(Key::MouseLeft, false, false, false),  // Better Bullet
    ]
};

impl CommandBinding {
    /// PARITY (`cOperationPurge::GetCommandState`, cOperationPurge.cpp:3116-3144
    /// with the verified `bExactModifiers = false`): a *required* modifier
    /// must be held (either side); a *non-required* modifier is ignored
    /// (holding Shift never suppresses a command that doesn't ask for
    /// Shift). Formally, for each modifier m: `!required_m || down_m`.
    pub fn modifiers_satisfied(&self, input: &super::Input) -> bool {
        (!self.control || input.control_down())
            && (!self.alt || input.alt_down())
            && (!self.shift || input.shift_down())
    }

    /// `bDown`: the bound key is down AND the required modifiers are held.
    pub fn is_down(&self, input: &super::Input) -> bool {
        input.is_down(self.key) && self.modifiers_satisfied(input)
    }

    /// `bPressed`: the bound key edged down this frame AND modifiers hold.
    pub fn just_pressed(&self, input: &super::Input) -> bool {
        input.just_pressed(self.key) && self.modifiers_satisfied(input)
    }

    /// `bReleased`: the bound key edged up this frame AND modifiers hold.
    pub fn just_released(&self, input: &super::Input) -> bool {
        input.just_released(self.key) && self.modifiers_satisfied(input)
    }
}

/// The binding for a fixed command.
pub fn binding(command: Command) -> &'static CommandBinding {
    &COMMAND_BINDINGS[command as usize]
}

/// The fire binding for projectile type `index`, if it has one.
pub fn fire_binding(index: usize) -> Option<&'static CommandBinding> {
    FIRE_COMMANDS.get(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    // PARITY: pins the eCommand ABI (cGameData.cpp:17-54).
    #[test]
    fn command_discriminants_match_ecommand_abi() {
        assert_eq!(Command::Restart as usize, 0);
        assert_eq!(Command::EndGame as usize, 1);
        assert_eq!(Command::ToggleMap as usize, 2);
        assert_eq!(Command::Info as usize, 14);
        assert_eq!(Command::Enter2DMode as usize, 17);
        assert_eq!(Command::FireProjectiles as usize, COMMAND_BINDINGS.len() - 1);
    }

    use crate::input::keys::Key;
    use crate::input::Input;

    #[test]
    fn plain_binding_ignores_extra_held_modifiers() {
        // PARITY: bExactModifiers = false — a command with no required
        // modifier still fires while Shift/Ctrl/Alt happen to be down.
        let b = cb(Key::KeyF, false, false, false);
        let mut input = Input::new();
        input.on_key_event(Key::KeyF, true);
        input.on_key_event(Key::ShiftLeft, true);
        input.on_key_event(Key::ControlRight, true);
        input.begin_frame();
        assert!(b.is_down(&input));
    }

    #[test]
    fn required_modifier_must_be_held_either_side() {
        let b = cb(Key::KeyR, true, false, false); // Ctrl+R
        let mut input = Input::new();
        input.on_key_event(Key::KeyR, true);
        input.begin_frame();
        assert!(!b.is_down(&input), "no Ctrl held ⇒ inactive");

        // Right control satisfies a Ctrl requirement (either-side).
        input.on_key_event(Key::ControlRight, true);
        input.begin_frame();
        assert!(b.is_down(&input));
    }

    #[test]
    fn pressed_and_released_edges_respect_modifiers() {
        let b = cb(Key::KeyP, false, false, true); // Shift+P
        let mut input = Input::new();
        input.begin_frame(); // consume the first-frame edge suppression (FIX)
        input.on_key_event(Key::ShiftLeft, true);
        input.on_key_event(Key::KeyP, true);
        input.begin_frame();
        assert!(b.just_pressed(&input));
        assert!(b.is_down(&input));
        // Hold: no repeated press edge.
        input.begin_frame();
        assert!(!b.just_pressed(&input));
        // Release the key.
        input.on_key_event(Key::KeyP, false);
        input.begin_frame();
        assert!(b.just_released(&input));
    }

    #[test]
    fn bindings_match_the_original_gamedata_anchors() {
        assert_eq!(binding(Command::Restart).key, Key::F4);
        assert_eq!(binding(Command::EndGame).key, Key::Escape);
        assert_eq!(binding(Command::FireProjectiles).key, Key::Space);
        let w = binding(Command::IncreaseBuildingWidth);
        assert_eq!((w.key, w.control), (Key::ArrowRight, true));
        let p = binding(Command::SelectObserverPosition);
        assert_eq!((p.key, p.control), (Key::MouseLeft, true));
        assert_eq!(fire_binding(0).map(|b| b.key), Some(Key::MouseLeft));
        assert_eq!(fire_binding(1).map(|b| b.key), Some(Key::MouseRight));
        assert_eq!(fire_binding(3), None);
    }
}
