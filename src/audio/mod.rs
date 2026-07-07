//! kira-backed audio, replacing `cSoundManager`/`cSound`/`cWaveFile`
//! (design §5.4; ported at M10 from `Sound.cpp`, `cWavefile.cpp`, and the
//! sound paths in `cOperationPurge.cpp:153-196` (create alarms),
//! `1247-1349` (listener + alarm playback), `1833-2005` (`Play3DSound`)).
//!
//! Architecture: the **voice-selection + alarm-priority logic is pure**
//! ([`voice`], [`spatial`], [`resolve_alarm`]) and unit-tested without any
//! device; playback is delegated to a [`SoundBackend`] trait so tests use a
//! fake and the real [`KiraBackend`] is only instantiated by the app. In a
//! headless environment (no audio device) the pure logic and the kira API
//! usage both compile and are tested; actual playback is deferred to the
//! user (see the M10 visual/audio gates).

pub mod spatial;
pub mod voice;

use glam::Vec3;
use spatial::AudioListener;
use std::collections::HashMap;
use voice::VoicePool;

#[cfg(not(test))]
use crate::error::AudioError;
#[cfg(not(test))]
use std::path::Path;

/// Millibels -> linear amplitude: `10^(mB/2000)` (design §5.4). Alarm
/// loops play at -1000 mB ~= 0.3162.
///
/// PARITY: `IDirectSoundBuffer::SetVolume` takes hundredths of a decibel
/// (millibels); the audible amplitude is `10^(mB/2000)` because
/// `mB/100` dB and `amplitude = 10^(dB/20)`.
pub fn millibels_to_amplitude(millibels: f32) -> f32 {
    10f32.powf(millibels / 2000.0)
}

/// The alarm-loop volume the original passes to `Play(..., -1000)`
/// (`cOperationPurge.cpp:1323,1334`).
pub const ALARM_MILLIBELS: f32 = -1000.0;

/// Alarm-loop amplitude (`= millibels_to_amplitude(-1000) ~= 0.3162`).
pub fn alarm_gain() -> f32 {
    millibels_to_amplitude(ALARM_MILLIBELS)
}

/// Convert a linear amplitude (our DS3D model's output) to the decibels
/// kira's volume API expects. Amplitudes at or below zero map to kira's
/// silence floor.
pub fn amplitude_to_decibels(amplitude: f32) -> f32 {
    if amplitude <= 0.0 {
        // kira treats -60 dB and below as silent (`Decibels::SILENCE`).
        -60.0
    } else {
        20.0 * amplitude.log10()
    }
}

/// Which alarm loop (if any) should be sounding this frame.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum Alarm {
    #[default]
    None,
    EnemyVisible,
    Targeted,
}

/// Alarm priority resolver (design §5.4; PARITY
/// `cOperationPurge.cpp:1315-1348`).
///
/// PARITY / code-over-docs (design §1.3): **EnemyVisible is checked and
/// played BEFORE Targeted**. When both are true, EnemyVisible wins and the
/// Targeted loop is stopped — the shipped code order overrides any doc that
/// implies the reverse.
pub fn resolve_alarm(enemy_visible: bool, targeted: bool) -> Alarm {
    if enemy_visible {
        Alarm::EnemyVisible
    } else if targeted {
        Alarm::Targeted
    } else {
        Alarm::None
    }
}

/// Identifies a preloaded clip's voice pool inside [`SpatialSounds`].
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub struct ClipId(pub usize);

/// Playback backend abstraction (design §5.4). The real implementation is
/// [`KiraBackend`]; tests substitute a fake so no device is touched. All
/// gains are **linear amplitudes** and all pans are in `[-1, 1]` (the
/// backend maps them to its own units).
pub trait SoundBackend {
    /// An opaque, per-play handle used to query/stop/adjust a live voice.
    type Handle;

    /// Start playing `clip` on a fresh voice at the given amplitude/pan.
    /// `looping` requests a seamless loop (the alarm loops).
    fn play(&mut self, clip: ClipId, gain: f32, pan: f32, looping: bool) -> Self::Handle;

    /// Stop a voice (mirrors `IDirectSoundBuffer::Stop`).
    fn stop(&mut self, handle: &mut Self::Handle);

    /// Retune a live voice's amplitude/pan (tweened — no per-frame crackle).
    fn set_gain_pan(&mut self, handle: &mut Self::Handle, gain: f32, pan: f32);

