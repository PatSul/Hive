use hive_ai::embeddings::{
    EmbeddingProvider, MockEmbeddingProvider, OllamaEmbeddings, OpenAiEmbeddings,
};

#[tokio::test]
async fn test_mock_embedding_provider_returns_correct_dimensions() {
    let provider = MockEmbeddingProvider::new(384);
    let result = provider.embed(&["hello world"]).await.unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].len(), 384);
}

#[tokio::test]
async fn test_mock_embedding_provider_batch_embed() {
    let provider = MockEmbeddingProvider::new(384);
    let texts = vec!["hello", "world", "test"];
    let result = provider.embed(&texts).await.unwrap();
    assert_eq!(result.len(), 3);
    for emb in &result {
        assert_eq!(emb.len(), 384);
    }
}

#[tokio::test]
async fn test_embedding_provider_metadata() {
    let provider = MockEmbeddingProvider::new(384);
    assert_eq!(provider.dimensions(), 384);
    assert_eq!(provider.model_name(), "mock-384");
}

#[tokio::test]
async fn test_openai_embeddings_unavailable_without_key() {
    let provider = OpenAiEmbeddings::new(String::new());
    assert!(!provider.is_available().await);
}

#[tokio::test]
async fn test_openai_embeddings_metadata() {
    let provider = OpenAiEmbeddings::new("sk-test".to_string());
    assert_eq!(provider.model_name(), "text-embedding-3-small");
    assert_eq!(provider.dimensions(), 1536);
}

#[tokio::test]
async fn test_openai_embeddings_empty_key_returns_auth_error() {
    let provider = OpenAiEmbeddings::new(String::new());
    let result = provider.embed(&["test"]).await;
    assert!(matches!(
        result,
        Err(hive_ai::embeddings::EmbeddingError::AuthError(_))
    ));
}

#[tokio::test]
async fn test_ollama_embeddings_metadata() {
    let provider = OllamaEmbeddings::new("http://localhost:11434".to_string());
    assert_eq!(provider.model_name(), "nomic-embed-text");
    assert_eq!(provider.dimensions(), 768);
}

#[tokio::test]
async fn test_ollama_embeddings_unavailable_when_not_running() {
    // Use a port that's definitely not running Ollama
    let provider = OllamaEmbeddings::new("http://localhost:1".to_string());
    assert!(!provider.is_available().await);
}

#[tokio::test]
async fn test_ollama_embeddings_returns_network_error_when_down() {
    let provider = OllamaEmbeddings::new("http://localhost:1".to_string());
    let result = provider.embed(&["test"]).await;
    assert!(matches!(
        result,
        Err(hive_ai::embeddings::EmbeddingError::Network(_))
    ));
}
