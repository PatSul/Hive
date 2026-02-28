use hive_ai::embeddings::{EmbeddingProvider, MockEmbeddingProvider};

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
