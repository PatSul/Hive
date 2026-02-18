//! Theme data model, built-in themes, and file management.
//!
//! [`ThemeDefinition`] is the portable, JSON-serialisable representation of a
//! theme.  It is shared between the Rust desktop app and the (future) web theme
//! gallery.
//!
//! [`ThemeManager`] loads, saves, lists, and deletes custom theme JSON files
//! stored in `~/.hive/themes/`, and also exposes the eight built-in themes that
//! ship with HiveCode.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Portable theme definition (the JSON schema)
// ---------------------------------------------------------------------------

/// Serializable theme definition -- the JSON file format.
///
/// This schema is shared between the Rust app and the web theme gallery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeDefinition {
    pub name: String,
    pub author: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub colors: ThemeColors,
    #[serde(default)]
    pub fonts: ThemeFonts,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

/// All color tokens in a theme, expressed as CSS hex strings (`"#RRGGBB"`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeColors {
    pub bg_primary: String,
    pub bg_secondary: String,
    pub bg_tertiary: String,
    pub bg_surface: String,
    pub accent_primary: String,
    pub accent_secondary: String,
    pub accent_success: String,
    pub accent_warning: String,
    pub accent_error: String,
    #[serde(default)]
    pub accent_info: String,
    #[serde(default)]
    pub accent_pink: String,
    pub text_primary: String,
    pub text_secondary: String,
    pub text_muted: String,
    #[serde(default = "default_text_on_accent")]
    pub text_on_accent: String,
    #[serde(default = "default_border")]
    pub border: String,
    #[serde(default)]
    pub border_focus: String,
}

fn default_text_on_accent() -> String {
    "#080812".to_string()
}
fn default_border() -> String {
    "#2A3962".to_string()
}

/// Font family names used in the theme.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeFonts {
    #[serde(default = "default_ui_font")]
    pub ui: String,
    #[serde(default = "default_mono_font")]
    pub mono: String,
}

impl Default for ThemeFonts {
    fn default() -> Self {
        Self {
            ui: default_ui_font(),
            mono: default_mono_font(),
        }
    }
}

fn default_ui_font() -> String {
    "Inter".to_string()
}
fn default_mono_font() -> String {
    "JetBrains Mono".to_string()
}

// ---------------------------------------------------------------------------
// ThemeManager
// ---------------------------------------------------------------------------

/// Manages theme JSON files stored in `~/.hive/themes/`.
pub struct ThemeManager {
    themes_dir: PathBuf,
}

impl ThemeManager {
    /// Create a new manager, ensuring the themes directory exists.
    pub fn new() -> Result<Self> {
        let dir = dirs::home_dir()
            .context("cannot determine home directory")?
            .join(".hive")
            .join("themes");
        fs::create_dir_all(&dir)?;
        Ok(Self { themes_dir: dir })
    }

