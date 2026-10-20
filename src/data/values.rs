//! Deserialize helpers for `GameData.toml` scalar types that don't map
//! directly onto TOML's native scalars.
//!
//! Every conversion here runs **once, at deserialize time**: colors land
//! as `Rgba` f32 directly (no `COLORREF`/`D3DCOLOR` byte order ever exists
//! in this port's memory) and angle fields land in radians. This folds
//! the original's separate `TransformGameData` mutate-in-place pass
//! (`cOperationPurge.cpp:2600-2691`) into parsing, so there is no second
//! conversion step and therefore no double-conversion trap.

use crate::math::Rgba;
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer};

/// `Color = "0xRRGGBB"` → opaque `Rgba` (alpha forced to `1.0`).
///
/// PARITY: alpha is forced opaque here for every `[colors]` field except
/// the alpha-keeping ones (`Background2DColor`, all
/// three center-type color lists, particle stage colors) — those use
/// [`deserialize_alpha_color`] instead.
pub fn deserialize_color<'de, D>(deserializer: D) -> Result<Rgba, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let (r, g, b, a) = parse_hex_color(&s, false).map_err(DeError::custom)?;
    Ok(Rgba::new(r, g, b, a))
}

/// `AlphaColor = "0xAARRGGBB"` (**alpha first**, matching the original
/// lexeme) → `Rgba` with alpha kept.
pub fn deserialize_alpha_color<'de, D>(deserializer: D) -> Result<Rgba, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let (r, g, b, a) = parse_hex_color(&s, true).map_err(DeError::custom)?;
    Ok(Rgba::new(r, g, b, a))
}

/// Parses `"0xRRGGBB"` or (`with_alpha`) `"0xAARRGGBB"` into `(r, g, b, a)`
/// components normalized to `0.0..=1.0`.
fn parse_hex_color(s: &str, with_alpha: bool) -> Result<(f32, f32, f32, f32), String> {
    let hex = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    let expected_len = if with_alpha { 8 } else { 6 };
    if hex.len() != expected_len {
        return Err(format!(
            "expected {expected_len} hex digits after `0x` (got `{s}`)"
        ));
    }
    let byte = |slice: &str| -> Result<f32, String> {
        u8::from_str_radix(slice, 16)
            .map(|v| v as f32 / 255.0)
            .map_err(|e| format!("`{s}`: {e}"))
    };
    if with_alpha {
        let a = byte(&hex[0..2])?;
        let r = byte(&hex[2..4])?;
        let g = byte(&hex[4..6])?;
        let b = byte(&hex[6..8])?;
        Ok((r, g, b, a))
    } else {
        let r = byte(&hex[0..2])?;
        let g = byte(&hex[2..4])?;
        let b = byte(&hex[4..6])?;
        Ok((r, g, b, 1.0))
    }
}


/// `Ref` = the string `"None"` → `None`, else a 0-based integer index
/// newtype. Bounds-validation against the referenced vector's length
/// happens after the *entire* file loads (GUARD) — the
/// original never validated at all and would index out of bounds at
/// spawn time.
pub fn deserialize_ref<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: From<u32>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum RefValue {
        Named(String),
        Index(u32),
    }
    match RefValue::deserialize(deserializer)? {
        RefValue::Named(s) if s == "None" => Ok(None),
        RefValue::Named(s) => Err(DeError::custom(format!(
            "expected `None` or a non-negative integer index, found `{s}`"
        ))),
        RefValue::Index(i) => Ok(Some(T::from(i))),
    }
}

/// Degrees → radians, applied at deserialize time for every angle field
/// authored in degrees (`robot_type.rotate_rate`,
/// `projectile.accuracy`, particle rotation fields, emitter theta/phi
/// ranges).
pub fn deserialize_degrees_as_radians<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: Deserializer<'de>,
{
    let degrees = f32::deserialize(deserializer)?;
    Ok(degrees.to_radians())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_parses_opaque() {
        let c = parse_hex_color("0xFF8800", false).unwrap();
        assert_eq!(c, (1.0, 0x88 as f32 / 255.0, 0.0, 1.0));
    }

    #[test]
    fn alpha_color_keeps_alpha_and_reads_alpha_first() {
        let c = parse_hex_color("0x80FF8800", true).unwrap();
        assert_eq!(c.3, 0x80 as f32 / 255.0, "alpha byte must be the FIRST byte");
        assert_eq!(c.0, 1.0);
        assert_eq!(c.1, 0x88 as f32 / 255.0);
        assert_eq!(c.2, 0.0);
    }

    #[test]
    fn bad_length_is_an_error() {
        assert!(parse_hex_color("0xFFF", false).is_err());
    }
}
