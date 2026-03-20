use std::sync::Arc;

use arrow_array::{
    BooleanArray, FixedSizeListArray, Float32Array, RecordBatch, RecordBatchIterator, StringArray,
    UInt32Array, types::Float32Type,
};
use arrow_schema::{DataType, Field, Schema};
use lancedb::connect;
use lancedb::query::{ExecutableQuery, QueryBase};

use super::types::*;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

/// Helper: extract a string column from a RecordBatch by name
fn str_col<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a StringArray> {
    let col = batch.column_by_name(name)?;
    col.as_any().downcast_ref::<StringArray>()
}

/// Helper: extract a Float32 column from a RecordBatch by name
fn f32_col<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a Float32Array> {
    let col = batch.column_by_name(name)?;
    col.as_any().downcast_ref::<Float32Array>()
}

/// Helper: extract a UInt32 column from a RecordBatch by name
fn u32_col<'a>(batch: &'a RecordBatch, name: &str) -> Option<&'a UInt32Array> {
    let col = batch.column_by_name(name)?;
    col.as_any().downcast_ref::<UInt32Array>()
}

pub struct MemoryStore {
    db: lancedb::Connection,
    vector_dim: usize,
}

impl MemoryStore {
    pub async fn open(path: &str) -> Result<Self, BoxErr> {
        let db = connect(path).execute().await?;
        let store = Self {
            db,
            vector_dim: 768,
        };
        store.ensure_tables().await?;
        Ok(store)
    }

    pub fn with_dimensions(mut self, dim: usize) -> Self {
        self.vector_dim = dim;
        self
    }

    async fn ensure_tables(&self) -> Result<(), BoxErr> {
        let table_names = self.db.table_names().execute().await?;

        if !table_names.contains(&"chunks".to_string()) {
            self.create_chunks_table().await?;
        }
        if !table_names.contains(&"memories".to_string()) {
            self.create_memories_table().await?;
        }
        Ok(())
    }

