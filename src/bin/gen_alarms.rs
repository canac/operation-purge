//! One-time tool: generates the two looping alarm sounds the game references
//! (`Sounds/Alarm.wav`, `Sounds/FastAlarm.wav`).
//!
//! Like `Explosion.wav`, these files were lost from the original tree — the
//! only surviving source WAVs are `MachineGun.wav`, `RocketLaunch.wav`, and
//! `EditedRocketLaunch.wav`. There is no boom/burst suitable for a *looping*
//! alarm, so these are RECONSTRUCTED as simple beep-and-gap tones (labeled
//! reconstructed in the README, like the LightData attenuation values). The
//! game loops them at -1000 mB (design §5.4); the beep/gap cadence is what
//! makes the loop read as an alarm.
//!
//! Output matches the surviving set's format exactly — 8-bit unsigned mono
//! 22050 Hz PCM (source-map §3.9) — so the loader path is identical. Each
//! buffer begins and ends on silence for a click-free loop.
//!
//! Usage: `gen_alarms [assets_dir]` (default `./assets`); writes
//! `<assets_dir>/Sounds/Alarm.wav` and `<assets_dir>/Sounds/FastAlarm.wav`.

use std::f32::consts::TAU;
use std::path::PathBuf;

const SAMPLE_RATE: u32 = 22050;
/// Beep amplitude in the 8-bit signed range [-128, 127]; kept moderate since
/// the alarm gain is applied downstream (-1000 mB loop).
const AMPLITUDE: f32 = 45.0;
/// Linear fade at each beep edge to avoid click transients (~5 ms).
const FADE_SAMPLES: usize = 110;

/// One alarm's parameters: tone frequency, beep-on duration, total loop period.
struct Alarm {
    file: &'static str,
    freq_hz: f32,
    beep_secs: f32,
    period_secs: f32,
}

/// `Alarm.wav`: slower, lower "targeted" tone. `FastAlarm.wav`: faster, higher
/// "enemy visible" tone (design §5.4: EnemyVisible outranks Targeted).
const ALARMS: [Alarm; 2] = [
    Alarm { file: "Alarm.wav", freq_hz: 660.0, beep_secs: 0.15, period_secs: 0.50 },
    Alarm { file: "FastAlarm.wav", freq_hz: 990.0, beep_secs: 0.09, period_secs: 0.25 },
];

fn render(alarm: &Alarm) -> Vec<i8> {
    let total = (alarm.period_secs * SAMPLE_RATE as f32).round() as usize;
    let beep = (alarm.beep_secs * SAMPLE_RATE as f32).round() as usize;
    (0..total)
        .map(|i| {
            if i >= beep {
                return 0; // silent gap
            }
            let t = i as f32 / SAMPLE_RATE as f32;
            let mut a = (TAU * alarm.freq_hz * t).sin() * AMPLITUDE;
            // Fade in/out across the beep edges.
            let fade_in = (i as f32 / FADE_SAMPLES as f32).min(1.0);
            let fade_out = ((beep - i) as f32 / FADE_SAMPLES as f32).min(1.0);
            a *= fade_in.min(fade_out);
            a.round().clamp(-128.0, 127.0) as i8
        })
        .collect()
}

fn main() -> anyhow::Result<()> {
    let assets_dir: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("assets"));
    let sounds = assets_dir.join("Sounds");
    std::fs::create_dir_all(&sounds)?;

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 8,
        sample_format: hound::SampleFormat::Int,
    };

    for alarm in &ALARMS {
        let dst = sounds.join(alarm.file);
        let samples = render(alarm);
        let mut writer = hound::WavWriter::create(&dst, spec)
            .map_err(|e| anyhow::anyhow!("cannot create {}: {e}", dst.display()))?;
        for &s in &samples {
            writer.write_sample(s)?;
        }
        writer.finalize()?;
        println!(
            "wrote {} ({} samples, {} Hz beep, {:.0} ms period)",
            dst.display(),
            samples.len(),
            alarm.freq_hz,
            alarm.period_secs * 1000.0
        );
    }
    Ok(())
}