    /// Whether the voice is still playing (mirrors `GetStatus &
    /// DSBSTATUS_PLAYING`).
    fn is_playing(&self, handle: &Self::Handle) -> bool;
}

/// One voice slot: the live handle (if any) and the listener-relative
/// distance at which its current sound was emitted (the value the original
/// read back via `IDirectSound3DBuffer::GetPosition`).
struct VoiceSlot<H> {
    handle: Option<H>,
    distance: f32,
}

impl<H> Default for VoiceSlot<H> {
    fn default() -> Self {
        Self {
            handle: None,
            distance: 0.0,
        }
    }
}

/// The game's 3D sound manager: per-clip voice pools with the exact
/// steal/drop rules, plus the two alarm loops (design §5.4). Generic over
/// [`SoundBackend`] so the selection logic is exercised against a fake in
/// tests and the real kira backend at runtime.
pub struct SpatialSounds<B: SoundBackend> {
    backend: B,
    /// Per-clip voice slots (`pools[clip.0]`).
    pools: Vec<Vec<VoiceSlot<B::Handle>>>,
    /// EnemyVisible / Targeted alarm clips + live loop handles.
    enemy_visible_clip: Option<ClipId>,
    targeted_clip: Option<ClipId>,
    enemy_alarm: Option<B::Handle>,
    targeted_alarm: Option<B::Handle>,
    current_alarm: Alarm,
    /// Test/debug hook: how many 3D sounds were **dropped** by the
    /// farthest-steal rule (design §6 accept: "logged assertion in a test
    /// hook" for the 3D drop case).
    dropped_3d: u64,
}

