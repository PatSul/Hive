use gpui::{Hsla, Pixels, SharedString, hsla, px};
use hive_core::theme_manager::ThemeDefinition;

/// Complete design system with all color tokens, typography, spacing, and radii.
#[derive(Clone)]
pub struct HiveTheme {
    // Base
    pub bg_primary: Hsla,
    pub bg_secondary: Hsla,
    pub bg_tertiary: Hsla,
    pub bg_surface: Hsla,

    // Accent
    pub accent_aqua: Hsla,
    pub accent_powder: Hsla,
    pub accent_cyan: Hsla,
    pub accent_green: Hsla,
    pub accent_red: Hsla,
    pub accent_yellow: Hsla,
    pub accent_pink: Hsla,

    // Text
    pub text_primary: Hsla,
    pub text_secondary: Hsla,
    pub text_muted: Hsla,
    pub text_on_accent: Hsla,

    // Borders
    pub border: Hsla,
    pub border_focus: Hsla,

    // Typography
    pub font_ui: SharedString,
    pub font_mono: SharedString,
    pub font_size_xs: Pixels,
    pub font_size_sm: Pixels,
    pub font_size_base: Pixels,
    pub font_size_lg: Pixels,
    pub font_size_xl: Pixels,
    pub font_size_2xl: Pixels,

    // Spacing (4px grid)
    pub space_1: Pixels,
    pub space_2: Pixels,
    pub space_3: Pixels,
    pub space_4: Pixels,
    pub space_6: Pixels,
    pub space_8: Pixels,

    // Radii
    pub radius_sm: Pixels,
    pub radius_md: Pixels,
    pub radius_lg: Pixels,
    pub radius_xl: Pixels,
    pub radius_full: Pixels,
}

impl HiveTheme {
    pub fn dark() -> Self {
        Self {
            // Base palette (deep navy + electric cyan contrast)
            bg_primary: hex_to_hsla(0x0B, 0x10, 0x1F),
            bg_secondary: hex_to_hsla(0x12, 0x19, 0x2B),
            bg_tertiary: hex_to_hsla(0x1A, 0x26, 0x44),
            bg_surface: hex_to_hsla(0x14, 0x1E, 0x38),

            // Accents
            accent_aqua: hex_to_hsla(0x00, 0xF3, 0xFF),
            accent_powder: hex_to_hsla(0xB5, 0xE8, 0xFF),
            accent_cyan: hex_to_hsla(0x00, 0xD4, 0xFF),
            accent_green: hex_to_hsla(0xA7, 0xE4, 0x98),
            accent_red: hex_to_hsla(0xFF, 0x8F, 0xA6),
            accent_yellow: hex_to_hsla(0xF9, 0xDE, 0x8C),
            accent_pink: hex_to_hsla(0xF5, 0xB8, 0xDD),

            // Text
            text_primary: hex_to_hsla(0xEF, 0xF4, 0xFF),
            text_secondary: hex_to_hsla(0xC0, 0xCD, 0xEF),
            text_muted: hex_to_hsla(0x8D, 0x98, 0xB8),
            text_on_accent: hex_to_hsla(0x08, 0x08, 0x12),

            // Borders
            border: hex_to_hsla(0x2A, 0x39, 0x62),
            border_focus: hsla(186.0 / 360.0, 1.0, 0.50, 0.45),

            // Typography
            font_ui: SharedString::from("Inter"),
            font_mono: SharedString::from("JetBrains Mono"),
            font_size_xs: px(11.0),
            font_size_sm: px(12.5),
            font_size_base: px(14.5),
            font_size_lg: px(16.5),
            font_size_xl: px(20.0),
            font_size_2xl: px(30.0),

            // Spacing (4px grid)
            space_1: px(4.0),
            space_2: px(8.0),
            space_3: px(12.0),
            space_4: px(16.0),
            space_6: px(24.0),
            space_8: px(32.0),

            // Radii
            radius_sm: px(6.0),
            radius_md: px(10.0),
            radius_lg: px(14.0),
            radius_xl: px(18.0),
            radius_full: px(9999.0),
        }
    }

