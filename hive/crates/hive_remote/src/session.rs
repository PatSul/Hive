use crate::protocol::DaemonEvent;
use anyhow::{Context, Result};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Append-only event journal for session persistence.
/// Each line is a JSON-serialized DaemonEvent.
/// On recovery, replay the journal to reconstruct state.
pub struct SessionJournal {
    path: PathBuf,
    writer: File,
}

impl SessionJournal {
    pub fn new(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create journal dir: {}", parent.display()))?;
        }
        let writer = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("Failed to open journal: {}", path.display()))?;
        Ok(Self {
            path: path.to_path_buf(),
            writer,
        })
    }

    pub fn append(&mut self, event: &DaemonEvent) -> Result<()> {
        let json = serde_json::to_string(event)?;
        writeln!(self.writer, "{}", json)?;
        self.writer.flush()?;
        Ok(())
    }

    pub fn replay(path: &Path) -> Result<Vec<DaemonEvent>> {
        if !path.exists() {
            return Ok(vec![]);
        }
        let file = File::open(path)
            .with_context(|| format!("Failed to open journal for replay: {}", path.display()))?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<DaemonEvent>(trimmed) {
                Ok(event) => events.push(event),
                Err(e) => {
                    tracing::warn!("Skipping corrupt journal line: {}", e);
                    continue;
                }
            }
        }
        Ok(events)
    }

    pub fn truncate(&mut self) -> Result<()> {
        drop(std::mem::replace(
            &mut self.writer,
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&self.path)?,
        ));
        self.writer = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
