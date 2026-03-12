//! Fast-path project indexing for immediate AI context.
//!
//! When Hive opens a project, `QuickIndex::build()` generates a lightweight
//! project map in <3 seconds: file tree, key symbols, dependency graph, and
//! recent git history. This provides immediate context for AI queries while
//! the deeper RAG/vector indexing runs in the background.

use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum number of files to scan for symbols (sorted by modification time).
const MAX_SYMBOL_SCAN_FILES: usize = 200;

/// Maximum total symbols to collect.
const MAX_SYMBOLS: usize = 500;

/// Maximum file size (bytes) for symbol extraction.
const MAX_FILE_SIZE_FOR_SYMBOLS: u64 = 100_000;

/// Directories to always skip during file-tree walks.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".hive-worktrees",
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".cache",
    ".gradle",
    ".idea",
    ".vscode",
];

/// Extensions worth scanning for symbols.
const SYMBOL_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "ts", "tsx", "jsx", "go", "java", "kt", "rb", "cpp", "c", "h", "hpp",
    "cs", "swift",
];

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A fast, lightweight project index built in <3 seconds.
pub struct QuickIndex {
    pub project_root: PathBuf,
    pub file_tree: FileTree,
    pub key_symbols: Vec<SymbolEntry>,
    pub dependencies: Vec<Dependency>,
    pub recent_git: Vec<GitEntry>,
    pub indexed_at: Instant,
}

/// Summary of the project file tree.
pub struct FileTree {
    /// Total number of files (excluding skipped directories).
    pub total_files: usize,
    /// File count by extension, e.g. {"rs": 150, "toml": 12}.
    pub by_extension: HashMap<String, usize>,
    /// Notable top-level directories, e.g. ["src/", "tests/", "crates/"].
    pub key_dirs: Vec<String>,
    /// Human-readable project structure summary.
    pub summary: String,
}

/// A symbol extracted via regex from a source file.
#[derive(Debug, Clone)]
pub struct SymbolEntry {
    pub name: String,
    pub kind: SymbolKind,
    /// Relative path from the project root.
    pub file: String,
}

/// The kind of a discovered symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Struct,
    Trait,
    Enum,
    Module,
    Class,
    Interface,
    Const,
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Function => write!(f, "fn"),
            Self::Struct => write!(f, "struct"),
            Self::Trait => write!(f, "trait"),
            Self::Enum => write!(f, "enum"),
            Self::Module => write!(f, "mod"),
            Self::Class => write!(f, "class"),
            Self::Interface => write!(f, "interface"),
            Self::Const => write!(f, "const"),
        }
    }
}

/// A parsed dependency from a manifest file.
#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub version: String,
    /// The source file, e.g. "Cargo.toml", "package.json".
    pub source: String,
}

/// A recent git log entry.
#[derive(Debug, Clone)]
pub struct GitEntry {
    pub hash: String,
    pub message: String,
    pub author: String,
    pub days_ago: u32,
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

impl QuickIndex {
    /// Build a quick index of a project directory.
    ///
    /// Target: <3 seconds for a 500-file project. Each stage degrades
    /// gracefully if the underlying data source is unavailable.
    pub fn build(project_root: &Path) -> Self {
        let start = Instant::now();

        let file_tree = Self::scan_file_tree(project_root);
        debug!(
            "QuickIndex: file tree scanned in {:?} ({} files)",
            start.elapsed(),
            file_tree.total_files
        );

        let key_symbols = Self::extract_symbols(project_root);
        debug!(
            "QuickIndex: symbols extracted in {:?} ({} symbols)",
            start.elapsed(),
            key_symbols.len()
        );

        let dependencies = Self::parse_dependencies(project_root);
        debug!(
            "QuickIndex: dependencies parsed in {:?} ({} deps)",
            start.elapsed(),
            dependencies.len()
        );

        let recent_git = Self::read_git_log(project_root);
        debug!(
            "QuickIndex: git log read in {:?} ({} entries)",
            start.elapsed(),
            recent_git.len()
        );

        let elapsed = start.elapsed();
        if elapsed.as_secs() > 3 {
            warn!(
                "QuickIndex: build took {:?} (target <3s) for {}",
                elapsed,
                project_root.display()
            );
        } else {
            debug!("QuickIndex: build completed in {:?}", elapsed);
        }

        Self {
            project_root: project_root.to_path_buf(),
            file_tree,
            key_symbols,
            dependencies,
            recent_git,
            indexed_at: start,
        }
    }

