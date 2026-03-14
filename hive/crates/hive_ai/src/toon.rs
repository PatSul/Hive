//! TOON (Token-Oriented Object Notation) encoding for prompt data.
//!
//! Provides helpers to encode structured data (file trees, symbol lists,
//! dependency lists, git history) in TOON format for ~30-40% token savings
//! when injecting context into LLM prompts.

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

use crate::quick_index::{Dependency, GitEntry, SymbolEntry};

/// Context encoding format for AI prompts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextFormat {
    /// Classic markdown/text formatting (backward-compatible default).
    #[default]
    Markdown,
    /// TOON encoding for token-efficient prompts.
    Toon,
    /// XML structured encoding for explicit context boundaries.
    Xml,
}

impl ContextFormat {
    /// Parse from a config string. Unrecognized values default to Markdown.
    pub fn from_config_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "toon" => Self::Toon,
            "xml" => Self::Xml,
            _ => Self::Markdown,
        }
    }
}

/// Encode file extension counts as a TOON table.
///
/// Produces output like:
/// ```text
/// file_types[4]{ext,count}:
///   rs,150
///   toml,12
///   md,8
///   json,5
/// ```
pub fn encode_file_types(by_extension: &HashMap<String, usize>) -> String {
    if by_extension.is_empty() {
        return String::new();
    }

    let mut exts: Vec<_> = by_extension.iter().collect();
    exts.sort_by(|a, b| b.1.cmp(a.1));
    let top: Vec<_> = exts.into_iter().take(8).collect();

    let rows: Vec<serde_json::Value> = top
        .iter()
        .map(|(ext, count)| json!({"ext": ext, "count": count}))
        .collect();

    let val = json!({"file_types": rows});
    toon_format::encode_default(&val).unwrap_or_else(|_| fallback_file_types(by_extension))
}

/// Encode dependencies as a TOON table.
pub fn encode_dependencies(deps: &[Dependency]) -> String {
    if deps.is_empty() {
        return String::new();
    }

    let rows: Vec<serde_json::Value> = deps
        .iter()
        .take(60)
        .map(|d| json!({"name": d.name, "version": d.version, "source": d.source}))
        .collect();

    let val = json!({"deps": rows});
    toon_format::encode_default(&val).unwrap_or_else(|_| fallback_dependencies(deps))
}

/// Encode key symbols as a TOON table.
pub fn encode_symbols(symbols: &[SymbolEntry]) -> String {
    if symbols.is_empty() {
        return String::new();
    }

    let rows: Vec<serde_json::Value> = symbols
        .iter()
        .take(200)
        .map(|s| json!({"kind": s.kind.to_string(), "name": s.name, "file": s.file}))
        .collect();

    let val = json!({"symbols": rows});
    toon_format::encode_default(&val).unwrap_or_else(|_| fallback_symbols(symbols))
}