impl<B: SoundBackend> SpatialSounds<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            pools: Vec::new(),
            enemy_visible_clip: None,
            targeted_clip: None,
            enemy_alarm: None,
            targeted_alarm: None,
            current_alarm: Alarm::None,
            dropped_3d: 0,
        }
    }

    /// Register a preloaded clip with an `n`-voice pool
    /// (`nSimultaneousInstances`). Returns its [`ClipId`].
    pub fn register_clip(&mut self, voices: usize) -> ClipId {
        let id = ClipId(self.pools.len());
        let mut pool = Vec::with_capacity(voices);
        pool.resize_with(voices, VoiceSlot::default);
        self.pools.push(pool);
        id
    }

    /// Designate the two alarm clips (`m_pEnemyVisibleSound`,
    /// `m_pTargetedSound`).
    pub fn set_alarm_clips(&mut self, enemy_visible: Option<ClipId>, targeted: Option<ClipId>) {
        self.enemy_visible_clip = enemy_visible;
        self.targeted_clip = targeted;
    }

    pub fn dropped_3d_count(&self) -> u64 {
        self.dropped_3d
    }

    pub fn current_alarm(&self) -> Alarm {
        self.current_alarm
    }

    /// Snapshot the pool's selection state, syncing `playing` from the
    /// backend (the original queried `GetStatus` per buffer).
    fn snapshot(&self, clip: ClipId) -> VoicePool {
        VoicePool {
            voices: self.pools[clip.0]
                .iter()
                .map(|s| voice::Voice {
                    playing: s
                        .handle
                        .as_ref()
                        .is_some_and(|h| self.backend.is_playing(h)),
                    distance_from_listener: s.distance,
                })
                .collect(),
        }
    }

    /// Play a fire-and-forget 3D sound at `origin` (`Play3DSound`,
    /// `cOperationPurge.cpp:1833-2005`). Selects a voice via the
    /// farthest-steal rule; if the new sound is the farthest, it is
    /// **dropped** (PARITY) and `dropped_3d` is incremented.
    ///
    /// PARITY note: the original lazily created a `cSound` on first play
    /// (then `Play3D` -> `GetFreeBuffer` -> first idle). Preloading the pool
    /// makes every voice start idle, so [`VoicePool::play_3d`]'s
    /// first-idle-wins branch reproduces that first-play behavior exactly —
    /// the lazy-create branch collapses into the general path.
    pub fn play_3d(&mut self, clip: ClipId, origin: Vec3, listener: &AudioListener) {
        let distance = listener.distance_to(origin);
        match self.snapshot(clip).play_3d(distance) {
            Some(i) => {
                let (gain, pan) = listener.spatialize(origin);
                if let Some(mut old) = self.pools[clip.0][i].handle.take() {
                    self.backend.stop(&mut old);
                }
                let handle = self.backend.play(clip, gain, pan, false);
                self.pools[clip.0][i] = VoiceSlot {
                    handle: Some(handle),
                    distance,
                };
            }
            None => {
                // The new sound is farther than every playing voice — dropped.
                self.dropped_3d += 1;
            }
        }
    }

    /// Play a non-3D one-shot (`cSound::Play` -> `GetFreeBuffer`,
    /// `Sound.cpp:238-246`): first idle voice, else steal a random busy one
    /// via `pick_random(n)` (`rand() % n`). Never drops.
    pub fn play_non_3d(&mut self, clip: ClipId, gain: f32, pan: f32, pick_random: impl FnOnce(usize) -> usize) {
        if let Some(i) = self.snapshot(clip).get_free_buffer(pick_random) {
            if let Some(mut old) = self.pools[clip.0][i].handle.take() {
                self.backend.stop(&mut old);
            }
            let handle = self.backend.play(clip, gain, pan, false);
            self.pools[clip.0][i] = VoiceSlot {
                handle: Some(handle),
                distance: 0.0,
            };
        }
    }

    /// Drive the two alarm loops for this frame (design §5.4; PARITY
    /// `cOperationPurge.cpp:1315-1348`). EnemyVisible is resolved BEFORE
    /// Targeted; the non-selected alarm is stopped; the selected alarm is
    /// (re)started only if not already playing (`!IsSoundPlaying()`).
    pub fn update_alarms(&mut self, enemy_visible: bool, targeted: bool) {
        let target = resolve_alarm(enemy_visible, targeted);
        match target {
            Alarm::EnemyVisible => {
                self.stop_targeted_alarm();
                self.start_enemy_alarm_if_idle();
            }
            Alarm::Targeted => {
                self.stop_enemy_alarm();
                self.start_targeted_alarm_if_idle();
            }
            Alarm::None => {
                self.stop_enemy_alarm();
                self.stop_targeted_alarm();
            }
        }
        self.current_alarm = target;
    }

    fn start_enemy_alarm_if_idle(&mut self) {
        let already = self
            .enemy_alarm
            .as_ref()
            .is_some_and(|h| self.backend.is_playing(h));
        if already {
            return;
        }
        if let Some(clip) = self.enemy_visible_clip {
            if let Some(mut old) = self.enemy_alarm.take() {
                self.backend.stop(&mut old);
            }
            self.enemy_alarm = Some(self.backend.play(clip, alarm_gain(), 0.0, true));
        }
    }

    fn start_targeted_alarm_if_idle(&mut self) {
        let already = self
            .targeted_alarm
            .as_ref()
            .is_some_and(|h| self.backend.is_playing(h));
        if already {
            return;
        }
        if let Some(clip) = self.targeted_clip {
            if let Some(mut old) = self.targeted_alarm.take() {
                self.backend.stop(&mut old);
            }
            self.targeted_alarm = Some(self.backend.play(clip, alarm_gain(), 0.0, true));
        }
    }

    fn stop_enemy_alarm(&mut self) {
        if let Some(mut h) = self.enemy_alarm.take() {
            self.backend.stop(&mut h);
        }
    }

    fn stop_targeted_alarm(&mut self) {
        if let Some(mut h) = self.targeted_alarm.take() {
            self.backend.stop(&mut h);
        }
    }

    /// Stop every voice and both alarm loops. This is the ordering hook the
    /// app must call **before entering any modal state** (design §5.4;
    /// PARITY `cOperationPurge.cpp:945-951,2479-2485` stop the alarms before
    /// popping a MessageBox). Modal states themselves are M11.
    pub fn stop_all(&mut self) {
        for pool in &mut self.pools {
            for slot in pool.iter_mut() {
                if let Some(mut h) = slot.handle.take() {
                    self.backend.stop(&mut h);
                }
            }
        }
        self.stop_enemy_alarm();
        self.stop_targeted_alarm();
        self.current_alarm = Alarm::None;
    }
}

// ===================== hound decode (cWaveFile) =====================