    /// Generate a concise context string suitable for injection into AI prompts.
    ///
    /// Targets <2000 tokens (~8000 chars). Contains project overview, key
    /// symbols, dependencies, and recent git activity.
    pub fn to_context_string(&self) -> String {
        let mut out = String::with_capacity(4096);

        // -- Project overview --
        out.push_str("# Project Overview\n\n");
        out.push_str(&self.file_tree.summary);
        out.push('\n');

        if !self.file_tree.key_dirs.is_empty() {
            out.push_str("Key directories: ");
            out.push_str(&self.file_tree.key_dirs.join(", "));
            out.push('\n');
        }

        // Top extensions
        let mut exts: Vec<_> = self.file_tree.by_extension.iter().collect();
        exts.sort_by(|a, b| b.1.cmp(a.1));
        if !exts.is_empty() {
            out.push_str("File types: ");
            let top: Vec<String> = exts
                .iter()
                .take(8)
                .map(|(ext, count)| format!(".{ext}({count})"))
                .collect();
            out.push_str(&top.join(", "));
            out.push('\n');
        }

        // -- Dependencies --
        if !self.dependencies.is_empty() {
            out.push_str("\n## Dependencies\n\n");
            // Group by source file
            let mut by_source: HashMap<&str, Vec<&Dependency>> = HashMap::new();
            for dep in &self.dependencies {
                by_source.entry(&dep.source).or_default().push(dep);
            }
            for (source, deps) in &by_source {
                out.push_str(&format!("From {source}: "));
                let names: Vec<String> = deps
                    .iter()
                    .take(30)
                    .map(|d| {
                        if d.version.is_empty() {
                            d.name.clone()
                        } else {
                            format!("{}@{}", d.name, d.version)
                        }
                    })
                    .collect();
                out.push_str(&names.join(", "));
                if deps.len() > 30 {
                    out.push_str(&format!(" ... and {} more", deps.len() - 30));
                }
                out.push('\n');
            }
        }

        // -- Key symbols --
        if !self.key_symbols.is_empty() {
            out.push_str("\n## Key Symbols\n\n");
            // Group by file for compactness
            let mut by_file: HashMap<&str, Vec<&SymbolEntry>> = HashMap::new();
            for sym in &self.key_symbols {
                by_file.entry(&sym.file).or_default().push(sym);
            }

            // Sort files by symbol count (most symbols first) and limit output
            let mut files: Vec<_> = by_file.into_iter().collect();
            files.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

            let mut symbol_lines = 0;
            for (file, syms) in &files {
                if symbol_lines > 60 {
                    out.push_str(&format!(
                        "... and {} more files with symbols\n",
                        files.len() - symbol_lines
                    ));
                    break;
                }
                out.push_str(&format!("  {file}: "));
                let names: Vec<String> = syms
                    .iter()
                    .take(10)
                    .map(|s| format!("{} {}", s.kind, s.name))
                    .collect();
                out.push_str(&names.join(", "));
                if syms.len() > 10 {
                    out.push_str(&format!(" +{}", syms.len() - 10));
                }
                out.push('\n');
                symbol_lines += 1;
            }
        }

        // -- Recent git history --
        if !self.recent_git.is_empty() {
            out.push_str("\n## Recent Git History\n\n");
            for entry in self.recent_git.iter().take(10) {
                let age = if entry.days_ago == 0 {
                    "today".to_string()
                } else if entry.days_ago == 1 {
                    "yesterday".to_string()
                } else {
                    format!("{}d ago", entry.days_ago)
                };
                out.push_str(&format!(
                    "  {} {} — {} ({})\n",
                    &entry.hash[..7.min(entry.hash.len())],
                    entry.message,
                    entry.author,
                    age
                ));
            }
        }

        out
    }

    /// Generate a context string using TOON encoding for ~30-40% token savings.
    ///
    /// Structured sections (file types, dependencies, symbols, git history) are
    /// encoded as TOON tables. Prose sections remain plain text.
    pub fn to_context_string_toon(&self) -> String {
        let mut out = String::with_capacity(4096);

        // -- Project overview (prose — no TOON benefit) --
        out.push_str("# Project Overview\n\n");
        out.push_str(&self.file_tree.summary);
        out.push('\n');

        if !self.file_tree.key_dirs.is_empty() {
            out.push_str("Key directories: ");
            out.push_str(&self.file_tree.key_dirs.join(", "));
            out.push('\n');
        }

        // File types — TOON table
        if !self.file_tree.by_extension.is_empty() {
            let encoded = crate::toon::encode_file_types(&self.file_tree.by_extension);
            if !encoded.is_empty() {
                out.push_str(&encoded);
                out.push('\n');
            }
        }

        // Dependencies — TOON table
        if !self.dependencies.is_empty() {
            out.push_str("\n## Dependencies\n\n");
            out.push_str(&crate::toon::encode_dependencies(&self.dependencies));
            out.push('\n');
        }

        // Key symbols — TOON table (biggest savings)
        if !self.key_symbols.is_empty() {
            out.push_str("\n## Key Symbols\n\n");
            out.push_str(&crate::toon::encode_symbols(&self.key_symbols));
            out.push('\n');
        }

        // Recent git history — TOON table
        if !self.recent_git.is_empty() {
            out.push_str("\n## Recent Git History\n\n");
            out.push_str(&crate::toon::encode_git_history(&self.recent_git));
            out.push('\n');
        }

        out
    }

    // -----------------------------------------------------------------------
    // File tree scanning
    // -----------------------------------------------------------------------

