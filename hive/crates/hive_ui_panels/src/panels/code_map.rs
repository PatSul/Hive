use std::collections::HashMap;

use gpui::*;

use hive_ui_core::{AppQuickIndex, HiveTheme};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Display-ready symbol entry for the CodeMap panel.
#[derive(Debug, Clone)]
pub struct CodeMapSymbol {
    pub name: String,
    pub kind: String,
    pub file: String,
}

/// Data backing the CodeMap panel.
#[derive(Debug, Clone, Default)]
pub struct CodeMapData {
    /// Symbols grouped by file path.
    pub by_file: Vec<(String, Vec<CodeMapSymbol>)>,
    /// Total symbol count.
    pub total_symbols: usize,
    /// Search/filter query.
    pub filter_query: String,
}

impl Global for CodeMapData {}

impl CodeMapData {
    /// Build from the AppQuickIndex global.
    pub fn from_quick_index(qi: &hive_ai::QuickIndex) -> Self {
        let mut by_file_map: HashMap<String, Vec<CodeMapSymbol>> = HashMap::new();
        let total = qi.key_symbols.len();

        for sym in &qi.key_symbols {
            by_file_map
                .entry(sym.file.clone())
                .or_default()
                .push(CodeMapSymbol {
                    name: sym.name.clone(),
                    kind: format!("{:?}", sym.kind),
                    file: sym.file.clone(),
                });
        }

        let mut by_file: Vec<(String, Vec<CodeMapSymbol>)> = by_file_map.into_iter().collect();
        by_file.sort_by(|a, b| a.0.cmp(&b.0));

        Self {
            by_file,
            total_symbols: total,
            filter_query: String::new(),
        }
    }

    /// Filter symbols by a query string (matches name or file path).
    pub fn filtered(&self) -> Vec<(String, Vec<CodeMapSymbol>)> {
        if self.filter_query.is_empty() {
            return self.by_file.clone();
        }
        let q = self.filter_query.to_lowercase();
        self.by_file
            .iter()
            .filter_map(|(file, syms)| {
                let matched: Vec<CodeMapSymbol> = syms
                    .iter()
                    .filter(|s| {
                        s.name.to_lowercase().contains(&q)
                            || s.kind.to_lowercase().contains(&q)
                            || file.to_lowercase().contains(&q)
                    })
                    .cloned()
                    .collect();
                if matched.is_empty() {
                    None
                } else {
                    Some((file.clone(), matched))
                }
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render the CodeMap panel content.
pub fn render_code_map(data: &CodeMapData, theme: &HiveTheme) -> impl IntoElement {
    let filtered = data.filtered();
    let total = data.total_symbols;
    let file_count = filtered.len();

    div()
        .id("code-map-panel")
        .flex()
        .flex_col()
        .size_full()
        .overflow_y_scroll()
        .bg(theme.bg_primary)
        .child(
            // Header
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(theme.space_4)
                .py(theme.space_3)
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(theme.space_2)
                        .child(
                            div()
                                .text_size(theme.font_size_lg)
                                .text_color(theme.text_primary)
                                .font_weight(FontWeight::BOLD)
                                .child("Code Map"),
                        )
                        .child(
                            div()
                                .text_size(theme.font_size_xs)
                                .text_color(theme.text_muted)
                                .child(format!("{} symbols in {} files", total, file_count)),
                        ),
                ),
        )
        .children(if filtered.is_empty() {
            vec![render_empty_state(theme).into_any_element()]
        } else {
            filtered
                .into_iter()
                .map(|(file, syms)| render_file_group(&file, &syms, theme).into_any_element())
                .collect()
        })
}

/// Render empty state when no symbols are found.
fn render_empty_state(theme: &HiveTheme) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .py(theme.space_8)
        .gap(theme.space_2)
        .child(
            div()
                .text_size(theme.font_size_lg)
                .text_color(theme.text_muted)
                .child("No symbols indexed yet"),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child("Symbols will appear after the project is indexed"),
        )
}

/// Render a collapsible file group with its symbols.
fn render_file_group(
    file: &str,
    symbols: &[CodeMapSymbol],
    theme: &HiveTheme,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .w_full()
        .border_b_1()
        .border_color(theme.border)
        .child(
            // File header
            div()
                .flex()
                .items_center()
                .gap(theme.space_2)
                .px(theme.space_4)
                .py(theme.space_2)
                .bg(theme.bg_secondary)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.accent_aqua)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(file.to_string()),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(format!("({})", symbols.len())),
                ),
        )
        .children(
            symbols
                .iter()
                .map(|sym| render_symbol_row(sym, theme).into_any_element()),
        )
}

/// Render a single symbol row.
fn render_symbol_row(sym: &CodeMapSymbol, theme: &HiveTheme) -> impl IntoElement {
    let kind_color = match sym.kind.as_str() {
        "Function" => theme.accent_aqua,
        "Struct" => theme.accent_green,
        "Trait" | "Interface" => theme.accent_yellow,
        "Enum" => theme.accent_pink,
        _ => theme.text_muted,
    };

    div()
        .flex()
        .items_center()
        .gap(theme.space_3)
        .px(theme.space_4)
        .pl(theme.space_6)
        .py(theme.space_1)
        .hover(|s| s.bg(theme.bg_tertiary))
        .child(
            // Kind badge
            div()
                .text_size(theme.font_size_xs)
                .text_color(kind_color)
                .min_w(px(60.0))
                .child(sym.kind.clone()),
        )
        .child(
            // Symbol name
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_primary)
                .child(sym.name.clone()),
        )
}

/// Build CodeMapData from the AppQuickIndex global if available.
pub fn build_code_map_data(cx: &App) -> CodeMapData {
    if cx.has_global::<AppQuickIndex>() {
        CodeMapData::from_quick_index(&cx.global::<AppQuickIndex>().0)
    } else {
        CodeMapData::default()
    }
}
