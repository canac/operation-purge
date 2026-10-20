//! Entry point (design §2 `main.rs`): CLI args, config load, startup.
//!
//! `anyhow` is used only here — every subsystem below `main` returns a
//! typed error (`operation_purge::error`), per design §1.2.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use operation_purge::app::{config::Config, embedded};
use std::path::Path;

struct Args {
    assets_dir: Option<String>,
    seed: Option<u64>,
    /// Hidden flag: fixed per-frame dt for deterministic runs (design §5.1).
    fixed_dt: Option<f32>,
    /// Hidden flags: scripted E2E run (see `app::e2e`).
    e2e: Option<String>,
    e2e_out: Option<String>,
    window: Option<(u32, u32)>,
}

fn parse_args() -> anyhow::Result<Args> {
    let mut args = Args {
        assets_dir: None,
        seed: None,
        fixed_dt: None,
        e2e: None,
        e2e_out: None,
        window: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--assets" => {
                args.assets_dir = Some(
                    it.next()
                        .ok_or_else(|| anyhow::anyhow!("--assets requires a path"))?,
                );
            }
            "--seed" => {
                let raw = it
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--seed requires a value"))?;
                args.seed = Some(raw.parse().map_err(|e| {
                    anyhow::anyhow!("--seed value `{raw}` is not a valid u64: {e}")
                })?);
            }
            "--fixed-dt" => {
                let raw = it
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--fixed-dt requires a value"))?;
                args.fixed_dt = Some(raw.parse().map_err(|e| {
                    anyhow::anyhow!("--fixed-dt value `{raw}` is not a valid f32: {e}")
                })?);
            }
            "--e2e" => {
                args.e2e = Some(it.next().ok_or_else(|| anyhow::anyhow!("--e2e requires a script path"))?);
            }
            "--e2e-out" => {
                args.e2e_out = Some(it.next().ok_or_else(|| anyhow::anyhow!("--e2e-out requires a dir"))?);
            }
            "--window" => {
                let raw = it
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--window requires WxH"))?;
                let (w, h) = raw
                    .split_once('x')
                    .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
                    .ok_or_else(|| anyhow::anyhow!("--window value `{raw}` is not WxH"))?;
                args.window = Some((w, h));
            }
            other => {
                anyhow::bail!("unrecognized argument: {other}");
            }
        }
    }
    Ok(args)
}

/// A relative configured path is tried beside the exe, then against the
/// working dir; if neither exists, the embedded assets are unpacked to the
/// user data dir and that is used.
fn resolve_assets_dir(configured: &str) -> String {
    let rel = Path::new(configured);
    if rel.is_absolute() {
        return configured.to_string();
    }
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(rel)))
        .filter(|p| p.is_dir());
    if let Some(p) = beside_exe {
        return p.display().to_string();
    }
    if rel.is_dir() {
        return configured.to_string();
    }
    if let Some(dest) = embedded::default_unpack_dir() {
        match embedded::ensure_unpacked(&dest) {
            Ok(()) => return dest.display().to_string(),
            Err(e) => eprintln!("warning: could not unpack assets to {}: {e}", dest.display()),
        }
    }
    configured.to_string()
}

fn main() -> anyhow::Result<()> {
    let args = parse_args()?;

    let mut config = Config::load();
    if let Some(assets_dir) = args.assets_dir {
        config.assets_dir = assets_dir;
    } else if let Ok(env_assets) = std::env::var("OP_ASSETS") {
        config.assets_dir = env_assets;
    } else {
        config.assets_dir = resolve_assets_dir(&config.assets_dir);
    }
    if let Some(seed) = args.seed {
        config.seed = Some(seed);
    }

    println!(
        "Operation Purge starting — assets_dir={}, vsync={}, seed={:?}, fixed_dt={:?}",
        config.assets_dir, config.vsync, config.seed, args.fixed_dt
    );

    let mut opts = operation_purge::app::RunOptions {
        fixed_dt: args.fixed_dt,
        windowed: args.window,
        ..Default::default()
    };
    if let Some(script) = args.e2e {
        let out = std::path::PathBuf::from(args.e2e_out.unwrap_or_else(|| "e2e-out".into()));
        let driver = operation_purge::app::e2e::Driver::load(std::path::Path::new(&script), out)
            .map_err(|e| anyhow::anyhow!(e))?;
        opts.e2e = Some(driver);
        opts.fixed_dt = opts.fixed_dt.or(Some(1.0 / 60.0));
        opts.windowed = opts.windowed.or(Some((1280, 800)));
    }
    operation_purge::app::run(config, opts)?;
    Ok(())
}