    /// Construct a `HiveTheme` from a portable [`ThemeDefinition`].
    ///
    /// Color tokens are parsed from hex strings; font sizes, spacing, and radii
    /// inherit the defaults from [`Self::dark()`].
    pub fn from_definition(def: &ThemeDefinition) -> Self {
        let c = &def.colors;
        Self {
            bg_primary: parse_hex_color(&c.bg_primary),
            bg_secondary: parse_hex_color(&c.bg_secondary),
            bg_tertiary: parse_hex_color(&c.bg_tertiary),
            bg_surface: parse_hex_color(&c.bg_surface),
            accent_aqua: parse_hex_color(&c.accent_primary),
            accent_powder: parse_hex_color(&c.accent_secondary),
            accent_cyan: parse_hex_color(if c.accent_info.is_empty() {
                &c.accent_primary
            } else {
                &c.accent_info
            }),
            accent_green: parse_hex_color(&c.accent_success),
            accent_red: parse_hex_color(&c.accent_error),
            accent_yellow: parse_hex_color(&c.accent_warning),
            accent_pink: parse_hex_color(if c.accent_pink.is_empty() {
                &c.accent_error
            } else {
                &c.accent_pink
            }),
            text_primary: parse_hex_color(&c.text_primary),
            text_secondary: parse_hex_color(&c.text_secondary),
            text_muted: parse_hex_color(&c.text_muted),
            text_on_accent: parse_hex_color(&c.text_on_accent),
            border: parse_hex_color(&c.border),
            border_focus: if c.border_focus.is_empty() {
                hsla(186.0 / 360.0, 1.0, 0.50, 0.45) // default cyan focus
            } else {
                let col = parse_hex_color(&c.border_focus);
                hsla(col.h, col.s, col.l, 0.45)
            },
            font_ui: SharedString::from(def.fonts.ui.clone()),
            font_mono: SharedString::from(def.fonts.mono.clone()),
            // Inherit fixed spacing/sizing defaults
            ..Self::dark()
        }
    }

    /// Export the current theme to a portable [`ThemeDefinition`].
    pub fn to_definition(&self, name: &str, author: &str) -> ThemeDefinition {
        use hive_core::theme_manager::{ThemeColors, ThemeFonts};
        ThemeDefinition {
            name: name.to_string(),
            author: author.to_string(),
            version: "1.0.0".to_string(),
            description: String::new(),
            colors: ThemeColors {
                bg_primary: hsla_to_hex(self.bg_primary),
                bg_secondary: hsla_to_hex(self.bg_secondary),
                bg_tertiary: hsla_to_hex(self.bg_tertiary),
                bg_surface: hsla_to_hex(self.bg_surface),
                accent_primary: hsla_to_hex(self.accent_aqua),
                accent_secondary: hsla_to_hex(self.accent_powder),
                accent_success: hsla_to_hex(self.accent_green),
                accent_warning: hsla_to_hex(self.accent_yellow),
                accent_error: hsla_to_hex(self.accent_red),
                accent_info: hsla_to_hex(self.accent_cyan),
                accent_pink: hsla_to_hex(self.accent_pink),
                text_primary: hsla_to_hex(self.text_primary),
                text_secondary: hsla_to_hex(self.text_secondary),
                text_muted: hsla_to_hex(self.text_muted),
                text_on_accent: hsla_to_hex(self.text_on_accent),
                border: hsla_to_hex(self.border),
                border_focus: hsla_to_hex(Hsla {
                    h: self.border_focus.h,
                    s: self.border_focus.s,
                    l: self.border_focus.l,
                    a: 1.0,
                }),
            },
            fonts: ThemeFonts {
                ui: self.font_ui.to_string(),
                mono: self.font_mono.to_string(),
            },
        }
    }

    /// Built-in light theme.
    pub fn light() -> Self {
        Self {
            bg_primary: hex_to_hsla(0xFA, 0xFA, 0xFC),
            bg_secondary: hex_to_hsla(0xF0, 0xF0, 0xF5),
            bg_tertiary: hex_to_hsla(0xE5, 0xE5, 0xEA),
            bg_surface: hex_to_hsla(0xFF, 0xFF, 0xFF),
            accent_aqua: hex_to_hsla(0x00, 0x96, 0xC7),
            accent_powder: hex_to_hsla(0x5B, 0xB3, 0xD5),
            accent_cyan: hex_to_hsla(0x00, 0x77, 0xB6),
            accent_green: hex_to_hsla(0x2D, 0x9C, 0x4A),
            accent_red: hex_to_hsla(0xDC, 0x26, 0x26),
            accent_yellow: hex_to_hsla(0xD9, 0x77, 0x06),
            accent_pink: hex_to_hsla(0xDB, 0x27, 0x77),
            text_primary: hex_to_hsla(0x1A, 0x1A, 0x2E),
            text_secondary: hex_to_hsla(0x4A, 0x4A, 0x68),
            text_muted: hex_to_hsla(0x9A, 0x9A, 0xB0),
            text_on_accent: hex_to_hsla(0xFF, 0xFF, 0xFF),
            border: hex_to_hsla(0xD0, 0xD0, 0xDE),
            border_focus: hsla(200.0 / 360.0, 0.8, 0.45, 0.45),
            ..Self::dark()
        }
    }
}

/// Convert RGB bytes to GPUI Hsla color.
pub fn hex_to_hsla(r: u8, g: u8, b: u8) -> Hsla {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;

    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;

    let l = (max + min) / 2.0;

    if delta == 0.0 {
        return hsla(0.0, 0.0, l, 1.0);
    }

    let s = if l < 0.5 {
        delta / (max + min)
    } else {
        delta / (2.0 - max - min)
    };

    let h = if max == rf {
        ((gf - bf) / delta + if gf < bf { 6.0 } else { 0.0 }) / 6.0
    } else if max == gf {
        ((bf - rf) / delta + 2.0) / 6.0
    } else {
        ((rf - gf) / delta + 4.0) / 6.0
    };

    hsla(h, s, l, 1.0)
}