    fn scan_file_tree(root: &Path) -> FileTree {
        let mut total_files = 0usize;
        let mut by_extension: HashMap<String, usize> = HashMap::new();
        let mut key_dirs: Vec<String> = Vec::new();

        // Walk the directory tree non-recursively for the top-level dirs,
        // then recurse. This is fast enough for <3s on typical projects.
        Self::walk_for_tree(root, root, &mut total_files, &mut by_extension);

        // Identify key top-level directories
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !name.starts_with('.') && !SKIP_DIRS.contains(&name.as_str()) {
                        key_dirs.push(format!("{name}/"));
                    }
                }
            }
        }
        key_dirs.sort();

        // Build human-readable summary
        let summary = Self::build_tree_summary(root, total_files, &by_extension, &key_dirs);

        FileTree {
            total_files,
            by_extension,
            key_dirs,
            summary,
        }
    }

    fn walk_for_tree(
        root: &Path,
        dir: &Path,
        total_files: &mut usize,
        by_extension: &mut HashMap<String, usize>,
    ) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if path.is_dir() {
                if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
                    continue;
                }
                Self::walk_for_tree(root, &path, total_files, by_extension);
            } else if path.is_file() {
                *total_files += 1;
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    *by_extension.entry(ext.to_lowercase()).or_insert(0) += 1;
                }
            }
        }
    }

    fn build_tree_summary(
        root: &Path,
        total_files: usize,
        by_extension: &HashMap<String, usize>,
        key_dirs: &[String],
    ) -> String {
        let project_name = root
            .file_name()
            .unwrap_or(root.as_os_str())
            .to_string_lossy();

        // Detect project type
        let has_cargo = root.join("Cargo.toml").exists();
        let has_package_json = root.join("package.json").exists();
        let has_pyproject = root.join("pyproject.toml").exists();
        let has_requirements = root.join("requirements.txt").exists();
        let has_go_mod = root.join("go.mod").exists();

        let mut project_type = Vec::new();
        if has_cargo {
            // Check if workspace
            let is_workspace = key_dirs.iter().any(|d| d == "crates/");
            let rs_count = by_extension.get("rs").copied().unwrap_or(0);
            let toml_count = by_extension.get("toml").copied().unwrap_or(0);
            if is_workspace {
                project_type.push(format!(
                    "Rust workspace with {} .rs files, {} .toml files",
                    rs_count, toml_count
                ));
            } else {
                project_type.push(format!("Rust project with {} .rs files", rs_count));
            }
        }
        if has_package_json {
            let js_count = by_extension.get("js").copied().unwrap_or(0)
                + by_extension.get("jsx").copied().unwrap_or(0);
            let ts_count = by_extension.get("ts").copied().unwrap_or(0)
                + by_extension.get("tsx").copied().unwrap_or(0);
            if ts_count > 0 {
                project_type.push(format!("TypeScript project with {} .ts/.tsx files", ts_count));
            } else if js_count > 0 {
                project_type.push(format!("JavaScript project with {} .js/.jsx files", js_count));
            } else {
                project_type.push("Node.js project".to_string());
            }
        }
        if has_pyproject || has_requirements {
            let py_count = by_extension.get("py").copied().unwrap_or(0);
            project_type.push(format!("Python project with {} .py files", py_count));
        }
        if has_go_mod {
            let go_count = by_extension.get("go").copied().unwrap_or(0);
            project_type.push(format!("Go project with {} .go files", go_count));
        }

        if project_type.is_empty() {
            format!(
                "Project \"{project_name}\" with {total_files} files"
            )
        } else {
            format!(
                "\"{project_name}\": {} ({total_files} total files)",
                project_type.join(" + ")
            )
        }
    }

    // -----------------------------------------------------------------------
    // Symbol extraction (regex-based, speed over precision)
    // -----------------------------------------------------------------------

    fn extract_symbols(root: &Path) -> Vec<SymbolEntry> {
        // Collect candidate files, sorted by modification time (most recent first)
        let mut candidates: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
        Self::collect_symbol_candidates(root, root, &mut candidates);
        candidates.sort_by(|a, b| b.1.cmp(&a.1));
        candidates.truncate(MAX_SYMBOL_SCAN_FILES);

        let mut symbols = Vec::new();

        for (path, _) in &candidates {
            if symbols.len() >= MAX_SYMBOLS {
                break;
            }

            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            // Read file content
            let content = match fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let rel_path = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");

            let file_symbols = match ext.as_str() {
                "rs" => Self::extract_rust_symbols(&content, &rel_path),
                "py" => Self::extract_python_symbols(&content, &rel_path),
                "js" | "jsx" | "ts" | "tsx" => Self::extract_js_ts_symbols(&content, &rel_path),
                "go" => Self::extract_go_symbols(&content, &rel_path),
                "java" | "kt" => Self::extract_java_symbols(&content, &rel_path),
                "c" | "cpp" | "h" | "hpp" => Self::extract_c_cpp_symbols(&content, &rel_path),
                "cs" => Self::extract_csharp_symbols(&content, &rel_path),
                "rb" => Self::extract_ruby_symbols(&content, &rel_path),
                "swift" => Self::extract_swift_symbols(&content, &rel_path),
                _ => Vec::new(),
            };

            let remaining = MAX_SYMBOLS - symbols.len();
            symbols.extend(file_symbols.into_iter().take(remaining));
        }

        symbols
    }

    fn collect_symbol_candidates(
        root: &Path,
        dir: &Path,
        candidates: &mut Vec<(PathBuf, std::time::SystemTime)>,
    ) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if path.is_dir() {
                if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
                    continue;
                }
                Self::collect_symbol_candidates(root, &path, candidates);
            } else if path.is_file() {
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if !SYMBOL_EXTENSIONS.contains(&ext.as_str()) {
                    continue;
                }
                // Skip large files
                if let Ok(meta) = entry.metadata() {
                    if meta.len() > MAX_FILE_SIZE_FOR_SYMBOLS {
                        continue;
                    }
                    let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                    candidates.push((path, mtime));
                }
            }
        }
    }

    fn extract_rust_symbols(content: &str, file: &str) -> Vec<SymbolEntry> {
        let mut symbols = Vec::new();
        // Match public and private functions, structs, traits, enums, modules, consts
        let re = Regex::new(
            r"(?m)^[[:space:]]*(pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(fn|struct|trait|enum|mod|const)\s+([A-Za-z_][A-Za-z0-9_]*)"
        ).unwrap();

        for cap in re.captures_iter(content) {
            let is_pub = cap.get(1).is_some();
            let kind_str = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let name = cap.get(3).map(|m| m.as_str()).unwrap_or("");

            // For fast indexing, prefer pub symbols but include private too
            let kind = match kind_str {
                "fn" => SymbolKind::Function,
                "struct" => SymbolKind::Struct,
                "trait" => SymbolKind::Trait,
                "enum" => SymbolKind::Enum,
                "mod" => SymbolKind::Module,
                "const" => SymbolKind::Const,
                _ => continue,
            };

            // Prioritize public symbols; include private if name is meaningful
            if is_pub || name.len() > 2 {
                symbols.push(SymbolEntry {
                    name: name.to_string(),
                    kind,
                    file: file.to_string(),
                });
            }
        }
        symbols
    }

    fn extract_python_symbols(content: &str, file: &str) -> Vec<SymbolEntry> {
        let mut symbols = Vec::new();
        let re = Regex::new(r"(?m)^(?:async\s+)?(?:def|class)\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap();

        for cap in re.captures_iter(content) {
            let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            // Detect kind from the matched keyword
            let line = cap.get(0).map(|m| m.as_str()).unwrap_or("");
            let kind = if line.contains("class ") {
                SymbolKind::Class
            } else {
                SymbolKind::Function
            };

            if !name.starts_with('_') || name.starts_with("__") {
                symbols.push(SymbolEntry {
                    name: name.to_string(),
                    kind,
                    file: file.to_string(),
                });
            }
        }
        symbols
    }

    fn extract_js_ts_symbols(content: &str, file: &str) -> Vec<SymbolEntry> {
        let mut symbols = Vec::new();

        // Export declarations
        let re_export = Regex::new(
            r"(?m)^[[:space:]]*export\s+(?:default\s+)?(?:async\s+)?(function|class|interface|const|enum)\s+([A-Za-z_$][A-Za-z0-9_$]*)"
        ).unwrap();
        for cap in re_export.captures_iter(content) {
            let kind_str = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let name = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let kind = match kind_str {
                "function" => SymbolKind::Function,
                "class" => SymbolKind::Class,
                "interface" => SymbolKind::Interface,
                "const" => SymbolKind::Const,
                "enum" => SymbolKind::Enum,
                _ => continue,
            };
            symbols.push(SymbolEntry {
                name: name.to_string(),
                kind,
                file: file.to_string(),
            });
        }

        // Non-exported top-level function/class declarations
        let re_decl = Regex::new(
            r"(?m)^(?:async\s+)?(?:function|class)\s+([A-Za-z_$][A-Za-z0-9_$]*)"
        ).unwrap();
        for cap in re_decl.captures_iter(content) {
            let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let line = cap.get(0).map(|m| m.as_str()).unwrap_or("");
            let kind = if line.contains("class ") {
                SymbolKind::Class
            } else {
                SymbolKind::Function
            };
            // Avoid duplicates with exported symbols
            if !symbols.iter().any(|s| s.name == name) {
                symbols.push(SymbolEntry {
                    name: name.to_string(),
                    kind,
                    file: file.to_string(),
                });
            }
        }
        symbols
    }

    fn extract_go_symbols(content: &str, file: &str) -> Vec<SymbolEntry> {
        let mut symbols = Vec::new();
        let re = Regex::new(
            r"(?m)^(?:func|type)\s+(?:\([^)]*\)\s+)?([A-Za-z_][A-Za-z0-9_]*)"
        ).unwrap();
        for cap in re.captures_iter(content) {
            let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let line = cap.get(0).map(|m| m.as_str()).unwrap_or("");
            let kind = if line.starts_with("type") {
                SymbolKind::Struct // Go types map to Struct
            } else {
                SymbolKind::Function
            };
            // In Go, exported symbols are capitalized
            symbols.push(SymbolEntry {
                name: name.to_string(),
                kind,
                file: file.to_string(),
            });
        }
        symbols
    }

    fn extract_java_symbols(content: &str, file: &str) -> Vec<SymbolEntry> {
        let mut symbols = Vec::new();
        let re = Regex::new(
            r"(?m)(?:public|private|protected)?\s*(?:static\s+)?(?:abstract\s+)?(?:final\s+)?(class|interface|enum)\s+([A-Za-z_][A-Za-z0-9_]*)"
        ).unwrap();
        for cap in re.captures_iter(content) {
            let kind_str = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let name = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let kind = match kind_str {
                "class" => SymbolKind::Class,
                "interface" => SymbolKind::Interface,
                "enum" => SymbolKind::Enum,
                _ => continue,
            };
            symbols.push(SymbolEntry {
                name: name.to_string(),
                kind,
                file: file.to_string(),
            });
        }
        symbols
    }

    fn extract_c_cpp_symbols(content: &str, file: &str) -> Vec<SymbolEntry> {
        let mut symbols = Vec::new();
        // Struct/class/enum declarations
        let re = Regex::new(
            r"(?m)^[[:space:]]*(struct|class|enum)\s+([A-Za-z_][A-Za-z0-9_]*)"
        ).unwrap();
        for cap in re.captures_iter(content) {
            let kind_str = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let name = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let kind = match kind_str {
                "struct" => SymbolKind::Struct,
                "class" => SymbolKind::Class,
                "enum" => SymbolKind::Enum,
                _ => continue,
            };
            symbols.push(SymbolEntry {
                name: name.to_string(),
                kind,
                file: file.to_string(),
            });
        }
        symbols
    }

    fn extract_csharp_symbols(content: &str, file: &str) -> Vec<SymbolEntry> {
        let mut symbols = Vec::new();
        let re = Regex::new(
            r"(?m)(?:public|private|protected|internal)?\s*(?:static\s+)?(?:abstract\s+)?(?:sealed\s+)?(class|interface|struct|enum)\s+([A-Za-z_][A-Za-z0-9_]*)"
        ).unwrap();
        for cap in re.captures_iter(content) {
            let kind_str = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let name = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let kind = match kind_str {
                "class" => SymbolKind::Class,
                "interface" => SymbolKind::Interface,
                "struct" => SymbolKind::Struct,
                "enum" => SymbolKind::Enum,
                _ => continue,
            };
            symbols.push(SymbolEntry {
                name: name.to_string(),
                kind,
                file: file.to_string(),
            });
        }
        symbols
    }

    fn extract_ruby_symbols(content: &str, file: &str) -> Vec<SymbolEntry> {
        let mut symbols = Vec::new();
        let re = Regex::new(r"(?m)^[[:space:]]*(class|module|def)\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap();
        for cap in re.captures_iter(content) {
            let kind_str = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let name = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let kind = match kind_str {
                "class" => SymbolKind::Class,
                "module" => SymbolKind::Module,
                "def" => SymbolKind::Function,
                _ => continue,
            };
            symbols.push(SymbolEntry {
                name: name.to_string(),
                kind,
                file: file.to_string(),
            });
        }
        symbols
    }

    fn extract_swift_symbols(content: &str, file: &str) -> Vec<SymbolEntry> {
        let mut symbols = Vec::new();
        let re = Regex::new(
            r"(?m)^[[:space:]]*(class|struct|enum|protocol|func)\s+([A-Za-z_][A-Za-z0-9_]*)"
        ).unwrap();
        for cap in re.captures_iter(content) {
            let kind_str = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let name = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let kind = match kind_str {
                "class" => SymbolKind::Class,
                "struct" => SymbolKind::Struct,
                "enum" => SymbolKind::Enum,
                "protocol" => SymbolKind::Interface,
                "func" => SymbolKind::Function,
                _ => continue,
            };
            symbols.push(SymbolEntry {
                name: name.to_string(),
                kind,
                file: file.to_string(),
            });
        }
        symbols
    }

    // -----------------------------------------------------------------------
    // Dependency parsing
    // -----------------------------------------------------------------------

    fn parse_dependencies(root: &Path) -> Vec<Dependency> {
        let mut deps = Vec::new();

        // Rust: Cargo.toml files
        Self::find_and_parse_cargo_tomls(root, &mut deps);

        // Node: package.json
        let package_json = root.join("package.json");
        if package_json.exists() {
            Self::parse_package_json(&package_json, &mut deps);
        }

        // Python: requirements.txt
        let requirements = root.join("requirements.txt");
        if requirements.exists() {
            Self::parse_requirements_txt(&requirements, &mut deps);
        }

        // Python: pyproject.toml
        let pyproject = root.join("pyproject.toml");
        if pyproject.exists() {
            Self::parse_pyproject_toml(&pyproject, &mut deps);
        }

        deps
    }

    fn find_and_parse_cargo_tomls(root: &Path, deps: &mut Vec<Dependency>) {
        // Root Cargo.toml
        let root_cargo = root.join("Cargo.toml");
        if root_cargo.exists() {
            Self::parse_cargo_toml(&root_cargo, deps);
        }
    }

    fn parse_cargo_toml(path: &Path, deps: &mut Vec<Dependency>) {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let source = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .to_string();

        // Parse [dependencies], [dev-dependencies], [build-dependencies]
        // Using regex for speed (no toml crate needed for this lightweight scan)
        let sections = [
            "[dependencies]",
            "[dev-dependencies]",
            "[build-dependencies]",
        ];

        for section in &sections {
            if let Some(start) = content.find(section) {
                let after = &content[start + section.len()..];
                // Read until next section header or end
                let end = after
                    .find("\n[")
                    .unwrap_or(after.len());
                let block = &after[..end];

                // Match `name = "version"` or `name = { version = "x" }` or `name.workspace = true`
                let re_simple = Regex::new(
                    r#"(?m)^[[:space:]]*([A-Za-z0-9_-]+)\s*=\s*"([^"]*)""#
                ).unwrap();
                let re_table = Regex::new(
                    r#"(?m)^[[:space:]]*([A-Za-z0-9_-]+)\s*=\s*\{[^}]*version\s*=\s*"([^"]*)"[^}]*\}"#
                ).unwrap();
                let re_workspace = Regex::new(
                    r"(?m)^[[:space:]]*([A-Za-z0-9_-]+)\s*(?:=\s*\{[^}]*workspace\s*=\s*true|\.workspace\s*=\s*true)"
                ).unwrap();

                for cap in re_simple.captures_iter(block) {
                    let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                    let version = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                    deps.push(Dependency {
                        name: name.to_string(),
                        version: version.to_string(),
                        source: source.clone(),
                    });
                }
                for cap in re_table.captures_iter(block) {
                    let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                    let version = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                    // Avoid duplicates from simple regex
                    if !deps.iter().any(|d| d.name == name && d.source == source) {
                        deps.push(Dependency {
                            name: name.to_string(),
                            version: version.to_string(),
                            source: source.clone(),
                        });
                    }
                }
                for cap in re_workspace.captures_iter(block) {
                    let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                    if !deps.iter().any(|d| d.name == name && d.source == source) {
                        deps.push(Dependency {
                            name: name.to_string(),
                            version: "workspace".to_string(),
                            source: source.clone(),
                        });
                    }
                }
            }
        }
    }

    fn parse_package_json(path: &Path, deps: &mut Vec<Dependency>) {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let parsed: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return,
        };

        let source = "package.json".to_string();

        for key in &["dependencies", "devDependencies"] {
            if let Some(obj) = parsed.get(key).and_then(|v| v.as_object()) {
                for (name, version) in obj {
                    deps.push(Dependency {
                        name: name.clone(),
                        version: version.as_str().unwrap_or("").to_string(),
                        source: source.clone(),
                    });
                }
            }
        }
    }

    fn parse_requirements_txt(path: &Path, deps: &mut Vec<Dependency>) {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let source = "requirements.txt".to_string();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('-') {
                continue;
            }
            // Split on ==, >=, <=, ~=, !=, etc.
            let re = Regex::new(r"^([A-Za-z0-9_.-]+)(?:\[.*\])?\s*(?:([><=!~]+)\s*(.+))?").unwrap();
            if let Some(cap) = re.captures(line) {
                let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                let version = cap.get(3).map(|m| m.as_str()).unwrap_or("");
                deps.push(Dependency {
                    name: name.to_string(),
                    version: version.to_string(),
                    source: source.clone(),
                });
            }
        }
    }

    fn parse_pyproject_toml(path: &Path, deps: &mut Vec<Dependency>) {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let source = "pyproject.toml".to_string();

        // Look for dependencies = [...] in [project] section
        // Simple regex extraction for speed
        let re = Regex::new(r#"(?m)^\s*"([A-Za-z0-9_.-]+)(?:\[.*\])?(?:[><=!~]+[^"]*)?""#).unwrap();

        // Find the dependencies array
        if let Some(start) = content.find("dependencies") {
            let after = &content[start..];
            if let Some(bracket_start) = after.find('[') {
                let after_bracket = &after[bracket_start..];
                if let Some(bracket_end) = after_bracket.find(']') {
                    let array_content = &after_bracket[..bracket_end + 1];
                    for cap in re.captures_iter(array_content) {
                        let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                        deps.push(Dependency {
                            name: name.to_string(),
                            version: String::new(),
                            source: source.clone(),
                        });
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Git log (via git2)
    // -----------------------------------------------------------------------

    fn read_git_log(root: &Path) -> Vec<GitEntry> {
        let repo = match git2::Repository::discover(root) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        let mut revwalk = match repo.revwalk() {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        if revwalk.push_head().is_err() {
            return Vec::new();
        }
        let _ = revwalk.set_sorting(git2::Sort::TIME);

        let now = chrono::Utc::now().timestamp();
        let mut entries = Vec::new();

        for oid_result in revwalk {
            if entries.len() >= 20 {
                break;
            }
            let oid = match oid_result {
                Ok(o) => o,
                Err(_) => continue,
            };
            let commit = match repo.find_commit(oid) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let timestamp = commit.time().seconds();
            let days_ago = ((now - timestamp).max(0) / 86400) as u32;

            entries.push(GitEntry {
                hash: oid.to_string(),
                message: commit.message().unwrap_or("").lines().next().unwrap_or("").to_string(),
                author: commit.author().name().unwrap_or("unknown").to_string(),
                days_ago,
            });
        }

        entries
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a temp directory with a basic Rust project structure.
    fn setup_rust_project() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Cargo.toml
        fs::write(
            root.join("Cargo.toml"),
            r#"[package]
name = "test_project"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1.0"
tokio = { version = "1", features = ["full"] }
anyhow.workspace = true

[dev-dependencies]
tempfile = "3"
"#,
        )
        .unwrap();

        // src directory
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/main.rs"),
            r#"pub fn main() {
    println!("Hello, world!");
}

pub struct AppConfig {
    pub name: String,
    pub port: u16,
}

pub trait Service {
    fn start(&self);
}

pub enum Status {
    Active,
    Inactive,
}

mod utils;
"#,
        )
        .unwrap();

        fs::write(
            root.join("src/utils.rs"),
            r#"pub fn format_name(name: &str) -> String {
    name.to_uppercase()
}

pub const MAX_RETRIES: u32 = 3;
"#,
        )
        .unwrap();

        // tests directory
        fs::create_dir_all(root.join("tests")).unwrap();
        fs::write(
            root.join("tests/integration.rs"),
            r#"#[test]
fn test_basic() {
    assert!(true);
}
"#,
        )
        .unwrap();

        dir
    }

    fn setup_node_project() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::write(
            root.join("package.json"),
            r#"{
  "name": "test-app",
  "version": "1.0.0",
  "dependencies": {
    "express": "^4.18.0",
    "lodash": "^4.17.21"
  },
  "devDependencies": {
    "jest": "^29.0.0"
  }
}"#,
        )
        .unwrap();

        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/index.ts"),
            r#"export function main() {
    console.log("Hello");
}

export class AppServer {
    start() {}
}

export interface Config {
    port: number;
}

export const VERSION = "1.0.0";
"#,
        )
        .unwrap();

        dir
    }

    fn setup_python_project() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::write(
            root.join("requirements.txt"),
            "flask==2.3.0\nrequests>=2.28\n# comment\nnumpy\n",
        )
        .unwrap();

        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/app.py"),
            r#"class MyApp:
    def __init__(self):
        pass

