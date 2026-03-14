use gpui::*;

use hive_ui_core::{ApplyCodeBlock, CopyToClipboard, HiveTheme};

/// Render a syntax-highlighted-style code block with line numbers and a language label.
pub fn render_code_block(code: &str, language: &str, theme: &HiveTheme) -> impl IntoElement {
    render_code_block_inner(code, language, None, theme)
}

/// Render a code block with a file path header + "Apply" button.
pub fn render_code_block_with_path(
    code: &str,
    language: &str,
    file_path: &str,
    theme: &HiveTheme,
) -> impl IntoElement {
    render_code_block_inner(code, language, Some(file_path), theme)
}

fn render_code_block_inner(
    code: &str,
    language: &str,
    file_path: Option<&str>,
    theme: &HiveTheme,
) -> impl IntoElement {
    let language = language.to_string();
    let lines: Vec<String> = code.lines().map(String::from).collect();
    let line_count = lines.len();
    let code_for_copy = code.to_string();

    let mut code_bg = theme.bg_primary;
    code_bg.a = 0.85;

    // Header label: show file path if available, otherwise language
    let label = if let Some(fp) = file_path {
        fp.to_string()
    } else if !language.is_empty() {
        language.clone()
    } else {
        "code".to_string()
    };

    // Build action buttons
    let copy_content = code_for_copy.clone();
    let copy_id = SharedString::from(format!("copy-{}", label));
    let mut buttons = div()
        .flex()
        .items_center()
        .gap(theme.space_2);

    // Copy button
    buttons = buttons.child(
        div()
            .id(copy_id)
            .text_size(theme.font_size_xs)
            .text_color(theme.text_muted)
            .px(theme.space_2)
            .py(theme.space_1)
            .rounded(theme.radius_sm)
            .cursor_pointer()
            .hover(|s| s.text_color(theme.text_primary))
            .on_mouse_down(MouseButton::Left, {
                move |_ev, _window, _cx| {
                    _cx.dispatch_action(&CopyToClipboard {
                        content: copy_content.clone(),
                    });
                }
            })
            .child("Copy"),
    );

    // Apply button (only when file_path is present)
    if let Some(fp) = file_path {
        let apply_path = fp.to_string();
        let apply_content = code_for_copy.clone();
        let apply_id = SharedString::from(format!("apply-{}", fp));
        buttons = buttons.child(
            div()
                .id(apply_id)
                .text_size(theme.font_size_xs)
                .text_color(theme.accent_aqua)
                .px(theme.space_2)
                .py(theme.space_1)
                .rounded(theme.radius_sm)
                .cursor_pointer()
                .hover(|s| s.bg(theme.accent_aqua).text_color(theme.bg_primary))
                .on_mouse_down(MouseButton::Left, {
                    move |_ev, _window, _cx| {
                        _cx.dispatch_action(&ApplyCodeBlock {
                            file_path: apply_path.clone(),
                            content: apply_content.clone(),
                        });
                    }
                })
                .child("Apply"),
        );
    }

    div()
        .w_full()
        .rounded(theme.radius_md)
        .bg(code_bg)
        .border_1()
        .border_color(theme.border)
        .overflow_hidden()
        .child(
            // Header
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(theme.space_3)
                .py(theme.space_1)
                .bg(theme.bg_secondary)
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(label),
                )
                .child(buttons),
        )
        .child(
            // Code body with line numbers
            div()
                .id("code-block-body")
                .overflow_y_scroll()
                .px(theme.space_3)
                .py(theme.space_2)
                .children((0..line_count).map(|i| {
                    let line_num = format!("{:>3}", i + 1);
                    let line_text = lines[i].clone();
                    render_code_line(line_num, line_text, theme)
                })),
        )
}

/// Render a single line of code with its line number.
fn render_code_line(line_num: String, line_text: String, theme: &HiveTheme) -> impl IntoElement {
    div()
        .flex()
        .items_start()
        .gap(theme.space_3)
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .min_w(px(28.0))
                .flex_shrink_0()
                .child(line_num),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_primary)
                .child(line_text),
        )
}
