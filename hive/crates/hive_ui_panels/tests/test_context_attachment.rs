use hive_ui_panels::components::context_attachment::{
    format_token_count, AttachedContext, AttachedFile,
};

#[test]
fn format_token_count_small() {
    assert_eq!(format_token_count(0), "0");
    assert_eq!(format_token_count(500), "500");
    assert_eq!(format_token_count(999), "999");
}

#[test]
fn format_token_count_thousands() {
    assert_eq!(format_token_count(1_000), "1.0k");
    assert_eq!(format_token_count(1_500), "1.5k");
    assert_eq!(format_token_count(12_345), "12.3k");
}

#[test]
fn format_token_count_millions() {
    assert_eq!(format_token_count(1_000_000), "1.0M");
    assert_eq!(format_token_count(2_500_000), "2.5M");
}

#[test]
fn attached_file_tokens_display() {
    let file = AttachedFile {
        name: "main.rs".into(),
        path: "/src/main.rs".into(),
        tokens: 3_200,
        source_type: "file".into(),
    };
    assert_eq!(file.tokens_display(), "3.2k");
}

#[test]
fn attached_context_empty() {
    let ctx = AttachedContext::empty();
    assert!(ctx.is_empty());
    assert_eq!(ctx.total_tokens, 0);
}

#[test]
fn attached_context_recalculate_tokens() {
    let mut ctx = AttachedContext {
        files: vec![
            AttachedFile {
                name: "a.rs".into(),
                path: "/a.rs".into(),
                tokens: 100,
                source_type: "file".into(),
            },
            AttachedFile {
                name: "b.rs".into(),
                path: "/b.rs".into(),
                tokens: 250,
                source_type: "file".into(),
            },
        ],
        total_tokens: 0,
    };
    ctx.recalculate_tokens();
    assert_eq!(ctx.total_tokens, 350);
}

#[test]
fn attached_context_is_empty_with_files() {
    let ctx = AttachedContext {
        files: vec![AttachedFile {
            name: "test.rs".into(),
            path: "/test.rs".into(),
            tokens: 50,
            source_type: "file".into(),
        }],
        total_tokens: 50,
    };
    assert!(!ctx.is_empty());
}
