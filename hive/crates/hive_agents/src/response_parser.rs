/// Parse AI responses for file-targeted code blocks that can be applied.
///
/// Supports two formats:
/// 1. Fenced code blocks with path: ```rust:src/main.rs
/// 2. XML edit tags: <edit path="src/main.rs">content</edit>

/// A parsed edit extracted from an AI response.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedEdit {
    pub file_path: String,
    pub new_content: String,
    pub language: String,
}

/// Parse all file-targeted edits from an AI response string.
pub fn parse_edits(response: &str) -> Vec<ParsedEdit> {
    let mut edits = Vec::new();
    edits.extend(parse_fenced_edits(response));
    edits.extend(parse_xml_edits(response));
    edits
}

/// Parse fenced code blocks with `lang:path` format.
/// Example: ```rust:src/main.rs
fn parse_fenced_edits(response: &str) -> Vec<ParsedEdit> {
    let mut edits = Vec::new();
    let mut lines = response.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if !trimmed.starts_with("```") || trimmed == "```" {
            continue;
        }

        // Strip the leading backticks
        let info_string = trimmed.trim_start_matches('`');
        if info_string.is_empty() {
            continue;
        }

        // Check for lang:path format
        let (lang, path) = if let Some(colon_pos) = info_string.find(':') {
            let lang = &info_string[..colon_pos];
            let path = &info_string[colon_pos + 1..];
            if path.is_empty() || lang.is_empty() {
                continue;
            }
            (lang.to_string(), path.to_string())
        } else {
            continue; // No path, skip
        };

        // Collect code content until closing ```
        let mut content = String::new();
        for inner_line in lines.by_ref() {
            if inner_line.trim() == "```" {
                break;
            }
            if !content.is_empty() {
                content.push('\n');
            }
            content.push_str(inner_line);
        }

        edits.push(ParsedEdit {
            file_path: path,
            new_content: content,
            language: lang,
        });
    }

    edits
}

/// Parse XML `<edit path="...">content</edit>` tags.
fn parse_xml_edits(response: &str) -> Vec<ParsedEdit> {
    let mut edits = Vec::new();
    let mut search_from = 0;

    while search_from < response.len() {
        let remaining = &response[search_from..];

        // Find <edit path="...">
        let Some(tag_start) = remaining.find("<edit ") else {
            break;
        };
        let abs_tag_start = search_from + tag_start;
        let tag_region = &response[abs_tag_start..];

        // Find the closing >
        let Some(tag_end) = tag_region.find('>') else {
            search_from = abs_tag_start + 6;
            continue;
        };
        let opening_tag = &tag_region[..tag_end + 1];

        // Extract path attribute
        let path = extract_attribute(opening_tag, "path");
        let lang = extract_attribute(opening_tag, "lang").unwrap_or_default();
        let Some(path) = path else {
            search_from = abs_tag_start + tag_end + 1;
            continue;
        };

        // Find </edit>
        let content_start = abs_tag_start + tag_end + 1;
        let Some(close_pos) = response[content_start..].find("</edit>") else {
            search_from = content_start;
            continue;
        };
        let content = &response[content_start..content_start + close_pos];

        // Strip CDATA wrapper if present
        let content = content
            .trim()
            .strip_prefix("<![CDATA[")
            .and_then(|s| s.strip_suffix("]]>"))
            .unwrap_or(content);

        edits.push(ParsedEdit {
            file_path: path,
            new_content: content.to_string(),
            language: lang,
        });

        search_from = content_start + close_pos + 7; // past </edit>
    }

    edits
}

/// Extract an attribute value from an XML-like opening tag.
fn extract_attribute(tag: &str, attr_name: &str) -> Option<String> {
    let pattern = format!("{}=\"", attr_name);
    let start = tag.find(&pattern)? + pattern.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fenced_code_block_with_path() {
        let response = "Here's the fix:\n```rust:src/main.rs\nfn main() {\n    println!(\"hello\");\n}\n```\n";
        let edits = parse_edits(response);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].file_path, "src/main.rs");
        assert_eq!(edits[0].language, "rust");
        assert_eq!(edits[0].new_content, "fn main() {\n    println!(\"hello\");\n}");
    }

    #[test]
    fn parse_xml_edit_tag() {
        let response = r#"Apply this: <edit path="src/lib.rs" lang="rust">pub fn add(a: i32, b: i32) -> i32 { a + b }</edit>"#;
        let edits = parse_edits(response);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].file_path, "src/lib.rs");
        assert_eq!(edits[0].new_content, "pub fn add(a: i32, b: i32) -> i32 { a + b }");
    }

    #[test]
    fn parse_xml_edit_with_cdata() {
        let response = r#"<edit path="src/main.rs"><![CDATA[fn main() {}]]></edit>"#;
        let edits = parse_edits(response);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_content, "fn main() {}");
    }

    #[test]
    fn parse_multiple_edits() {
        let response = "```rust:src/a.rs\nlet a = 1;\n```\nSome text\n```py:src/b.py\nprint('hi')\n```\n";
        let edits = parse_edits(response);
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0].file_path, "src/a.rs");
        assert_eq!(edits[1].file_path, "src/b.py");
    }

    #[test]
    fn skip_code_blocks_without_path() {
        let response = "```rust\nfn foo() {}\n```\n";
        let edits = parse_edits(response);
        assert!(edits.is_empty());
    }

    #[test]
    fn mixed_fenced_and_xml() {
        let response = "```ts:src/app.ts\nconsole.log('hi');\n```\n<edit path=\"src/style.css\" lang=\"css\">body { color: red; }</edit>";
        let edits = parse_edits(response);
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0].language, "ts");
        assert_eq!(edits[1].language, "css");
    }

    #[test]
    fn empty_response() {
        assert!(parse_edits("").is_empty());
    }

    #[test]
    fn malformed_xml_skipped() {
        let response = r#"<edit path="foo.rs">content"#; // no closing tag
        let edits = parse_edits(response);
        assert!(edits.is_empty());
    }
}
