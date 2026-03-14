use gpui::*;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use hive_ui_core::HiveTheme;

use super::code_block::render_code_block;

/// Render a markdown string into GPUI elements.
pub fn render_markdown(source: &str, theme: &HiveTheme) -> AnyElement {
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(source, options);

    let mut container_children: Vec<AnyElement> = Vec::new();

    // State tracking
    let mut in_code_block = false;
    let mut code_block_content = String::new();
    let mut code_block_lang = String::new();
    let mut bold_active = false;
    let mut emphasis_active = false;
    let mut _in_heading = false;
    let mut heading_level: u8 = 0;
    let mut inline_segments: Vec<AnyElement> = Vec::new();
    let mut _in_list = false;
    let mut list_items: Vec<AnyElement> = Vec::new();
    let mut list_item_segments: Vec<AnyElement> = Vec::new();
    let mut in_list_item = false;

    for event in parser {
        match event {
            // -- Code blocks --
            Event::Start(Tag::CodeBlock(kind)) => {
                flush_inline_segments(&mut inline_segments, &mut container_children, theme);
                in_code_block = true;
                code_block_content.clear();
                code_block_lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                container_children.push(
                    render_code_block(&code_block_content, &code_block_lang, theme)
                        .into_any_element(),
                );
                code_block_content.clear();
                code_block_lang.clear();
            }

            // -- Headings --
            Event::Start(Tag::Heading { level, .. }) => {
                flush_inline_segments(&mut inline_segments, &mut container_children, theme);
                _in_heading = true;
                heading_level = level as u8;
            }
            Event::End(TagEnd::Heading(_)) => {
                _in_heading = false;
                let size = match heading_level {
                    1 => theme.font_size_xl,
                    2 => theme.font_size_lg,
                    _ => theme.font_size_base,
                };
                let heading_el = div()
                    .mt(theme.space_2)
                    .mb(theme.space_1)
                    .text_size(size)
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text_primary)
                    .children(inline_segments.drain(..))
                    .into_any_element();
                container_children.push(heading_el);
                heading_level = 0;
            }

            // -- Paragraphs --
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                flush_inline_segments(&mut inline_segments, &mut container_children, theme);
            }

            // -- Bold / Emphasis --
            Event::Start(Tag::Strong) => {
                bold_active = true;
            }
            Event::End(TagEnd::Strong) => {
                bold_active = false;
            }
            Event::Start(Tag::Emphasis) => {
                emphasis_active = true;
            }
            Event::End(TagEnd::Emphasis) => {
                emphasis_active = false;
            }

            // -- Lists --
            Event::Start(Tag::List(_)) => {
                flush_inline_segments(&mut inline_segments, &mut container_children, theme);
                _in_list = true;
                list_items.clear();
            }
            Event::End(TagEnd::List(_)) => {
                if in_list_item {
                    flush_list_item(&mut list_item_segments, &mut list_items, theme);
                    in_list_item = false;
                }
                _in_list = false;
                let list_el = div()
                    .flex()
                    .flex_col()
                    .gap(theme.space_1)
                    .pl(theme.space_3)
                    .my(theme.space_1)
                    .children(list_items.drain(..))
                    .into_any_element();
                container_children.push(list_el);
            }
            Event::Start(Tag::Item) => {
                if in_list_item {
                    flush_list_item(&mut list_item_segments, &mut list_items, theme);
                }
                in_list_item = true;
                list_item_segments.clear();
            }
            Event::End(TagEnd::Item) => {
                flush_list_item(&mut list_item_segments, &mut list_items, theme);
                in_list_item = false;
            }

            // -- Inline code --
            Event::Code(text) => {
                let code_el = div()
                    .px(theme.space_1)
                    .rounded(theme.radius_sm)
                    .bg(theme.bg_tertiary)
                    .text_size(theme.font_size_sm)
                    .font_family(theme.font_mono.clone())
                    .text_color(theme.accent_powder)
                    .child(text.to_string())
                    .into_any_element();

                if in_list_item {
                    list_item_segments.push(code_el);
                } else {
                    inline_segments.push(code_el);
                }
            }

            // -- Text --
            Event::Text(text) => {
                if in_code_block {
                    code_block_content.push_str(&text);
                } else {
                    let mut el = div()
                        .text_size(theme.font_size_base)
                        .text_color(theme.text_primary);

                    if bold_active {
                        el = el.font_weight(FontWeight::BOLD);
                    }
                    if emphasis_active {
                        el = el.italic();
                    }

                    let el = el.child(text.to_string()).into_any_element();
                    if in_list_item {
                        list_item_segments.push(el);
                    } else {
                        inline_segments.push(el);
                    }
                }
            }

            // -- Breaks --
            Event::SoftBreak | Event::HardBreak => {
                if in_code_block {
                    code_block_content.push('\n');
                }
            }

            // -- Horizontal rule --
            Event::Rule => {
                flush_inline_segments(&mut inline_segments, &mut container_children, theme);
                container_children.push(
                    div()
                        .w_full()
                        .h(px(1.0))
                        .my(theme.space_2)
                        .bg(theme.border)
                        .into_any_element(),
                );
            }

            _ => {}
        }
    }

    flush_inline_segments(&mut inline_segments, &mut container_children, theme);

    div()
        .flex()
        .flex_col()
        .gap(theme.space_1)
        .text_color(theme.text_primary)
        .children(container_children)
        .into_any_element()
}

/// Flush accumulated inline segments into a paragraph element.
fn flush_inline_segments(
    segments: &mut Vec<AnyElement>,
    container: &mut Vec<AnyElement>,
    theme: &HiveTheme,
) {
    if segments.is_empty() {
        return;
    }
    let p = div()
        .flex()
        .flex_wrap()
        .gap(px(0.0))
        .text_size(theme.font_size_base)
        .children(segments.drain(..))
        .into_any_element();
    container.push(p);
}

/// Flush accumulated list item segments into a list item element.
fn flush_list_item(
    segments: &mut Vec<AnyElement>,
    list_items: &mut Vec<AnyElement>,
    theme: &HiveTheme,
) {
    if segments.is_empty() {
        return;
    }
    let item = div()
        .flex()
        .flex_wrap()
        .gap(px(0.0))
        .text_size(theme.font_size_base)
        .child(
            div()
                .text_color(theme.text_muted)
                .mr(theme.space_1)
                .child("\u{2022}"),
        )
        .children(segments.drain(..))
        .into_any_element();
    list_items.push(item);
}