/// Encode recent git history as a TOON table.
pub fn encode_git_history(entries: &[GitEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let rows: Vec<serde_json::Value> = entries
        .iter()
        .take(10)
        .map(|e| {
            let age = if e.days_ago == 0 {
                "today".to_string()
            } else if e.days_ago == 1 {
                "yesterday".to_string()
            } else {
                format!("{}d ago", e.days_ago)
            };
            json!({
                "hash": &e.hash[..7.min(e.hash.len())],
                "message": e.message,
                "author": e.author,
                "age": age,
            })
        })
        .collect();

    let val = json!({"commits": rows});
    toon_format::encode_default(&val).unwrap_or_else(|_| fallback_git_history(entries))
}

// ---------------------------------------------------------------------------
// Fallbacks — plain text if TOON encoding fails for any reason
// ---------------------------------------------------------------------------

fn fallback_file_types(by_extension: &HashMap<String, usize>) -> String {
    let mut exts: Vec<_> = by_extension.iter().collect();
    exts.sort_by(|a, b| b.1.cmp(a.1));
    exts.iter()
        .take(8)
        .map(|(ext, count)| format!(".{ext}({count})"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn fallback_dependencies(deps: &[Dependency]) -> String {
    deps.iter()
        .take(60)
        .map(|d| {
            if d.version.is_empty() {
                d.name.clone()
            } else {
                format!("{}@{}", d.name, d.version)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn fallback_symbols(symbols: &[SymbolEntry]) -> String {
    symbols
        .iter()
        .take(200)
        .map(|s| format!("{} {} ({})", s.kind, s.name, s.file))
        .collect::<Vec<_>>()
        .join("\n")
}

fn fallback_git_history(entries: &[GitEntry]) -> String {
    entries
        .iter()
        .take(10)
        .map(|e| {
            format!(
                "{} {} — {}",
                &e.hash[..7.min(e.hash.len())],
                e.message,
                e.author
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quick_index::SymbolKind;

    #[test]
    fn test_encode_file_types_empty() {
        assert_eq!(encode_file_types(&HashMap::new()), "");
    }

    #[test]
    fn test_encode_file_types_produces_toon() {
        let mut map = HashMap::new();
        map.insert("rs".into(), 150);
        map.insert("toml".into(), 12);
        map.insert("md".into(), 8);

        let result = encode_file_types(&map);
        assert!(!result.is_empty());
        // Should contain the tabular schema marker
        assert!(result.contains("file_types["));
        assert!(result.contains("rs"));
        assert!(result.contains("150"));
    }

    #[test]
    fn test_encode_dependencies_empty() {
        assert_eq!(encode_dependencies(&[]), "");
    }

    #[test]
    fn test_encode_dependencies_tabular() {
        let deps = vec![
            Dependency {
                name: "serde".into(),
                version: "1.0".into(),
                source: "Cargo.toml".into(),
            },
            Dependency {
                name: "tokio".into(),
                version: "1.38".into(),
                source: "Cargo.toml".into(),
            },
        ];

        let result = encode_dependencies(&deps);
        assert!(result.contains("deps["));
        assert!(result.contains("serde"));
        assert!(result.contains("tokio"));
    }

    #[test]
    fn test_encode_symbols_empty() {
        assert_eq!(encode_symbols(&[]), "");
    }

    #[test]
    fn test_encode_symbols_tabular() {
        let symbols = vec![
            SymbolEntry {
                name: "QuickIndex".into(),
                kind: SymbolKind::Struct,
                file: "src/quick_index.rs".into(),
            },
            SymbolEntry {
                name: "build".into(),
                kind: SymbolKind::Function,
                file: "src/quick_index.rs".into(),
            },
        ];

        let result = encode_symbols(&symbols);
        assert!(result.contains("symbols["));
        assert!(result.contains("QuickIndex"));
        assert!(result.contains("struct"));
    }

    #[test]
    fn test_encode_git_history_empty() {
        assert_eq!(encode_git_history(&[]), "");
    }

    #[test]
    fn test_encode_git_history_tabular() {
        let entries = vec![
            GitEntry {
                hash: "abc1234def5678".into(),
                message: "feat: add TOON support".into(),
                author: "pat".into(),
                days_ago: 0,
            },
            GitEntry {
                hash: "def5678abc1234".into(),
                message: "fix: typo".into(),
                author: "pat".into(),
                days_ago: 3,
            },
        ];

        let result = encode_git_history(&entries);
        assert!(result.contains("commits["));
        assert!(result.contains("abc1234"));
        assert!(result.contains("today"));
        assert!(result.contains("3d ago"));
    }

    #[test]
    fn test_toon_shorter_than_json() {
        // Build a realistic symbol list and compare TOON vs JSON encoding.
        // TOON's savings come from eliminating repeated keys in uniform arrays.
        let symbols: Vec<SymbolEntry> = (0..50)
            .map(|i| SymbolEntry {
                name: format!("Symbol{i}"),
                kind: if i % 3 == 0 {
                    SymbolKind::Struct
                } else {
                    SymbolKind::Function
                },
                file: format!("src/module_{}.rs", i / 5),
            })
            .collect();

        let toon_output = encode_symbols(&symbols);

        // JSON equivalent — what TOON replaces
        let json_rows: Vec<serde_json::Value> = symbols
            .iter()
            .map(|s| {
                serde_json::json!({"kind": s.kind.to_string(), "name": s.name, "file": s.file})
            })
            .collect();
        let json_output = serde_json::to_string(&json_rows).unwrap();

        assert!(
            toon_output.len() < json_output.len(),
            "TOON ({} chars) should be shorter than JSON ({} chars)",
            toon_output.len(),
            json_output.len()
        );
    }

    #[test]
    fn test_context_format_from_config_str() {
        assert_eq!(ContextFormat::from_config_str("toon"), ContextFormat::Toon);
        assert_eq!(ContextFormat::from_config_str("TOON"), ContextFormat::Toon);
        assert_eq!(
            ContextFormat::from_config_str("markdown"),
            ContextFormat::Markdown
        );
        assert_eq!(
            ContextFormat::from_config_str("unknown"),
            ContextFormat::Markdown
        );
        assert_eq!(ContextFormat::from_config_str(""), ContextFormat::Markdown);
    }
}
