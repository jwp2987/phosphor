//! A serde serialization module that supports the RRGGBBAA format (8-digit hex).
//! Also compatible with the RRGGBB (6-digit) format, in which case alpha defaults
//! to 255 (opaque).

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use warpui::color::ColorU;

use super::OPAQUE;

const SHORT_LEN: usize = 3;
const RGB_LEN: usize = 6;
const RGBA_LEN: usize = 8;

/// Parses a ColorU from a hex string, supporting the #RGB, #RRGGBB, and
/// #RRGGBBAA formats.
fn coloru_from_hex_alpha(s: &str) -> Result<ColorU, String> {
    if !s.starts_with('#') {
        return Err("Expected hex color string starting with #".to_string());
    }

    let hex = &s[1..];

    if hex.len() != SHORT_LEN && hex.len() != RGB_LEN && hex.len() != RGBA_LEN {
        return Err(format!(
            "Expected hex color string with 3, 6, or 8 characters after #, got {}",
            hex.len()
        ));
    }

    // Expand the 3-digit shorthand: #RGB -> #RRGGBB
    let expanded: String = if hex.len() == SHORT_LEN {
        hex.chars().flat_map(|c| std::iter::repeat_n(c, 2)).collect()
    } else {
        hex.to_string()
    };

    let parsed: Result<Vec<u8>, _> = (0..expanded.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&expanded[i..i + 2], 16))
        .collect();

    match parsed {
        Ok(bytes) => match bytes.len() {
            // This branch handles both #RRGGBB (6 characters) and the already-expanded
            // #RGB (3->6 characters) cases
            3 => Ok(ColorU {
                r: bytes[0],
                g: bytes[1],
                b: bytes[2],
                a: OPAQUE,
            }),
            4 => Ok(ColorU {
                r: bytes[0],
                g: bytes[1],
                b: bytes[2],
                a: bytes[3],
            }),
            _ => Err("Invalid hex color length".to_string()),
        },
        Err(_) => Err("Invalid hex color string".to_string()),
    }
}

/// Serializes a ColorU into a hex string.
/// Outputs 6 digits (#RRGGBB) when alpha is 255 to keep it concise, otherwise
/// outputs 8 digits (#RRGGBBAA).
fn coloru_to_hex_alpha_string(color: &ColorU) -> String {
    if color.a == OPAQUE {
        format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            color.r, color.g, color.b, color.a
        )
    }
}

/// serde deserialize function, for use with `#[serde(with = "hex_color_alpha")]`.
pub fn deserialize<'de, D, C>(deserializer: D) -> Result<C, D::Error>
where
    C: From<ColorU>,
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    coloru_from_hex_alpha(&s)
        .map(Into::into)
        .map_err(de::Error::custom)
}

/// serde serialize function, for use with `#[serde(with = "hex_color_alpha")]`.
pub fn serialize<S, C>(color: &C, serializer: S) -> Result<S::Ok, S::Error>
where
    C: Into<ColorU> + Clone,
    S: Serializer,
{
    let coloru: ColorU = color.to_owned().into();
    coloru_to_hex_alpha_string(&coloru).serialize(serializer)
}

/// A serde serialization/deserialization module that supports Option<ColorU>.
/// For use with `#[serde(default, with = "hex_color_alpha::option")]`.
pub mod option {
    use super::*;

    /// Deserializes an optional hex color value, supporting both None and a string.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<ColorU>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            Some(s) => coloru_from_hex_alpha(&s).map(Some).map_err(de::Error::custom),
            None => Ok(None),
        }
    }

    /// Serializes an optional hex color value, outputting null when None.
    pub fn serialize<S>(color: &Option<ColorU>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match color {
            Some(c) => coloru_to_hex_alpha_string(c).serialize(serializer),
            None => serializer.serialize_none(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_6char() {
        let c = coloru_from_hex_alpha("#3994BC").unwrap();
        assert_eq!(c.r, 0x39);
        assert_eq!(c.g, 0x94);
        assert_eq!(c.b, 0xBC);
        assert_eq!(c.a, 255);
    }

    #[test]
    fn test_parse_8char() {
        let c = coloru_from_hex_alpha("#3994BCB3").unwrap();
        assert_eq!(c.r, 0x39);
        assert_eq!(c.g, 0x94);
        assert_eq!(c.b, 0xBC);
        assert_eq!(c.a, 0xB3);
    }

    #[test]
    fn test_parse_3char() {
        let c = coloru_from_hex_alpha("#ABC").unwrap();
        assert_eq!(c.r, 0xAA);
        assert_eq!(c.g, 0xBB);
        assert_eq!(c.b, 0xCC);
        assert_eq!(c.a, 255);
    }

    #[test]
    fn test_serialize_opaque() {
        let c = ColorU { r: 0x39, g: 0x94, b: 0xBC, a: 255 };
        assert_eq!(coloru_to_hex_alpha_string(&c), "#3994bc");
    }

    #[test]
    fn test_serialize_with_alpha() {
        let c = ColorU { r: 0x39, g: 0x94, b: 0xBC, a: 0xB3 };
        assert_eq!(coloru_to_hex_alpha_string(&c), "#3994bcb3");
    }
}