/// Parse a CSS hex color string (e.g. `"#2E3440"` or `"2E3440"`) into GPUI Hsla.
pub fn parse_hex_color(hex: &str) -> Hsla {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return hsla(0.0, 0.0, 0.5, 1.0); // fallback gray
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(128);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(128);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(128);
    hex_to_hsla(r, g, b)
}

/// Convert a GPUI Hsla color back to a CSS hex string (e.g. `"#2E3440"`).
pub fn hsla_to_hex(color: Hsla) -> String {
    let h = color.h;
    let s = color.s;
    let l = color.l;

    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = match (h * 6.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    let r = ((r1 + m) * 255.0).round() as u8;
    let g = ((g1 + m) * 255.0).round() as u8;
    let b = ((b1 + m) * 255.0).round() as u8;

    format!("#{:02X}{:02X}{:02X}", r, g, b)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_color_with_hash() {
        let c = parse_hex_color("#FF0000");
        // Pure red: h=0, s=1, l=0.5
        assert!((c.h - 0.0).abs() < 0.01);
        assert!((c.s - 1.0).abs() < 0.01);
        assert!((c.l - 0.5).abs() < 0.01);
    }

    #[test]
    fn parse_hex_color_without_hash() {
        let c = parse_hex_color("00FF00");
        // Pure green: h=1/3, s=1, l=0.5
        assert!((c.h - 1.0 / 3.0).abs() < 0.01);
        assert!((c.s - 1.0).abs() < 0.01);
        assert!((c.l - 0.5).abs() < 0.01);
    }

    #[test]
    fn parse_hex_color_invalid_returns_gray() {
        let c = parse_hex_color("#ZZZ");
        assert!((c.l - 0.5).abs() < 0.01);
        assert!((c.s - 0.0).abs() < 0.01);
    }

    #[test]
    fn hex_to_hsla_black() {
        let c = hex_to_hsla(0, 0, 0);
        assert!((c.l - 0.0).abs() < 0.01);
    }

    #[test]
    fn hex_to_hsla_white() {
        let c = hex_to_hsla(255, 255, 255);
        assert!((c.l - 1.0).abs() < 0.01);
    }

    #[test]
    fn hsla_to_hex_pure_red() {
        let color = hsla(0.0, 1.0, 0.5, 1.0);
        assert_eq!(hsla_to_hex(color), "#FF0000");
    }

    #[test]
    fn hsla_to_hex_pure_white() {
        let color = hsla(0.0, 0.0, 1.0, 1.0);
        assert_eq!(hsla_to_hex(color), "#FFFFFF");
    }

    #[test]
    fn hsla_to_hex_pure_black() {
        let color = hsla(0.0, 0.0, 0.0, 1.0);
        assert_eq!(hsla_to_hex(color), "#000000");
    }

    #[test]
    fn hex_roundtrip_common_colors() {
        // Test that hex -> hsla -> hex roundtrips accurately for common colors.
        let test_cases = [
            "#2E3440", // Nord bg
            "#FF5555", // Dracula red
            "#282C34", // One Dark bg
            "#FAFAFC", // Light bg
        ];
        for hex in test_cases {
            let hsla_val = parse_hex_color(hex);
            let back = hsla_to_hex(hsla_val);
            assert_eq!(
                hex, back,
                "Roundtrip failed for {hex}: got {back}"
            );
        }
    }

    #[test]
    fn dark_theme_constructs() {
        let t = HiveTheme::dark();
        // Spot-check a few values
        assert!(t.bg_primary.l < 0.15, "dark bg should be very dark");
        assert!(t.accent_aqua.s > 0.9, "aqua accent should be saturated");
    }

    #[test]
    fn light_theme_constructs() {
        let t = HiveTheme::light();
        assert!(t.bg_primary.l > 0.9, "light bg should be very light");
        assert!(t.text_on_accent.l > 0.9, "text_on_accent should be light in light theme");
    }

    #[test]
    fn from_definition_roundtrip() {
        let dark = HiveTheme::dark();
        let def = dark.to_definition("Test Dark", "Tester");
        let reconstructed = HiveTheme::from_definition(&def);

        // Colors should match closely (f32 rounding)
        assert!(
            (dark.bg_primary.h - reconstructed.bg_primary.h).abs() < 0.02,
            "bg_primary hue mismatch"
        );
        assert!(
            (dark.accent_aqua.s - reconstructed.accent_aqua.s).abs() < 0.02,
            "accent_aqua sat mismatch"
        );
        assert_eq!(reconstructed.font_ui.as_ref(), "Inter");
        assert_eq!(reconstructed.font_mono.as_ref(), "JetBrains Mono");
    }
}
