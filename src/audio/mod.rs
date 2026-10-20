//! kira-backed audio, replacing `cSoundManager`/`cSound`/`cWaveFile`
//! (`Sound.cpp`, `cWavefile.cpp`, and the sound paths in
//! `cOperationPurge.cpp:153-196` (create alarms), `1247-1349` (listener +
//! alarm playback), `1833-2005` (`Play3DSound`)).
//!
//! The voice-selection and alarm-priority logic is pure ([`voice`],
//! [`spatial`], [`resolve_alarm`]); playback goes through the
//! [`SoundBackend`] trait so tests use a fake and only the app opens a
//! device via [`KiraBackend`].

pub mod spatial;
pub mod voice;

use crate::error::AudioError;
use glam::Vec3;
use spatial::AudioListener;
use std::path::Path;
use voice::VoicePool;

/// Millibels -> linear amplitude: `10^(mB/2000)`. Alarm loops play at
/// -1000 mB ~= 0.3162.
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

/// Alarm priority resolver (PARITY `cOperationPurge.cpp:1315-1348`).
///
/// PARITY / code-over-docs: **EnemyVisible is checked and
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

/// A clip registered with [`SpatialSounds::register_clip`]: indexes both
/// the backend's clip store and the voice pools.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub struct ClipId(pub usize);

/// Playback backend abstraction. The real implementation is
/// [`KiraBackend`]; tests substitute a fake so no device is touched. All
/// gains are **linear amplitudes** and all pans are in `[-1, 1]` (the
/// backend maps them to its own units).
pub trait SoundBackend {
    /// An opaque, per-play handle used to query/stop/adjust a live voice.
    type Handle;
    /// Decoded clip data the backend stores.
    type Clip;

    /// Store `clip` as the next dense [`ClipId`]. Only
    /// [`SpatialSounds::register_clip`] calls this, which keeps backend clip
    /// ids and voice-pool ids in lockstep.
    fn add_clip(&mut self, clip: Self::Clip);

    /// Start playing `clip` on a fresh voice at the given amplitude/pan.
    /// `looping` requests a seamless loop (the alarm loops). `None` means
    /// the backend could not start it (already logged); the caller skips it.
    fn play(&mut self, clip: ClipId, gain: f32, pan: f32, looping: bool) -> Option<Self::Handle>;

    /// Stop a voice (mirrors `IDirectSoundBuffer::Stop`).
    fn stop(&mut self, handle: &mut Self::Handle);

    /// Retune a live voice's amplitude/pan (tweened — no per-frame crackle).
    fn set_gain_pan(&mut self, handle: &mut Self::Handle, gain: f32, pan: f32);

    /// Whether the voice is still playing (mirrors `GetStatus &
    /// DSBSTATUS_PLAYING`).
    fn is_playing(&self, handle: &Self::Handle) -> bool;
}

/// One voice slot: the live handle (if any) and the world position its
/// current sound was emitted from (what the original read back via
/// `IDirectSound3DBuffer::GetPosition`).
struct VoiceSlot<H> {
    handle: Option<H>,
    origin: Vec3,
}

impl<H> Default for VoiceSlot<H> {
    fn default() -> Self {
        Self {
            handle: None,
            origin: Vec3::ZERO,
        }
    }
}