def run_server():
    pass

async def fetch_data():
    pass
"#,
        )
        .unwrap();

        dir
    }

    // -- QuickIndex::build ---------------------------------------------------

    #[test]
    fn test_build_rust_project() {
        let dir = setup_rust_project();
        let index = QuickIndex::build(dir.path());

        assert!(index.file_tree.total_files > 0);
        assert!(index.file_tree.by_extension.contains_key("rs"));
        assert!(index.file_tree.by_extension.contains_key("toml"));
        assert!(!index.key_symbols.is_empty());
        assert!(!index.dependencies.is_empty());
    }

    #[test]
    fn test_build_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let index = QuickIndex::build(dir.path());

        assert_eq!(index.file_tree.total_files, 0);
        assert!(index.key_symbols.is_empty());
        assert!(index.dependencies.is_empty());
        assert!(index.recent_git.is_empty());
    }

    // -- File tree scanning --------------------------------------------------

    #[test]
    fn test_file_tree_counts() {
        let dir = setup_rust_project();
        let tree = QuickIndex::scan_file_tree(dir.path());

        // Should have Cargo.toml, src/main.rs, src/utils.rs, tests/integration.rs
        assert_eq!(tree.total_files, 4);
        assert_eq!(tree.by_extension.get("rs"), Some(&3));
        assert_eq!(tree.by_extension.get("toml"), Some(&1));
    }

    #[test]
    fn test_file_tree_skips_hidden() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".git/config"), "data").unwrap();
        fs::write(dir.path().join("visible.txt"), "data").unwrap();

        let tree = QuickIndex::scan_file_tree(dir.path());
        assert_eq!(tree.total_files, 1);
    }

    #[test]
    fn test_file_tree_key_dirs() {
        let dir = setup_rust_project();
        let tree = QuickIndex::scan_file_tree(dir.path());

        assert!(tree.key_dirs.contains(&"src/".to_string()));
        assert!(tree.key_dirs.contains(&"tests/".to_string()));
    }

    #[test]
    fn test_file_tree_summary_rust() {
        let dir = setup_rust_project();
        let tree = QuickIndex::scan_file_tree(dir.path());

        assert!(tree.summary.contains("Rust"));
        assert!(tree.summary.contains(".rs"));
    }

    // -- Symbol extraction ---------------------------------------------------

    #[test]
    fn test_rust_symbols() {
        let symbols = QuickIndex::extract_rust_symbols(
            r#"
pub fn main() {}
pub struct Config { name: String }
pub trait Service { fn start(&self); }
pub enum Status { Active, Inactive }
mod utils;
pub const MAX: u32 = 10;
fn private_helper() {}
"#,
            "test.rs",
        );

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"Config"));
        assert!(names.contains(&"Service"));
        assert!(names.contains(&"Status"));
        assert!(names.contains(&"utils"));
        assert!(names.contains(&"MAX"));
        assert!(names.contains(&"private_helper"));
    }

    #[test]
    fn test_python_symbols() {
        let symbols = QuickIndex::extract_python_symbols(
            r#"
class MyApp:
    pass

def run_server():
    pass

async def fetch_data():
    pass
"#,
            "app.py",
        );

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"MyApp"));
        assert!(names.contains(&"run_server"));
        assert!(names.contains(&"fetch_data"));
    }

    #[test]
    fn test_js_ts_symbols() {
        let symbols = QuickIndex::extract_js_ts_symbols(
            r#"
export function main() {}
export class AppServer {}
export interface Config {}
export const VERSION = "1.0";
function helper() {}
"#,
            "index.ts",
        );

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"AppServer"));
        assert!(names.contains(&"Config"));
        assert!(names.contains(&"VERSION"));
        assert!(names.contains(&"helper"));
    }

    // -- Dependency parsing --------------------------------------------------

    #[test]
    fn test_parse_cargo_toml() {
        let dir = setup_rust_project();
        let mut deps = Vec::new();
        QuickIndex::parse_cargo_toml(&dir.path().join("Cargo.toml"), &mut deps);

        let names: Vec<&str> = deps.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"serde"));
        assert!(names.contains(&"tokio"));
        assert!(names.contains(&"anyhow"));
        assert!(names.contains(&"tempfile"));

        // Check version parsing
        let serde = deps.iter().find(|d| d.name == "serde").unwrap();
        assert_eq!(serde.version, "1.0");

        let tokio = deps.iter().find(|d| d.name == "tokio").unwrap();
        assert_eq!(tokio.version, "1");

        let anyhow = deps.iter().find(|d| d.name == "anyhow").unwrap();
        assert_eq!(anyhow.version, "workspace");
    }

    #[test]
    fn test_parse_package_json() {
        let dir = setup_node_project();
        let mut deps = Vec::new();
        QuickIndex::parse_package_json(&dir.path().join("package.json"), &mut deps);

        let names: Vec<&str> = deps.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"express"));
        assert!(names.contains(&"lodash"));
        assert!(names.contains(&"jest"));
    }

    #[test]
    fn test_parse_requirements_txt() {
        let dir = setup_python_project();
        let mut deps = Vec::new();
        QuickIndex::parse_requirements_txt(&dir.path().join("requirements.txt"), &mut deps);

        let names: Vec<&str> = deps.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"flask"));
        assert!(names.contains(&"requests"));
        assert!(names.contains(&"numpy"));

        let flask = deps.iter().find(|d| d.name == "flask").unwrap();
        assert_eq!(flask.version, "2.3.0");
    }

    // -- Context string generation -------------------------------------------

    #[test]
    fn test_context_string_not_empty() {
        let dir = setup_rust_project();
        let index = QuickIndex::build(dir.path());
        let ctx = index.to_context_string();

        assert!(!ctx.is_empty());
        assert!(ctx.contains("Project Overview"));
        assert!(ctx.contains("Rust"));
    }

    #[test]
    fn test_context_string_contains_deps() {
        let dir = setup_rust_project();
        let index = QuickIndex::build(dir.path());
        let ctx = index.to_context_string();

        assert!(ctx.contains("Dependencies"));
        assert!(ctx.contains("serde"));
    }

    #[test]
    fn test_context_string_contains_symbols() {
        let dir = setup_rust_project();
        let index = QuickIndex::build(dir.path());
        let ctx = index.to_context_string();

        assert!(ctx.contains("Key Symbols"));
    }

    #[test]
    fn test_context_string_size_reasonable() {
        let dir = setup_rust_project();
        let index = QuickIndex::build(dir.path());
        let ctx = index.to_context_string();

        // Should be well under 8000 chars (~2000 tokens)
        assert!(ctx.len() < 8000, "Context string too large: {} chars", ctx.len());
    }

    // -- Git log (requires git init) -----------------------------------------

    #[test]
    fn test_git_log_graceful_no_repo() {
        let dir = tempfile::tempdir().unwrap();
        let entries = QuickIndex::read_git_log(dir.path());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_git_log_with_repo() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();

        // Configure and create a commit
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@test.com").unwrap();

        fs::write(dir.path().join("file.txt"), "content").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("file.txt")).unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .unwrap();

        let entries = QuickIndex::read_git_log(dir.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].message, "Initial commit");
        assert_eq!(entries[0].author, "Test");
        assert_eq!(entries[0].days_ago, 0);
    }

    // -- Full integration test -----------------------------------------------

    #[test]
    fn test_full_index_node_project() {
        let dir = setup_node_project();
        let index = QuickIndex::build(dir.path());

        assert!(index.file_tree.total_files > 0);
        assert!(!index.dependencies.is_empty());
        assert!(!index.key_symbols.is_empty());
        assert!(index.to_context_string().contains("Dependencies"));
    }

    #[test]
    fn test_full_index_python_project() {
        let dir = setup_python_project();
        let index = QuickIndex::build(dir.path());

        assert!(index.file_tree.total_files > 0);
        assert!(!index.dependencies.is_empty());
        assert!(!index.key_symbols.is_empty());
    }

    // -- SymbolKind display --------------------------------------------------

    #[test]
    fn test_symbol_kind_display() {
        assert_eq!(format!("{}", SymbolKind::Function), "fn");
        assert_eq!(format!("{}", SymbolKind::Struct), "struct");
        assert_eq!(format!("{}", SymbolKind::Trait), "trait");
        assert_eq!(format!("{}", SymbolKind::Enum), "enum");
        assert_eq!(format!("{}", SymbolKind::Module), "mod");
        assert_eq!(format!("{}", SymbolKind::Class), "class");
        assert_eq!(format!("{}", SymbolKind::Interface), "interface");
        assert_eq!(format!("{}", SymbolKind::Const), "const");
    }
}
