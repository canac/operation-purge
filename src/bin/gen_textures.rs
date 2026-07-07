//! One-time tool: generates the alpha sprite/particle textures the game
//! references but which were lost from the original tree
//! (`Textures/Projectile.png`, `Textures/Hole.png`, `Textures/Smoke.png`).
//!
//! Only three BMP textures survived (`Camo big.bmp`, `Screw.bmp`,
//! `Steel.bmp`); the wall/ceiling references are remapped onto those in
//! `GameData.toml`, but the three PNGs are alpha-blended billboards with no
//! BMP equivalent, so they are RECONSTRUCTED here as simple radial gradients
//! (labeled reconstructed in the README, like the alarm tones and LightData).
//! They exist so the sprite pipeline has real RGBA textures to bind; the art
//! is a placeholder, not a faithful copy.
//!
//! Usage: `gen_textures [assets_dir]` (default `./assets`); writes into
//! `<assets_dir>/Textures/`.

use std::path::PathBuf;

/// A radial RGBA sprite: `inner` color at center fading to transparent at the
/// edge. `alpha_scale` caps peak opacity; `power` shapes the falloff curve.
fn radial(size: u32, inner: [u8; 3], alpha_scale: f32, power: f32) -> image::RgbaImage {
    let c = (size as f32 - 1.0) / 2.0;
    let max_r = c;
    image::RgbaImage::from_fn(size, size, |x, y| {
        let dx = x as f32 - c;
        let dy = y as f32 - c;
        let d = (dx * dx + dy * dy).sqrt() / max_r; // 0 center .. 1 edge
        let a = ((1.0 - d).clamp(0.0, 1.0)).powf(power) * alpha_scale;
        image::Rgba([inner[0], inner[1], inner[2], (a * 255.0).round() as u8])
    })
}

fn main() -> anyhow::Result<()> {
    let assets_dir: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("assets"));
    let dir = assets_dir.join("Textures");
    std::fs::create_dir_all(&dir)?;

    // Projectile: small hot yellow-white dot, sharp core.
    let projectile = radial(16, [255, 236, 160], 1.0, 1.8);
    // Hole: dark scorch decal, wider soft edge.
    let hole = radial(32, [24, 20, 18], 0.9, 1.2);
    // Smoke: soft grey puff, low peak opacity.
    let smoke = radial(32, [150, 150, 150], 0.55, 1.5);

    for (name, img) in [
        ("Projectile.png", &projectile),
        ("Hole.png", &hole),
        ("Smoke.png", &smoke),
    ] {
        let dst = dir.join(name);
        img.save(&dst)
            .map_err(|e| anyhow::anyhow!("cannot write {}: {e}", dst.display()))?;
        println!("wrote {} ({}x{})", dst.display(), img.width(), img.height());
    }
    Ok(())
}