    /// Create a manager pointing at a specific directory (useful for tests).
    #[cfg(test)]
    pub fn with_dir(dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&dir)?;
        Ok(Self { themes_dir: dir })
    }

    /// List all custom themes found on disk.
    pub fn list_custom_themes(&self) -> Vec<ThemeDefinition> {
        let mut themes = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.themes_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|e| e == "json") {
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        if let Ok(def) = serde_json::from_str::<ThemeDefinition>(&content) {
                            themes.push(def);
                        }
                    }
                }
            }
        }
        themes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        themes
    }

    /// Load a specific theme by name (file: `{name}.json`).
    pub fn load_theme(&self, name: &str) -> Result<ThemeDefinition> {
        let sanitized = sanitize_filename(name);
        let path = self.themes_dir.join(format!("{sanitized}.json"));
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Theme file not found: {}", path.display()))?;
        serde_json::from_str(&content).context("Invalid theme JSON")
    }

    /// Save a theme definition to disk.
    pub fn save_theme(&self, def: &ThemeDefinition) -> Result<()> {
        let sanitized = sanitize_filename(&def.name);
        let path = self.themes_dir.join(format!("{sanitized}.json"));
        let json = serde_json::to_string_pretty(def)?;
        fs::write(&path, json)?;
        Ok(())
    }

    /// Delete a theme file.
    pub fn delete_theme(&self, name: &str) -> Result<()> {
        let sanitized = sanitize_filename(name);
        let path = self.themes_dir.join(format!("{sanitized}.json"));
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// Returns all available themes: built-in + custom from disk.
    pub fn all_themes(&self) -> Vec<ThemeDefinition> {
        let mut all = Self::builtin_themes();
        all.extend(self.list_custom_themes());
        all
    }

    /// Returns the 8 built-in theme definitions.
    pub fn builtin_themes() -> Vec<ThemeDefinition> {
        vec![
            Self::dark_def(),
            Self::light_def(),
            Self::nord_def(),
            Self::dracula_def(),
            Self::solarized_dark_def(),
            Self::monokai_def(),
            Self::one_dark_def(),
            Self::github_dark_def(),
        ]
    }

    // -- Built-in theme constructors ----------------------------------------

    /// HiveCode Dark (default)
    pub fn dark_def() -> ThemeDefinition {
        ThemeDefinition {
            name: "HiveCode Dark".into(),
            author: "HiveCode".into(),
            version: "1.0.0".into(),
            description: "Default dark theme with deep navy and electric cyan accents.".into(),
            colors: ThemeColors {
                bg_primary: "#0B101F".into(),
                bg_secondary: "#12192B".into(),
                bg_tertiary: "#1A2644".into(),
                bg_surface: "#141E38".into(),
                accent_primary: "#00F3FF".into(),
                accent_secondary: "#B5E8FF".into(),
                accent_success: "#A7E498".into(),
                accent_warning: "#F9DE8C".into(),
                accent_error: "#FF8FA6".into(),
                accent_info: "#00D4FF".into(),
                accent_pink: "#F5B8DD".into(),
                text_primary: "#EFF4FF".into(),
                text_secondary: "#C0CDEF".into(),
                text_muted: "#8D98B8".into(),
                text_on_accent: "#080812".into(),
                border: "#2A3962".into(),
                border_focus: "#00D4FF".into(),
            },
            fonts: ThemeFonts::default(),
        }
    }

    /// HiveCode Light
    pub fn light_def() -> ThemeDefinition {
        ThemeDefinition {
            name: "HiveCode Light".into(),
            author: "HiveCode".into(),
            version: "1.0.0".into(),
            description: "Clean light theme for daytime use.".into(),
            colors: ThemeColors {
                bg_primary: "#FAFAFC".into(),
                bg_secondary: "#F0F0F5".into(),
                bg_tertiary: "#E5E5EA".into(),
                bg_surface: "#FFFFFF".into(),
                accent_primary: "#0096C7".into(),
                accent_secondary: "#5BB3D5".into(),
                accent_success: "#2D9C4A".into(),
                accent_warning: "#D97706".into(),
                accent_error: "#DC2626".into(),
                accent_info: "#0077B6".into(),
                accent_pink: "#DB2777".into(),
                text_primary: "#1A1A2E".into(),
                text_secondary: "#4A4A68".into(),
                text_muted: "#9A9AB0".into(),
                text_on_accent: "#FFFFFF".into(),
                border: "#D0D0DE".into(),
                border_focus: "#0096C7".into(),
            },
            fonts: ThemeFonts::default(),
        }
    }

    /// Nord
    pub fn nord_def() -> ThemeDefinition {
        ThemeDefinition {
            name: "Nord".into(),
            author: "Arctic Ice Studio".into(),
            version: "1.0.0".into(),
            description: "An arctic, north-bluish color palette.".into(),
            colors: ThemeColors {
                bg_primary: "#2E3440".into(),
                bg_secondary: "#3B4252".into(),
                bg_tertiary: "#434C5E".into(),
                bg_surface: "#4C566A".into(),
                accent_primary: "#88C0D0".into(),
                accent_secondary: "#81A1C1".into(),
                accent_success: "#A3BE8C".into(),
                accent_warning: "#EBCB8B".into(),
                accent_error: "#BF616A".into(),
                accent_info: "#88C0D0".into(),
                accent_pink: "#B48EAD".into(),
                text_primary: "#ECEFF4".into(),
                text_secondary: "#D8DEE9".into(),
                text_muted: "#4C566A".into(),
                text_on_accent: "#2E3440".into(),
                border: "#3B4252".into(),
                border_focus: "#88C0D0".into(),
            },
            fonts: ThemeFonts::default(),
        }
    }

    /// Dracula
    pub fn dracula_def() -> ThemeDefinition {
        ThemeDefinition {
            name: "Dracula".into(),
            author: "Zeno Rocha".into(),
            version: "1.0.0".into(),
            description: "A dark theme with vibrant colors.".into(),
            colors: ThemeColors {
                bg_primary: "#282A36".into(),
                bg_secondary: "#343746".into(),
                bg_tertiary: "#44475A".into(),
                bg_surface: "#383A4A".into(),
                accent_primary: "#BD93F9".into(),
                accent_secondary: "#8BE9FD".into(),
                accent_success: "#50FA7B".into(),
                accent_warning: "#F1FA8C".into(),
                accent_error: "#FF5555".into(),
                accent_info: "#8BE9FD".into(),
                accent_pink: "#FF79C6".into(),
                text_primary: "#F8F8F2".into(),
                text_secondary: "#BFBFBF".into(),
                text_muted: "#6272A4".into(),
                text_on_accent: "#282A36".into(),
                border: "#44475A".into(),
                border_focus: "#BD93F9".into(),
            },
            fonts: ThemeFonts::default(),
        }
    }

    /// Solarized Dark
    pub fn solarized_dark_def() -> ThemeDefinition {
        ThemeDefinition {
            name: "Solarized Dark".into(),
            author: "Ethan Schoonover".into(),
            version: "1.0.0".into(),
            description: "Precision colors for machines and people.".into(),
            colors: ThemeColors {
                bg_primary: "#002B36".into(),
                bg_secondary: "#073642".into(),
                bg_tertiary: "#0A3F4E".into(),
                bg_surface: "#073642".into(),
                accent_primary: "#268BD2".into(),
                accent_secondary: "#2AA198".into(),
                accent_success: "#859900".into(),
                accent_warning: "#B58900".into(),
                accent_error: "#DC322F".into(),
                accent_info: "#2AA198".into(),
                accent_pink: "#D33682".into(),
                text_primary: "#839496".into(),
                text_secondary: "#93A1A1".into(),
                text_muted: "#586E75".into(),
                text_on_accent: "#002B36".into(),
                border: "#073642".into(),
                border_focus: "#268BD2".into(),
            },
            fonts: ThemeFonts::default(),
        }
    }

    /// Monokai
    pub fn monokai_def() -> ThemeDefinition {
        ThemeDefinition {
            name: "Monokai".into(),
            author: "Wimer Hazenberg".into(),
            version: "1.0.0".into(),
            description: "A warm, high-contrast dark theme.".into(),
            colors: ThemeColors {
                bg_primary: "#272822".into(),
                bg_secondary: "#3E3D32".into(),
                bg_tertiary: "#49483E".into(),
                bg_surface: "#353630".into(),
                accent_primary: "#F92672".into(),
                accent_secondary: "#66D9EF".into(),
                accent_success: "#A6E22E".into(),
                accent_warning: "#E6DB74".into(),
                accent_error: "#F92672".into(),
                accent_info: "#66D9EF".into(),
                accent_pink: "#F92672".into(),
                text_primary: "#F8F8F2".into(),
                text_secondary: "#CFCFC2".into(),
                text_muted: "#75715E".into(),
                text_on_accent: "#272822".into(),
                border: "#3E3D32".into(),
                border_focus: "#66D9EF".into(),
            },
            fonts: ThemeFonts::default(),
        }
    }

    /// One Dark
    pub fn one_dark_def() -> ThemeDefinition {
        ThemeDefinition {
            name: "One Dark".into(),
            author: "Atom".into(),
            version: "1.0.0".into(),
            description: "Atom's iconic dark UI theme.".into(),
            colors: ThemeColors {
                bg_primary: "#282C34".into(),
                bg_secondary: "#2C313A".into(),
                bg_tertiary: "#353B45".into(),
                bg_surface: "#31363F".into(),
                accent_primary: "#61AFEF".into(),
                accent_secondary: "#C678DD".into(),
                accent_success: "#98C379".into(),
                accent_warning: "#E5C07B".into(),
                accent_error: "#E06C75".into(),
                accent_info: "#56B6C2".into(),
                accent_pink: "#C678DD".into(),
                text_primary: "#ABB2BF".into(),
                text_secondary: "#8B92A0".into(),
                text_muted: "#5C6370".into(),
                text_on_accent: "#282C34".into(),
                border: "#3E4451".into(),
                border_focus: "#61AFEF".into(),
            },
            fonts: ThemeFonts::default(),
        }
    }

    /// GitHub Dark
    pub fn github_dark_def() -> ThemeDefinition {
        ThemeDefinition {
            name: "GitHub Dark".into(),
            author: "GitHub".into(),
            version: "1.0.0".into(),
            description: "GitHub's official dark theme.".into(),
            colors: ThemeColors {
                bg_primary: "#0D1117".into(),
                bg_secondary: "#161B22".into(),
                bg_tertiary: "#21262D".into(),
                bg_surface: "#161B22".into(),
                accent_primary: "#58A6FF".into(),
                accent_secondary: "#BC8CFF".into(),
                accent_success: "#3FB950".into(),
                accent_warning: "#D29922".into(),
                accent_error: "#F85149".into(),
                accent_info: "#58A6FF".into(),
                accent_pink: "#F778BA".into(),
                text_primary: "#C9D1D9".into(),
                text_secondary: "#8B949E".into(),
                text_muted: "#484F58".into(),
                text_on_accent: "#0D1117".into(),
                border: "#30363D".into(),
                border_focus: "#58A6FF".into(),
            },
            fonts: ThemeFonts::default(),
        }
    }
}

