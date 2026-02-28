use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::hive_memory::HiveMemory;
use tracing;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

/// Known code file extensions worth indexing
const CODE_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "ts", "tsx", "jsx", "go", "c", "cpp", "h", "hpp", "java", "kt", "rb",
    "php", "swift", "cs", "lua", "sh", "bash", "zsh", "toml", "yaml", "yml", "json", "md",
    "html", "css", "scss", "sql", "proto", "graphql",
];

/// Background indexer that walks a directory and indexes code files into HiveMemory.
pub struct BackgroundIndexer {
    memory: Arc<HiveMemory>,
    /// Track file hashes to skip unchanged files
    file_hashes: HashMap<PathBuf, u64>,
}

impl BackgroundIndexer {
    pub fn new(memory: Arc<HiveMemory>) -> Self {
        Self {
            memory,
            file_hashes: HashMap::new(),
        }
    }

    /// Index all code files in a directory recursively. Returns the number of files indexed.
    pub async fn index_directory(&mut self, dir_path: &str) -> Result<usize, BoxErr> {
        let mut count = 0;
        let entries = self.collect_files(Path::new(dir_path))?;

        for path in entries {
            match self.index_single_file(&path).await {
                Ok(indexed) => {
                    if indexed {
                        count += 1;
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to index {}: {}", path.display(), e);
                }
            }
        }

        Ok(count)
    }

    /// Index a single file. Returns true if the file was indexed (not skipped).
    pub async fn index_single_file(&mut self, path: &Path) -> Result<bool, BoxErr> {
        if !Self::is_indexable(path) {
            return Ok(false);
        }

        let content = std::fs::read_to_string(path)?;

        // Check if content is binary-looking (many null bytes)
        if content.bytes().take(512).filter(|b| *b == 0).count() > 4 {
            return Ok(false);
        }

        // Skip empty files
        if content.trim().is_empty() {
            return Ok(false);
        }

        // Hash check — skip if unchanged
        let hash = Self::hash_content(&content);
        if self.file_hashes.get(path) == Some(&hash) {
            return Ok(false);
        }

        let rel_path = path.to_string_lossy().to_string();
        self.memory.index_file(&rel_path, &content).await?;
        self.file_hashes.insert(path.to_path_buf(), hash);

        Ok(true)
    }

    /// Collect all indexable files from a directory recursively.
    fn collect_files(&self, dir: &Path) -> Result<Vec<PathBuf>, BoxErr> {
        let mut files = Vec::new();
        self.walk_dir_entries(dir, &mut files)?;
        Ok(files)
    }

    fn walk_dir_entries(&self, dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), BoxErr> {
        if !dir.is_dir() {
            return Ok(());
        }

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if path.is_dir() {
                // Skip hidden dirs, node_modules, target, .git, etc.
                if name.starts_with('.')
                    || name == "node_modules"
                    || name == "target"
                    || name == "__pycache__"
                {
                    continue;
                }
                self.walk_dir_entries(&path, files)?;
            } else if Self::is_indexable(&path) {
                files.push(path);
            }
        }

        Ok(())
    }

    /// Collect all indexable code file paths from a directory (public static helper).
    /// Useful when the caller wants to drive indexing directly via HiveMemory.
    pub fn collect_indexable_files(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        Self::walk_static(dir, &mut files);
        files
    }

    fn walk_static(dir: &Path, files: &mut Vec<PathBuf>) {
        if !dir.is_dir() {
            return;
        }
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_dir() {
                if name.starts_with('.')
                    || name == "node_modules"
                    || name == "target"
                    || name == "__pycache__"
                {
                    continue;
                }
                Self::walk_static(&path, files);
            } else if Self::is_indexable(&path) {
                files.push(path);
            }
        }
    }

    fn is_indexable(path: &Path) -> bool {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        CODE_EXTENSIONS.contains(&ext)
    }

    fn hash_content(content: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        hasher.finish()
    }
}