    fn chunks_schema(&self) -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("source_file", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, false),
            Field::new(
                "embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    self.vector_dim as i32,
                ),
                false,
            ),
            Field::new("start_line", DataType::UInt32, false),
            Field::new("end_line", DataType::UInt32, false),
        ]))
    }

    fn memories_schema(&self) -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, false),
            Field::new(
                "embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    self.vector_dim as i32,
                ),
                false,
            ),
            Field::new("category", DataType::Utf8, false),
            Field::new("importance", DataType::Float32, false),
            Field::new("conversation_id", DataType::Utf8, false),
            Field::new("timestamp", DataType::Utf8, false),
            Field::new("decay_exempt", DataType::Boolean, false),
        ]))
    }

    fn make_embedding_array(&self, embedding: &[f32]) -> FixedSizeListArray {
        FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            vec![Some(embedding.iter().map(|v| Some(*v)).collect::<Vec<_>>())],
            self.vector_dim as i32,
        )
    }

    async fn create_chunks_table(&self) -> Result<(), BoxErr> {
        let schema = self.chunks_schema();
        let batch = RecordBatch::new_empty(schema.clone());
        let batches: RecordBatchIterator<_> = RecordBatchIterator::new(
            vec![Ok(batch) as Result<RecordBatch, arrow_schema::ArrowError>],
            schema,
        );
        self.db
            .create_table("chunks", Box::new(batches))
            .execute()
            .await?;
        Ok(())
    }

    async fn create_memories_table(&self) -> Result<(), BoxErr> {
        let schema = self.memories_schema();
        let batch = RecordBatch::new_empty(schema.clone());
        let batches: RecordBatchIterator<_> = RecordBatchIterator::new(
            vec![Ok(batch) as Result<RecordBatch, arrow_schema::ArrowError>],
            schema,
        );
        self.db
            .create_table("memories", Box::new(batches))
            .execute()
            .await?;
        Ok(())
    }

    pub async fn remember(&self, entry: MemoryEntry, embedding: &[f32]) -> Result<(), BoxErr> {
        let table = self.db.open_table("memories").execute().await?;
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();

        let schema = table.schema().await?;
        let embedding_array = self.make_embedding_array(embedding);

        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec![id.as_str()])),
                Arc::new(StringArray::from(vec![entry.content.as_str()])),
                Arc::new(embedding_array),
                Arc::new(StringArray::from(vec![entry.category.as_str()])),
                Arc::new(Float32Array::from(vec![entry.importance])),
                Arc::new(StringArray::from(vec![entry.conversation_id.as_str()])),
                Arc::new(StringArray::from(vec![timestamp.as_str()])),
                Arc::new(BooleanArray::from(vec![entry.decay_exempt])),
            ],
        )?;

        let batches: RecordBatchIterator<_> = RecordBatchIterator::new(
            vec![Ok(batch.clone()) as Result<RecordBatch, arrow_schema::ArrowError>],
            batch.schema(),
        );
        table.add(Box::new(batches)).execute().await?;
        Ok(())
    }

    pub async fn recall(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<MemoryResult>, BoxErr> {
        let table = self.db.open_table("memories").execute().await?;

        use futures::TryStreamExt;
        let results: Vec<RecordBatch> = table
            .query()
            .nearest_to(query_embedding)?
            .limit(limit)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut memories = Vec::new();
        for batch in &results {
            let content = str_col(batch, "content");
            let cat = str_col(batch, "category");
            let imp = f32_col(batch, "importance");
            let ts = str_col(batch, "timestamp");
            let dist = f32_col(batch, "_distance");

            if let (Some(content), Some(cat), Some(imp), Some(ts)) = (content, cat, imp, ts) {
                for i in 0..batch.num_rows() {
                    let score = dist.map(|d| 1.0 - d.value(i)).unwrap_or(0.0);
                    memories.push(MemoryResult {
                        content: content.value(i).to_string(),
                        category: cat.value(i).to_string(),
                        importance: imp.value(i),
                        score,
                        timestamp: ts.value(i).to_string(),
                    });
                }
            }
        }
        Ok(memories)
    }

    pub async fn index_chunk(
        &self,
        source_file: &str,
        content: &str,
        embedding: &[f32],
        start_line: u32,
        end_line: u32,
    ) -> Result<(), BoxErr> {
        let table = self.db.open_table("chunks").execute().await?;
        let id = uuid::Uuid::new_v4().to_string();

        let schema = table.schema().await?;
        let embedding_array = self.make_embedding_array(embedding);

        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec![id.as_str()])),
                Arc::new(StringArray::from(vec![source_file])),
                Arc::new(StringArray::from(vec![content])),
                Arc::new(embedding_array),
                Arc::new(UInt32Array::from(vec![start_line])),
                Arc::new(UInt32Array::from(vec![end_line])),
            ],
        )?;

        let batches: RecordBatchIterator<_> = RecordBatchIterator::new(
            vec![Ok(batch.clone()) as Result<RecordBatch, arrow_schema::ArrowError>],
            batch.schema(),
        );
        table.add(Box::new(batches)).execute().await?;
        Ok(())
    }

    pub async fn search_chunks(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<ChunkResult>, BoxErr> {
        let table = self.db.open_table("chunks").execute().await?;

        use futures::TryStreamExt;
        let results: Vec<RecordBatch> = table
            .query()
            .nearest_to(query_embedding)?
            .limit(limit)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let mut chunks = Vec::new();
        for batch in &results {
            let file = str_col(batch, "source_file");
            let content = str_col(batch, "content");
            let start = u32_col(batch, "start_line");
            let end = u32_col(batch, "end_line");
            let dist = f32_col(batch, "_distance");

            if let (Some(file), Some(content), Some(start), Some(end)) = (file, content, start, end)
            {
                for i in 0..batch.num_rows() {
                    let score = dist.map(|d| 1.0 - d.value(i)).unwrap_or(0.0);
                    chunks.push(ChunkResult {
                        source_file: file.value(i).to_string(),
                        content: content.value(i).to_string(),
                        start_line: start.value(i),
                        end_line: end.value(i),
                        score,
                    });
                }
            }
        }
        Ok(chunks)
    }

    pub async fn stats(&self) -> Result<StoreStats, BoxErr> {
        let chunks_table = self.db.open_table("chunks").execute().await?;
        let memories_table = self.db.open_table("memories").execute().await?;

        let chunk_count = chunks_table.count_rows(None).await?;
        let memory_count = memories_table.count_rows(None).await?;

        Ok(StoreStats {
            total_chunks: chunk_count,
            total_memories: memory_count,
            indexed_files: 0,
        })
    }

    pub async fn clear_chunks(&self) -> Result<(), BoxErr> {
        self.db.drop_table("chunks", &[]).await?;
        self.create_chunks_table().await?;
        Ok(())
    }

    pub async fn clear_memories(&self) -> Result<(), BoxErr> {
        self.db.drop_table("memories", &[]).await?;
        self.create_memories_table().await?;
        Ok(())
    }
}
