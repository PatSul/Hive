use hive_ai::memory::flush::MemoryExtractor;

#[test]
fn test_parse_extracted_memories_from_json() {
    let json = r#"[
        {"content": "User prefers Rust", "importance": 8, "category": "user_preference"},
        {"content": "Auth uses JWT tokens", "importance": 6, "category": "decision"}
    ]"#;

    let memories = MemoryExtractor::parse_response(json).unwrap();
    assert_eq!(memories.len(), 2);
    assert_eq!(memories[0].content, "User prefers Rust");
    assert_eq!(memories[0].importance, 8.0);
}

#[test]
fn test_parse_filters_low_importance() {
    let json = r#"[
        {"content": "Important thing", "importance": 8, "category": "decision"},
        {"content": "Trivial detail", "importance": 3, "category": "general"}
    ]"#;

    let memories = MemoryExtractor::parse_response(json).unwrap();
    let filtered = MemoryExtractor::filter_by_importance(memories, 5.0);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].content, "Important thing");
}

#[test]
fn test_build_extraction_prompt() {
    let messages = vec![
        "User: How do I fix auth?".to_string(),
        "Assistant: Use JWT tokens with refresh...".to_string(),
    ];
    let prompt = MemoryExtractor::build_prompt(&messages);
    assert!(prompt.contains("extract key memories"));
    assert!(prompt.contains("JSON"));
}

#[test]
fn test_parse_markdown_wrapped_json() {
    let json = "```json\n[{\"content\": \"Test memory\", \"importance\": 7, \"category\": \"general\"}]\n```";
    let memories = MemoryExtractor::parse_response(json).unwrap();
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].content, "Test memory");
}