/// Sanitize a theme name for use as a filename.
fn sanitize_filename(name: &str) -> String {
    name.replace(['/', '\\', '.', ' '], "_").to_lowercase()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_definition_json_roundtrip() {
        let def = ThemeManager::dark_def();
        let json = serde_json::to_string_pretty(&def).unwrap();
        let parsed: ThemeDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(def, parsed);
    }

    #[test]
    fn theme_definition_minimal_json_deserialize() {
        // Only required fields -- optional fields should get defaults.
        let json = r##"{
            "name": "Test",
            "author": "Tester",
            "colors": {
                "bg_primary": "#111111",
                "bg_secondary": "#222222",
                "bg_tertiary": "#333333",
                "bg_surface": "#444444",
                "accent_primary": "#55FF55",
                "accent_secondary": "#5555FF",
                "accent_success": "#00FF00",
                "accent_warning": "#FFFF00",
                "accent_error": "#FF0000",
                "text_primary": "#FFFFFF",
                "text_secondary": "#CCCCCC",
                "text_muted": "#888888"
            }
        }"##;
        let def: ThemeDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(def.name, "Test");
        assert_eq!(def.version, "1.0.0"); // default
        assert_eq!(def.fonts.ui, "Inter"); // default
        assert_eq!(def.fonts.mono, "JetBrains Mono"); // default
        assert_eq!(def.colors.text_on_accent, "#080812"); // default
        assert_eq!(def.colors.border, "#2A3962"); // default
        assert!(def.colors.accent_info.is_empty()); // default empty
    }

    #[test]
    fn builtin_themes_count() {
        let themes = ThemeManager::builtin_themes();
        assert_eq!(themes.len(), 8);
    }

    #[test]
    fn builtin_theme_names_unique() {
        let themes = ThemeManager::builtin_themes();
        let mut names: Vec<String> = themes.iter().map(|t| t.name.clone()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 8);
    }

    #[test]
    fn all_builtin_colors_are_valid_hex() {
        for theme in ThemeManager::builtin_themes() {
            let c = &theme.colors;
            for hex in [
                &c.bg_primary,
                &c.bg_secondary,
                &c.bg_tertiary,
                &c.bg_surface,
                &c.accent_primary,
                &c.accent_secondary,
                &c.accent_success,
                &c.accent_warning,
                &c.accent_error,
                &c.accent_info,
                &c.accent_pink,
                &c.text_primary,
                &c.text_secondary,
                &c.text_muted,
                &c.text_on_accent,
                &c.border,
                &c.border_focus,
            ] {
                assert!(
                    hex.starts_with('#') && hex.len() == 7,
                    "Invalid hex color '{}' in theme '{}'",
                    hex,
                    theme.name,
                );
            }
        }
    }

    #[test]
    fn manager_save_load_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = ThemeManager::with_dir(tmp.path().to_path_buf()).unwrap();

        let def = ThemeManager::nord_def();
        mgr.save_theme(&def).unwrap();

        let loaded = mgr.load_theme("Nord").unwrap();
        assert_eq!(loaded.name, "Nord");
        assert_eq!(loaded.colors.bg_primary, "#2E3440");

        let custom = mgr.list_custom_themes();
        assert_eq!(custom.len(), 1);

        mgr.delete_theme("Nord").unwrap();
        assert!(mgr.load_theme("Nord").is_err());
        assert_eq!(mgr.list_custom_themes().len(), 0);
    }

    #[test]
    fn manager_all_themes_includes_builtins() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = ThemeManager::with_dir(tmp.path().to_path_buf()).unwrap();

        let all = mgr.all_themes();
        assert!(all.len() >= 8);
    }

    #[test]
    fn sanitize_filename_works() {
        assert_eq!(sanitize_filename("My Theme"), "my_theme");
        assert_eq!(sanitize_filename("foo/bar\\baz.qux"), "foo_bar_baz_qux");
    }
}
