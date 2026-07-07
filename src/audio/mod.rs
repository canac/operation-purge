//! kira-backed audio manager, replacing `cSoundManager`/`cSound`
//! (design §5.4). Bodies land at M10; kept as a type stub through M0
//! (task instructions defer exercising the `kira` API).

pub mod spatial;
pub mod voice;

use crate::error::AudioError;
use std::collections::HashMap;
use std::path::PathBuf;

/// Millibels -> linear amplitude: `10^(mB/2000)` (design §5.4). Alarm
/// loops play at -1000 mB ~= 0.3162.
pub fn millibels_to_amplitude(millibels: f32) -> f32 {
    10f32.powf(millibels / 2000.0)
}

/// Preloaded clip cache, keyed by asset path (mirrors the original's
/// whole-WAV-preloaded-into-a-secondary-buffer model, source-map §3.8).
#[derive(Default)]
pub struct AudioManager {
    // Read once `load_clip()` grows a real body at M10 — see the same
    // note on `render::textures::TextureManager::cache`.
    #[allow(dead_code)]
    clips: HashMap<PathBuf, voice::VoicePool>,
}

impl AudioManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Decodes (via `hound`) and preloads a WAV into an
    /// `nSimultaneousInstances`-voice pool. Body lands at M10.
    pub fn load_clip(&mut self, _path: &std::path::Path, _voices: usize) -> Result<(), AudioError> {
        todo!("ported at M10 — hound decode + kira StaticSoundData (design §5.4)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alarm_loop_gain_matches_the_constant_in_the_design_doc() {
        // design §7: "Alarm loop volume | -1000 mB ~= 0.316 gain"
        let gain = millibels_to_amplitude(-1000.0);
        assert!((gain - 0.3162).abs() < 0.001);
    }
}