/// The game's 3D sound manager: per-clip voice pools with the exact
/// steal/drop rules, plus the two alarm loops. Generic over
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
    /// How many 3D sounds the farthest-steal rule **dropped**.
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

    /// Hand `clip` to the backend and give it a `voices`-slot pool
    /// (`nSimultaneousInstances`). Returns its [`ClipId`].
    pub fn register_clip(&mut self, clip: B::Clip, voices: usize) -> ClipId {
        let id = ClipId(self.pools.len());
        self.backend.add_clip(clip);
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

    /// Snapshot the pool's selection state: `playing` from the backend (the
    /// original queried `GetStatus` per buffer) and each voice's distance
    /// from `live_listener`, measured now.
    fn snapshot(&self, clip: ClipId, live_listener: Vec3) -> VoicePool {
        VoicePool {
            voices: self.pools[clip.0]
                .iter()
                .map(|s| voice::Voice {
                    playing: s
                        .handle
                        .as_ref()
                        .is_some_and(|h| self.backend.is_playing(h)),
                    distance_from_listener: s.origin.distance(live_listener),
                })
                .collect(),
        }
    }

    /// Play a fire-and-forget 3D sound at `origin` (`Play3DSound`,
    /// `cOperationPurge.cpp:1833-2005`). Selects a voice via the
    /// farthest-steal rule; if the new sound is the farthest, it is
    /// **dropped** (PARITY) and `dropped_3d` is incremented.
    ///
    /// PARITY: the steal decision compares distances from `live_listener`,
    /// the active robot / observer position *this frame*
    /// (`cOperationPurge.cpp:1911-1920`); `listener` is the DS3D pose,
    /// refreshed every 10th frame, used only for gain and pan.
    ///
    /// PARITY note: the original lazily created a `cSound` on first play
    /// (then `Play3D` -> `GetFreeBuffer` -> first idle). Preloading the pool
    /// makes every voice start idle, so [`VoicePool::play_3d`]'s
    /// first-idle-wins branch reproduces that first-play behavior exactly —
    /// the lazy-create branch collapses into the general path.
    pub fn play_3d(
        &mut self,
        clip: ClipId,
        origin: Vec3,
        listener: &AudioListener,
        live_listener: Vec3,
    ) {
        let distance = origin.distance(live_listener);
        match self.snapshot(clip, live_listener).play_3d(distance) {
            Some(i) => {
                let (gain, pan) = listener.spatialize(origin);
                if let Some(mut old) = self.pools[clip.0][i].handle.take() {
                    self.backend.stop(&mut old);
                }
                let handle = self.backend.play(clip, gain, pan, false);
                self.pools[clip.0][i] = VoiceSlot { handle, origin };
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
        if let Some(i) = self.snapshot(clip, Vec3::ZERO).get_free_buffer(pick_random) {
            if let Some(mut old) = self.pools[clip.0][i].handle.take() {
                self.backend.stop(&mut old);
            }
            let handle = self.backend.play(clip, gain, pan, false);
            self.pools[clip.0][i] = VoiceSlot {
                handle,
                origin: Vec3::ZERO,
            };
        }
    }

    /// Drive the two alarm loops for this frame (PARITY
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
            self.enemy_alarm = self.backend.play(clip, alarm_gain(), 0.0, true);
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
            self.targeted_alarm = self.backend.play(clip, alarm_gain(), 0.0, true);
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

    /// Stop every voice and both alarm loops. The app must call this
    /// **before entering any modal state** (PARITY
    /// `cOperationPurge.cpp:945-951,2479-2485` stop the alarms before
    /// popping a MessageBox).
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

/// Upper bound on simultaneously live kira sounds for `clips` pools of
/// `voices` each plus the two alarm loops. Doubled because a stolen voice
/// keeps its slot while its stop tween fades out.
pub fn sound_capacity(clips: usize, voices: usize) -> usize {
    2 * (clips * voices + 2)
}

/// Decode a WAV file into a kira `StaticSoundData`.
///
/// Every shipped clip is normalized offline to 16-bit signed PCM mono (see
/// the README), so this reads `i16` samples with no per-format branch; kira
/// resamples to the device rate at playback.
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

/// The real [`SoundBackend`], backed by kira. Only the app constructs it,
/// since `new` opens the default output device.
pub struct KiraBackend {
    manager: kira::AudioManager<kira::DefaultBackend>,
    /// Indexed by `ClipId`.
    clips: Vec<kira::sound::static_sound::StaticSoundData>,
}

impl KiraBackend {
    /// Open the default output device with room for `sound_capacity` live
    /// sounds (see [`sound_capacity`]).
    pub fn new(sound_capacity: usize) -> Result<Self, AudioError> {
        let settings = kira::AudioManagerSettings {
            main_track_builder: kira::track::MainTrackBuilder::new()
                .sound_capacity(sound_capacity),
            ..Default::default()
        };
        let manager = kira::AudioManager::<kira::DefaultBackend>::new(settings)
            .map_err(|e| AudioError::Backend(e.to_string()))?;
        Ok(Self {
            manager,
            clips: Vec::new(),
        })
    }
}

impl SoundBackend for KiraBackend {
    type Handle = kira::sound::static_sound::StaticSoundHandle;
    type Clip = kira::sound::static_sound::StaticSoundData;

    fn add_clip(&mut self, clip: Self::Clip) {
        self.clips.push(clip);
    }

    fn play(&mut self, clip: ClipId, gain: f32, pan: f32, looping: bool) -> Option<Self::Handle> {
        let mut data = self.clips[clip.0]
            .volume(amplitude_to_decibels(gain))
            .panning(pan);
        if looping {
            data = data.loop_region(..);
        }
        match self.manager.play(data) {
            Ok(handle) => Some(handle),
            Err(e) => {
                crate::log_error!("audio: could not play clip {}: {e}", clip.0);
                None
            }
        }
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
    use std::collections::HashMap;
    use std::rc::Rc;

    #[test]
    fn alarm_loop_gain_is_minus_1000_millibels() {
        let gain = millibels_to_amplitude(-1000.0);
        assert!((gain - 0.3162).abs() < 0.001);
        assert!((alarm_gain() - 0.3162).abs() < 0.001);
    }

    #[test]
    fn gain_conversion_millibels_amplitude_decibels_roundtrip() {
        // -1000 mB == -10 dB == 0.3162 amplitude.
        assert!((amplitude_to_decibels(millibels_to_amplitude(-1000.0)) + 10.0).abs() < 1e-4);
        assert_eq!(millibels_to_amplitude(0.0), 1.0);
        assert_eq!(amplitude_to_decibels(1.0), 0.0);
        assert_eq!(amplitude_to_decibels(0.0), -60.0);
    }

    #[test]
    fn alarm_priority_enemy_visible_outranks_targeted() {
        assert_eq!(resolve_alarm(true, true), Alarm::EnemyVisible);
        assert_eq!(resolve_alarm(true, false), Alarm::EnemyVisible);
        assert_eq!(resolve_alarm(false, true), Alarm::Targeted);
        assert_eq!(resolve_alarm(false, false), Alarm::None);
    }

    #[test]
    fn decode_wav_reads_a_shipped_clip() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/Sounds/BulletFire.wav");
        let data = decode_wav(&path).unwrap();
        assert!(data.sample_rate > 0);
        assert!(!data.frames.is_empty());
        let f = data.frames[data.frames.len() / 2];
        assert_eq!(f.left, f.right, "mono decodes to identical channels");
        assert!(matches!(
            decode_wav(Path::new("no/such.wav")),
            Err(AudioError::Io { .. })
        ));
    }

    #[test]
    fn capacity_covers_every_pool_voice_twice_plus_alarms() {
        assert_eq!(sound_capacity(7, 10), 144);
    }

    #[derive(Default)]
    struct FakeState {
        next: u64,
        clips: Vec<&'static str>,
        /// handle id -> still playing
        playing: HashMap<u64, bool>,
        /// (clip name, gain, pan, looping) per successful play, in order.
        log: Vec<(&'static str, f32, f32, bool)>,
        refuse_plays: bool,
    }

    #[derive(Clone, Default)]
    struct FakeBackend {
        state: Rc<RefCell<FakeState>>,
    }

    struct FakeHandle(u64);

    impl FakeBackend {
        fn play_count(&self) -> usize {
            self.state.borrow().log.len()
        }
        fn last_play(&self) -> (&'static str, f32, f32, bool) {
            *self.state.borrow().log.last().unwrap()
        }
    }

    impl SoundBackend for FakeBackend {
        type Handle = FakeHandle;
        type Clip = &'static str;
        fn add_clip(&mut self, clip: Self::Clip) {
            self.state.borrow_mut().clips.push(clip);
        }
        fn play(&mut self, clip: ClipId, gain: f32, pan: f32, looping: bool) -> Option<FakeHandle> {
            let mut s = self.state.borrow_mut();
            if s.refuse_plays {
                return None;
            }
            s.next += 1;
            let id = s.next;
            s.playing.insert(id, true);
            let name = s.clips[clip.0];
            s.log.push((name, gain, pan, looping));
            Some(FakeHandle(id))
        }
        fn stop(&mut self, handle: &mut FakeHandle) {
            self.state.borrow_mut().playing.insert(handle.0, false);
        }
        fn set_gain_pan(&mut self, _handle: &mut FakeHandle, _gain: f32, _pan: f32) {}
        fn is_playing(&self, handle: &FakeHandle) -> bool {
            self.state.borrow().playing.get(&handle.0) == Some(&true)
        }
    }

    fn manager() -> (SpatialSounds<FakeBackend>, FakeBackend) {
        let backend = FakeBackend::default();
        (SpatialSounds::new(backend.clone()), backend)
    }

    fn listener_at_origin() -> AudioListener {
        AudioListener::from_pose(glam::Vec2::ZERO, 0.0)
    }

    #[test]
    fn clip_ids_address_the_same_clip_in_backend_and_pools() {
        let (mut mgr, probe) = manager();
        let a = mgr.register_clip("a", 1);
        let b = mgr.register_clip("b", 3);
        assert_eq!((a, b), (ClipId(0), ClipId(1)));
        mgr.play_non_3d(b, 1.0, 0.0, |_| 0);
        assert_eq!(probe.last_play().0, "b");
        mgr.play_non_3d(a, 1.0, 0.0, |_| 0);
        assert_eq!(probe.last_play().0, "a");
    }

    #[test]
    fn manager_3d_farthest_steal_or_drop_including_the_drop_case() {
        let (mut mgr, probe) = manager();
        let clip = mgr.register_clip("shot", 2);
        let l = listener_at_origin();

        mgr.play_3d(clip, Vec3::new(5.0, 0.0, 0.0), &l, Vec3::ZERO);
        mgr.play_3d(clip, Vec3::new(0.0, 8.0, 0.0), &l, Vec3::ZERO);
        assert_eq!(probe.play_count(), 2);
        assert_eq!(mgr.dropped_3d_count(), 0);

        mgr.play_3d(clip, Vec3::new(20.0, 0.0, 0.0), &l, Vec3::ZERO);
        assert_eq!(probe.play_count(), 2, "farthest new sound must be dropped");
        assert_eq!(mgr.dropped_3d_count(), 1);

        mgr.play_3d(clip, Vec3::new(0.0, 3.0, 0.0), &l, Vec3::ZERO);
        assert_eq!(probe.play_count(), 3);
        assert_eq!(mgr.dropped_3d_count(), 1);

        mgr.play_3d(clip, Vec3::new(50.0, 0.0, 0.0), &l, Vec3::ZERO);
        assert_eq!(probe.play_count(), 3);
        assert_eq!(mgr.dropped_3d_count(), 2);
    }

    #[test]
    fn steal_distances_use_the_live_listener_not_the_ds3d_pose() {
        let (mut mgr, probe) = manager();
        let clip = mgr.register_clip("shot", 1);
        let stale = listener_at_origin();
        mgr.play_3d(clip, Vec3::new(-50.0, 0.0, 0.0), &stale, Vec3::ZERO);

        // Live listener moved to x=60: the old voice is now 110 away and the
        // new sound 10 away, so it steals. Measured from the stale pose
        // (50 vs 70) it would have been dropped.
        mgr.play_3d(clip, Vec3::new(70.0, 0.0, 0.0), &stale, Vec3::new(60.0, 0.0, 0.0));
        assert_eq!(probe.play_count(), 2);
        assert_eq!(mgr.dropped_3d_count(), 0);
    }

    #[test]
    fn manager_non_3d_random_steal_after_n_shots() {
        let (mut mgr, probe) = manager();
        let clip = mgr.register_clip("ui", 3);
        for _ in 0..3 {
            mgr.play_non_3d(clip, 1.0, 0.0, |_| panic!("should not steal yet"));
        }
        assert_eq!(probe.play_count(), 3);
        mgr.play_non_3d(clip, 1.0, 0.0, |n| 1 % n);
        assert_eq!(probe.play_count(), 4);
    }

    #[test]
    fn manager_3d_gain_and_pan_track_listener_geometry() {
        let (mut mgr, probe) = manager();
        let clip = mgr.register_clip("shot", 1);
        // Facing +X; a source due +Y at distance 20: gain 10/20, pan hard right.
        mgr.play_3d(clip, Vec3::new(0.0, 20.0, 0.0), &listener_at_origin(), Vec3::ZERO);
        let (name, gain, pan, looping) = probe.last_play();
        assert_eq!(name, "shot");
        assert!(!looping);
        assert!((gain - 0.5).abs() < 1e-6, "gain = {gain}");
        assert!((pan - 1.0).abs() < 1e-6, "pan = {pan}");
    }

    #[test]
    fn refused_play_is_skipped_and_leaves_the_voice_idle() {
        let (mut mgr, probe) = manager();
        let clip = mgr.register_clip("shot", 1);
        let enemy = mgr.register_clip("enemy", 1);
        mgr.set_alarm_clips(Some(enemy), None);
        probe.state.borrow_mut().refuse_plays = true;
        mgr.play_3d(clip, Vec3::ZERO, &listener_at_origin(), Vec3::ZERO);
        mgr.update_alarms(true, false);
        assert_eq!(probe.play_count(), 0);

        probe.state.borrow_mut().refuse_plays = false;
        mgr.play_3d(clip, Vec3::new(90.0, 0.0, 0.0), &listener_at_origin(), Vec3::ZERO);
        mgr.update_alarms(true, false);
        assert_eq!(probe.play_count(), 2);
    }

    #[test]
    fn manager_alarm_priority_and_stop_semantics() {
        let (mut mgr, probe) = manager();
        let enemy = mgr.register_clip("enemy", 1);
        let targeted = mgr.register_clip("targeted", 1);
        mgr.set_alarm_clips(Some(enemy), Some(targeted));

        mgr.update_alarms(true, true);
        assert_eq!(mgr.current_alarm(), Alarm::EnemyVisible);
        assert_eq!(probe.play_count(), 1);
        let (name, gain, _pan, looping) = probe.last_play();
        assert_eq!(name, "enemy");
        assert!(looping);
        assert!((gain - 0.3162).abs() < 0.001);

        mgr.update_alarms(true, false);
        assert_eq!(probe.play_count(), 1, "already playing => not restarted");

        mgr.update_alarms(false, true);
        assert_eq!(mgr.current_alarm(), Alarm::Targeted);
        assert_eq!(probe.play_count(), 2);
        assert_eq!(probe.last_play().0, "targeted");

        mgr.update_alarms(false, false);
        assert_eq!(mgr.current_alarm(), Alarm::None);
    }

    #[test]
    fn stop_all_is_the_pre_modal_hook() {
        let (mut mgr, probe) = manager();
        let clip = mgr.register_clip("shot", 2);
        let enemy = mgr.register_clip("enemy", 1);
        mgr.set_alarm_clips(Some(enemy), None);
        let l = listener_at_origin();

        mgr.play_3d(clip, Vec3::new(1.0, 0.0, 0.0), &l, Vec3::ZERO);
        mgr.update_alarms(true, false);
        mgr.stop_all();
        assert_eq!(mgr.current_alarm(), Alarm::None);

        let before = probe.play_count();
        mgr.play_3d(clip, Vec3::new(1.0, 0.0, 0.0), &l, Vec3::ZERO);
        assert_eq!(probe.play_count(), before + 1);
    }
}
