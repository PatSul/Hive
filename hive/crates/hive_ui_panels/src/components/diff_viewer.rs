use gpui::*;

use hive_ui_core::HiveTheme;

/// A single line in a unified diff view.
#[derive(Debug, Clone)]
pub enum DiffLine {
    Added(String),
    Removed(String),
    Context(String),
}

/// Compute a simple line-by-line diff between old and new content.
pub fn compute_diff_lines_public(old: &str, new: &str) -> Vec<DiffLine> {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    let mut result = Vec::new();
    let mut oi = 0;
    let mut ni = 0;
    while oi < old_lines.len() && ni < new_lines.len() {
        if old_lines[oi] == new_lines[ni] {
            result.push(DiffLine::Context(old_lines[oi].to_string()));
            oi += 1;
            ni += 1;
        } else {
            // Look ahead in new for a match of current old line.
            let mut found_in_new = None;
            for j in (ni + 1)..new_lines.len().min(ni + 5) {
                if new_lines[j] == old_lines[oi] {
                    found_in_new = Some(j);
                    break;
                }
            }
            if let Some(j) = found_in_new {
                for k in ni..j {
                    result.push(DiffLine::Added(new_lines[k].to_string()));
                }
                ni = j;
            } else {
                let mut found_in_old = None;
                for j in (oi + 1)..old_lines.len().min(oi + 5) {
                    if old_lines[j] == new_lines[ni] {
                        found_in_old = Some(j);
                        break;
                    }
                }
                if let Some(j) = found_in_old {
                    for k in oi..j {
                        result.push(DiffLine::Removed(old_lines[k].to_string()));
                    }
                    oi = j;
                } else {
                    result.push(DiffLine::Removed(old_lines[oi].to_string()));
                    result.push(DiffLine::Added(new_lines[ni].to_string()));
                    oi += 1;
                    ni += 1;
                }
            }
        }
    }
    while oi < old_lines.len() {
        result.push(DiffLine::Removed(old_lines[oi].to_string()));
        oi += 1;
    }
    while ni < new_lines.len() {
        result.push(DiffLine::Added(new_lines[ni].to_string()));
        ni += 1;
    }
    result
}

/// Render a unified diff view with colored lines and gutter symbols.
pub fn render_diff(lines: &[DiffLine], theme: &HiveTheme) -> impl IntoElement {
    let lines: Vec<DiffLine> = lines.to_vec();

    div()
        .id("diff-viewer")
        .w_full()
        .overflow_y_scroll()
        .rounded(theme.radius_md)
        .bg(theme.bg_primary)
        .border_1()
        .border_color(theme.border)
        .py(theme.space_2)
        .children(
            lines
                .into_iter()
                .enumerate()
                .map(|(i, line)| render_diff_line(i, line, theme)),
        )
}

/// Render a single diff line with gutter symbol and appropriate coloring.
fn render_diff_line(index: usize, line: DiffLine, theme: &HiveTheme) -> impl IntoElement {
    let (gutter, text, bg, text_color) = match line {
        DiffLine::Added(t) => {
            let mut added_bg = theme.accent_green;
            added_bg.a = 0.10;
            ("+", t, added_bg, theme.accent_green)
        }
        DiffLine::Removed(t) => {
            let mut removed_bg = theme.accent_red;
            removed_bg.a = 0.10;
            ("-", t, removed_bg, theme.accent_red)
        }
        DiffLine::Context(t) => {
            let transparent = hsla(0.0, 0.0, 0.0, 0.0);
            (" ", t, transparent, theme.text_secondary)
        }
    };

    let line_num = format!("{:>3}", index + 1);

    div()
        .flex()
        .items_start()
        .w_full()
        .bg(bg)
        .px(theme.space_3)
        .child(
            // Line number
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .min_w(px(28.0))
                .flex_shrink_0()
                .child(line_num),
        )
        .child(
            // Gutter symbol
            div()
                .text_size(theme.font_size_sm)
                .text_color(text_color)
                .min_w(px(16.0))
                .flex_shrink_0()
                .child(gutter),
        )
        .child(
            // Line content
            div()
                .text_size(theme.font_size_sm)
                .text_color(text_color)
                .child(text),
        )
}