/// Decode a WAV file into a kira `StaticSoundData` (design §5.4).
///
/// Every shipped clip is normalized offline to the standard format —
/// **16-bit signed PCM, mono** (see the README) — so this reads `i16` samples
/// directly with no per-format branch. kira resamples each clip's native
/// sample rate to the device rate at playback. This preserves the original's
/// behavior: `cWaveFile` loaded whole mono PCM into a DirectSound buffer.
#[cfg(not(test))]
pub fn decode_wav(path: &Path) -> Result<kira::sound::static_sound::StaticSoundData, AudioError> {
    use kira::Frame;

    let mut reader = hound::WavReader::open(path).map_err(|e| map_hound_err(path, e))?;
    let sample_rate = reader.spec().sample_rate;
    let frames: Vec<Frame> = reader
        .samples::<i16>()
        .map(|s| s.map(|v| Frame::from_mono(v as f32 / 32768.0)))
        .collect::<Result<_, _>>()
        .map_err(|e| map_hound_err(path, e))?;

    Ok(kira::sound::static_sound::StaticSoundData {
        sample_rate,
        frames: frames.into(),
        settings: kira::sound::static_sound::StaticSoundSettings::default(),
        slice: None,
    })
}

#[cfg(not(test))]
fn map_hound_err(path: &Path, err: hound::Error) -> AudioError {
    match err {
        hound::Error::IoError(io) => AudioError::Io {
            path: path.to_path_buf(),
            source: io,
        },
        other => AudioError::Decode {
            path: path.to_path_buf(),
            message: other.to_string(),
        },
    }
}

// ===================== kira backend (runtime only) =====================

/// The real [`SoundBackend`], backed by kira. Constructed by the app at
/// runtime — **never** in headless tests (its `new` opens an audio device).
/// Compiled (type-checked) here so the kira API usage is verified; playback
/// is deferred to the user.
#[cfg(not(test))]
pub struct KiraBackend {
    manager: kira::AudioManager<kira::DefaultBackend>,
    /// Preloaded clip data, indexed by `ClipId`.
    clips: Vec<kira::sound::static_sound::StaticSoundData>,
    /// Preload cache keyed by path so a clip is decoded once.
    by_path: HashMap<std::path::PathBuf, ClipId>,
}

#[cfg(not(test))]
impl KiraBackend {
    /// Open the default output device (app-only; never called in tests).
    pub fn new() -> Result<Self, AudioError> {
        let manager = kira::AudioManager::<kira::DefaultBackend>::new(
            kira::AudioManagerSettings::default(),
        )
        .map_err(|e| AudioError::Backend(e.to_string()))?;
        Ok(Self {
            manager,
            clips: Vec::new(),
            by_path: HashMap::new(),
        })
    }

    /// Preload a WAV, returning a stable [`ClipId`] (deduped by path). The
    /// caller pairs this with `SpatialSounds::register_clip(voices)`.
    pub fn preload(&mut self, path: &Path) -> Result<ClipId, AudioError> {
        if let Some(id) = self.by_path.get(path) {
            return Ok(*id);
        }
        let data = decode_wav(path)?;
        let id = ClipId(self.clips.len());
        self.clips.push(data);
        self.by_path.insert(path.to_path_buf(), id);
        Ok(id)
    }
}

#[cfg(not(test))]
impl SoundBackend for KiraBackend {
    type Handle = kira::sound::static_sound::StaticSoundHandle;

    fn play(&mut self, clip: ClipId, gain: f32, pan: f32, looping: bool) -> Self::Handle {
        let mut data = self.clips[clip.0]
            .volume(amplitude_to_decibels(gain))
            .panning(pan);
        if looping {
            data = data.loop_region(..);
        }
        // Deferred to the user; a play failure at runtime is logged upstream.
        self.manager.play(data).expect("kira play")
    }

    fn stop(&mut self, handle: &mut Self::Handle) {
        handle.stop(kira::Tween::default());
    }

    fn set_gain_pan(&mut self, handle: &mut Self::Handle, gain: f32, pan: f32) {
        handle.set_volume(amplitude_to_decibels(gain), kira::Tween::default());
        handle.set_panning(pan, kira::Tween::default());
    }

