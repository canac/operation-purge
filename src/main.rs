//! Entry point (design §2 `main.rs`): CLI args, config load, startup.
//!
//! `anyhow` is used only here — every subsystem below `main` returns a
//! typed error (`operation_purge::error`), per design §1.2.

use operation_purge::app::config::Config;

struct Args {
    assets_dir: Option<String>,
    seed: Option<u64>,
    /// Hidden flag: drives deterministic headless tests only (design §5.1).
    fixed_dt: Option<f32>,
}

fn parse_args() -> anyhow::Result<Args> {
    let mut args = Args {
        assets_dir: None,
        seed: None,
        fixed_dt: None,
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
            other => {
                anyhow::bail!("unrecognized argument: {other}");
            }
        }
    }
    Ok(args)
}

fn main() -> anyhow::Result<()> {
    let args = parse_args()?;

    let mut config = Config::load();
    if let Some(assets_dir) = args.assets_dir {
        config.assets_dir = assets_dir;
    } else if let Ok(env_assets) = std::env::var("OP_ASSETS") {
        config.assets_dir = env_assets;
    }
    if let Some(seed) = args.seed {
        config.seed = Some(seed);
    }

    println!(
        "Operation Purge starting — assets_dir={}, vsync={}, seed={:?}, fixed_dt={:?}",
        config.assets_dir, config.vsync, config.seed, args.fixed_dt
    );

    // M3: open the window, create the GL context, and run the input/text
    // shell (borderless fullscreen; Esc quits). See RUN.md for what to
    // eyeball. `--fixed-dt` is a headless-test-only flag and does not drive
    // this interactive path.
    operation_purge::app::run(config)?;
    Ok(())
}
