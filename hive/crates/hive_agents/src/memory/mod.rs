//! Tiered Memory Architecture
//!
//! 5-layer memory system for agent context management:
//! - L1 HOT: In-memory session state with WAL persistence
//! - L2 WARM: Semantic vector search (bridges to hive_ai::memory::HiveMemory)
//! - L3 COLD: SQLite keyword search (CollectiveMemory, enhanced)
//! - L4 ARCHIVE: Daily markdown logs for historical reference
//! - L5 CLOUD: Cross-device sync (deferred)

pub mod archive;
pub mod bootstrap;
pub mod session_state;
pub mod tiered_memory;
pub mod vector_bridge;

pub use archive::{ArchiveEntry, ArchiveService};
pub use bootstrap::{BootstrapContext, BootstrapGenerator};
pub use session_state::{Decision, EntityInfo, PendingWrite, SessionState};
pub use tiered_memory::{MemoryQuery, MemoryQueryResult, TieredEntry, TieredMemory};
pub use vector_bridge::{MockVectorBridge, VectorMemoryBridge, VectorResult, VectorSource};

use serde::{Deserialize, Serialize};

/// Which memory layer to target for writes or queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetLayer {
    /// In-memory session state (fastest, ephemeral)
    Hot,
    /// Vector/semantic search via LanceDB
    Warm,
    /// SQLite keyword search (CollectiveMemory)
    Cold,
    /// Markdown daily logs
    Archive,
}

impl std::fmt::Display for TargetLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hot => write!(f, "hot"),
            Self::Warm => write!(f, "warm"),
            Self::Cold => write!(f, "cold"),
            Self::Archive => write!(f, "archive"),
        }
    }
}