    fn is_playing(&self, handle: &Self::Handle) -> bool {
        handle.state() == kira::sound::PlaybackState::Playing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn alarm_loop_gain_matches_the_constant_in_the_design_doc() {
        // design §7: "Alarm loop volume | -1000 mB ~= 0.316 gain"
        let gain = millibels_to_amplitude(-1000.0);
        assert!((gain - 0.3162).abs() < 0.001);
        assert!((alarm_gain() - 0.3162).abs() < 0.001);
    }

    #[test]
    fn gain_conversion_millibels_amplitude_decibels_roundtrip() {
        // -1000 mB == -10 dB == 0.3162 amplitude.
        assert!((amplitude_to_decibels(millibels_to_amplitude(-1000.0)) + 10.0).abs() < 1e-4);
        // 0 mB == unity.
        assert_eq!(millibels_to_amplitude(0.0), 1.0);
        assert_eq!(amplitude_to_decibels(1.0), 0.0);
        // Silence floor.
        assert_eq!(amplitude_to_decibels(0.0), -60.0);
    }

    #[test]
    fn alarm_priority_enemy_visible_outranks_targeted() {
        // PARITY: EnemyVisible checked/played BEFORE Targeted (code over docs).
        assert_eq!(resolve_alarm(true, true), Alarm::EnemyVisible);
        assert_eq!(resolve_alarm(true, false), Alarm::EnemyVisible);
        assert_eq!(resolve_alarm(false, true), Alarm::Targeted);
        assert_eq!(resolve_alarm(false, false), Alarm::None);
    }

    // ---- fake backend: tracks live handles + play log, no device ----

    #[derive(Default)]
    struct FakeState {
        next: u64,
        /// handle id -> still playing
        playing: HashMap<u64, bool>,
        /// (clip, gain, pan, looping) per play call, in order.
        log: Vec<(ClipId, f32, f32, bool)>,
    }

    #[derive(Clone)]
    struct FakeBackend {
        state: Rc<RefCell<FakeState>>,
    }

    struct FakeHandle {
        id: u64,
        state: Rc<RefCell<FakeState>>,
    }

    impl FakeBackend {
        fn new() -> Self {
            Self {
                state: Rc::new(RefCell::new(FakeState::default())),
            }
        }
        fn play_count(&self) -> usize {
            self.state.borrow().log.len()
        }
        fn last_play(&self) -> (ClipId, f32, f32, bool) {
            *self.state.borrow().log.last().unwrap()
        }
    }

    impl SoundBackend for FakeBackend {
        type Handle = FakeHandle;
        fn play(&mut self, clip: ClipId, gain: f32, pan: f32, looping: bool) -> Self::Handle {
            let mut s = self.state.borrow_mut();
            s.next += 1;
            let id = s.next;
            s.playing.insert(id, true);
            s.log.push((clip, gain, pan, looping));
            FakeHandle {
                id,
                state: self.state.clone(),
            }
        }
        fn stop(&mut self, handle: &mut Self::Handle) {
            self.state.borrow_mut().playing.insert(handle.id, false);
        }
        fn set_gain_pan(&mut self, _handle: &mut Self::Handle, _gain: f32, _pan: f32) {}
        fn is_playing(&self, handle: &Self::Handle) -> bool {
            *self.state.borrow().playing.get(&handle.id).unwrap_or(&false)
        }
    }

    impl Drop for FakeHandle {
        fn drop(&mut self) {
            // Dropping a slot's handle (on steal/stop) leaves the play log intact.
            let _ = &self.state;
        }
    }

    fn listener_at_origin() -> AudioListener {
        AudioListener::from_pose(glam::Vec2::ZERO, 0.0)
    }

    #[test]
    fn manager_3d_farthest_steal_or_drop_including_the_drop_case() {
        let backend = FakeBackend::new();
        let probe = backend.clone();
        let mut mgr = SpatialSounds::new(backend);
        let clip = mgr.register_clip(2); // nSimultaneousInstances = 2
        let l = listener_at_origin();

        // Two shots fill both idle voices (first-idle-wins).
        mgr.play_3d(clip, Vec3::new(5.0, 0.0, 0.0), &l); // dist 5
        mgr.play_3d(clip, Vec3::new(0.0, 8.0, 0.0), &l); // dist 8
        assert_eq!(probe.play_count(), 2);
        assert_eq!(mgr.dropped_3d_count(), 0);

        // THE DROP CASE (design blocker item): a third shot at dist 20 is
        // FARTHER than both playing voices (5, 8) — the new sound is the
        // farthest, so it is silently dropped (no steal).
        mgr.play_3d(clip, Vec3::new(20.0, 0.0, 0.0), &l); // dist 20
        assert_eq!(probe.play_count(), 2, "farthest new sound must be dropped");
        assert_eq!(mgr.dropped_3d_count(), 1);

        // A fourth shot at dist 3 is NEARER than both — a strictly-farther
        // playing voice exists (dist 8), so it steals that voice and plays.
        mgr.play_3d(clip, Vec3::new(0.0, 3.0, 0.0), &l); // dist 3, steals dist-8 voice
        assert_eq!(probe.play_count(), 3);
        assert_eq!(mgr.dropped_3d_count(), 1);

        // Voices now at dist 5 and dist 3. Another far shot at dist 50 is the
        // farthest again => dropped.
        mgr.play_3d(clip, Vec3::new(50.0, 0.0, 0.0), &l);
        assert_eq!(probe.play_count(), 3);
        assert_eq!(mgr.dropped_3d_count(), 2);
    }

    #[test]
    fn manager_non_3d_random_steal_after_n_shots() {
        let backend = FakeBackend::new();
        let probe = backend.clone();
        let mut mgr = SpatialSounds::new(backend);
        let clip = mgr.register_clip(3);

        // First 3 shots fill the 3 idle voices (pick_random never consulted).
        for _ in 0..3 {
            mgr.play_non_3d(clip, 1.0, 0.0, |_| panic!("should not steal yet"));
        }
        assert_eq!(probe.play_count(), 3);

        // 4th shot: all busy => random steal at the supplied index (1).
        mgr.play_non_3d(clip, 1.0, 0.0, |n| 1 % n);
        assert_eq!(probe.play_count(), 4);
    }

    #[test]
    fn manager_3d_gain_and_pan_track_listener_geometry() {
        let backend = FakeBackend::new();
        let probe = backend.clone();
        let mut mgr = SpatialSounds::new(backend);
        let clip = mgr.register_clip(1);
        let l = listener_at_origin(); // at origin, facing +X

        // Source due +Y at dist 20: gain = 10/20 = 0.5, pan hard right (+1).
        mgr.play_3d(clip, Vec3::new(0.0, 20.0, 0.0), &l);
        let (c, gain, pan, looping) = probe.last_play();
        assert_eq!(c, clip);
        assert!(!looping);
        assert!((gain - 0.5).abs() < 1e-6, "gain = {gain}");
        assert!((pan - 1.0).abs() < 1e-6, "pan = {pan}");
    }

    #[test]
    fn manager_alarm_priority_and_stop_semantics() {
        let backend = FakeBackend::new();
        let probe = backend.clone();
        let mut mgr = SpatialSounds::new(backend);
        let enemy = mgr.register_clip(1);
        let targeted = mgr.register_clip(1);
        mgr.set_alarm_clips(Some(enemy), Some(targeted));

        // Both flags => EnemyVisible wins; one looping alarm at 0.3162 starts.
        mgr.update_alarms(true, true);
        assert_eq!(mgr.current_alarm(), Alarm::EnemyVisible);
        assert_eq!(probe.play_count(), 1);
        let (c, gain, _pan, looping) = probe.last_play();
        assert_eq!(c, enemy);
        assert!(looping);
        assert!((gain - 0.3162).abs() < 0.001);

        // Still EnemyVisible next frame => NOT restarted (already playing).
        mgr.update_alarms(true, false);
        assert_eq!(probe.play_count(), 1);

        // Drop to Targeted => enemy stops, targeted loop starts.
        mgr.update_alarms(false, true);
        assert_eq!(mgr.current_alarm(), Alarm::Targeted);
        assert_eq!(probe.play_count(), 2);
        assert_eq!(probe.last_play().0, targeted);

        // Clear => both stop.
        mgr.update_alarms(false, false);
        assert_eq!(mgr.current_alarm(), Alarm::None);
    }

    #[test]
    fn stop_all_is_the_pre_modal_hook() {
        let backend = FakeBackend::new();
        let probe = backend.clone();
        let mut mgr = SpatialSounds::new(backend);
        let clip = mgr.register_clip(2);
        let enemy = mgr.register_clip(1);
        mgr.set_alarm_clips(Some(enemy), None);
        let l = listener_at_origin();

        mgr.play_3d(clip, Vec3::new(1.0, 0.0, 0.0), &l);
        mgr.update_alarms(true, false);
        // A voice and the alarm are live.
        mgr.stop_all();
        assert_eq!(mgr.current_alarm(), Alarm::None);

        // After stop_all every voice reads idle, so the next 3D play takes the
        // first (now-idle) voice rather than stealing.
        let before = probe.play_count();
        mgr.play_3d(clip, Vec3::new(1.0, 0.0, 0.0), &l);
        assert_eq!(probe.play_count(), before + 1);
    }
}
