//! The game's sound pass: preloads every clip once, then each frame feeds
//! the world's sound events, the DS3D listener and the alarm loops to
//! [`SpatialSounds`] (`cOperationPurge.cpp:1247-1349`).

use std::collections::HashMap;
use std::path::Path;

use glam::{Vec2, Vec3};

use super::spatial::{AudioListener, LISTENER_UPDATE_INTERVAL_FRAMES};
use super::{decode_wav, sound_capacity, ClipId, KiraBackend, SpatialSounds};
use crate::data::assets::names;
use crate::data::GameData;
use crate::game::{Game, ObserverType};
use crate::sim::world::SoundKind;

pub struct GameAudio {
    sounds: SpatialSounds<KiraBackend>,
    /// `iFrame`, a function-local static in `FrameChange`, so it carries
    /// over between games like the DS3D listener itself.
    listener_frame: u32,
    listener: AudioListener,
    clips: HashMap<&'static str, ClipId>,
    /// One-shots started since launch (E2E state dump).
    played: u64,
}

impl GameAudio {
    /// Open the device, preload every alarm + per-event clip, and register a
    /// voice pool (`nSimultaneousInstances`) per clip. Returns `None` (muted)
    /// when no device opens, so the game still runs.
    pub fn open(sounds_dir: &Path, data: &GameData) -> Option<Self> {
        let voices = data.sounds.simultaneous_instances.max(1) as usize;
        let backend = match KiraBackend::new(sound_capacity(names::ALL_SOUNDS.len(), voices)) {
            Ok(b) => b,
            Err(e) => {
                crate::log_error!("audio disabled (no device): {e}");
                return None;
            }
        };

        let mut sounds = SpatialSounds::new(backend);
        let mut clips = HashMap::new();
        for &name in names::ALL_SOUNDS {
            let path = sounds_dir.join(name);
            match decode_wav(&path) {
                Ok(data) => {
                    clips.insert(name, sounds.register_clip(data, voices));
                }
                Err(e) => crate::log_error!("could not preload {}: {e}", path.display()),
            }
        }
        sounds.set_alarm_clips(
            clips.get(names::ENEMY_VISIBLE_SOUND).copied(),
            clips.get(names::TARGETED_SOUND).copied(),
        );

        Some(Self {
            sounds,
            listener_frame: 0,
            listener: AudioListener::default(),
            clips,
            played: 0,
        })
    }

    /// Refresh the DS3D listener (every 10th frame), play this frame's
    /// sound events, and drive the alarm loops.
    pub fn frame(&mut self, game: &mut Game) {
        self.listener_frame = (self.listener_frame + 1) % LISTENER_UPDATE_INTERVAL_FRAMES;
        if self.listener_frame == 0 {
            let (pos, rot) = listener_pose(game);
            self.listener = AudioListener::from_pose(pos, rot);
        }

        let live = live_listener(game);
        for ev in std::mem::take(&mut game.world.sound_events) {
            let name = match ev.kind {
                SoundKind::Fire(t) => names::projectile_sounds(t.get() as usize).0,
                SoundKind::Collision(t) => names::projectile_sounds(t.get() as usize).1,
                SoundKind::Destroy => names::DESTROY_SOUND,
            };
            if let Some(&clip) = self.clips.get(name) {
                self.sounds.play_3d(clip, ev.origin, &self.listener, live);
                self.played += 1;
            }
        }

        // Alarms only sound in 3D; the 2D map and modals silence them.
        if game.sim_frozen() {
            self.sounds.update_alarms(false, false);
        } else {
            self.sounds.update_alarms(game.enemy_visible, game.targeted);
        }
    }

    pub fn debug_line(&self) -> String {
        format!("sounds_played={} alarm={:?}", self.played, self.sounds.current_alarm())
    }

    /// Stop every voice + alarm (`StopAllSounds`), before a modal opens or a
    /// new game starts.
    pub fn stop_all(&mut self) {
        self.sounds.update_alarms(false, false);
        self.sounds.stop_all();
    }
}

/// The DS3D listener pose (`cOperationPurge.cpp:1277-1285`): the observer
/// point, else the active robot.
fn listener_pose(game: &Game) -> (Vec2, f32) {
    match game.observer_type {
        ObserverType::Point => (game.observer_point.truncate(), game.observer_theta),
        _ => game
            .active_robot
            .and_then(|id| game.world.robots.get(&id))
            .map(|r| (r.position, r.direction))
            .unwrap_or((Vec2::ZERO, 0.0)),
    }
}

/// Where the 3D voice-steal rule measures from this frame
/// (`cOperationPurge.cpp:1911-1920`): the observer point, else the active robot.
fn live_listener(game: &Game) -> Vec3 {
    match game.observer_type {
        ObserverType::Point => game.observer_point,
        _ => listener_pose(game).0.extend(0.0),
    }
}
