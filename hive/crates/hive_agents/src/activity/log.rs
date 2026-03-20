use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use super::ActivityEvent;

/// A persisted activity event entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub id: i64,
    pub timestamp: String,
    pub event_type: String,
    pub agent_id: Option<String>,
    pub task_id: Option<String>,
    pub category: String,
    pub summary: String,
    pub detail_json: Option<String>,
    pub cost_usd: f64,
}

/// Filter for querying activity events.
#[derive(Debug, Clone, Default)]
pub struct ActivityFilter {
    pub categories: Option<Vec<String>>,
    pub agent_id: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub search: Option<String>,
    pub limit: usize,
    pub offset: usize,
}

/// Cost summary across agents and models.
#[derive(Debug, Clone, Default)]
pub struct CostSummary {
    pub total_usd: f64,
    pub by_agent: Vec<(String, f64)>,
    pub by_model: Vec<(String, f64)>,
    pub request_count: usize,
}

/// SQLite-backed activity log.
pub struct ActivityLog {
    conn: Mutex<Connection>,
}

impl ActivityLog {
    /// Open the activity log at the given path.
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let log = Self {
            conn: Mutex::new(conn),
        };
        log.init_schema()?;
        Ok(log)
    }

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let log = Self {
            conn: Mutex::new(conn),
        };
        log.init_schema()?;
        Ok(log)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS activity_events (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp     TEXT    NOT NULL,
                event_type    TEXT    NOT NULL,
                agent_id      TEXT,
                task_id       TEXT,
                category      TEXT    NOT NULL,
                summary       TEXT    NOT NULL,
                detail_json   TEXT,
                cost_usd      REAL    DEFAULT 0.0
            );
            CREATE INDEX IF NOT EXISTS idx_events_category  ON activity_events(category);
            CREATE INDEX IF NOT EXISTS idx_events_agent     ON activity_events(agent_id);
            CREATE INDEX IF NOT EXISTS idx_events_timestamp ON activity_events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_events_type      ON activity_events(event_type);",
        )?;
        Ok(())
    }

    /// Record an activity event.
    pub fn record(&self, event: &ActivityEvent) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let detail = serde_json::to_string(event).ok();
        conn.execute(
            "INSERT INTO activity_events (timestamp, event_type, agent_id, task_id, category, summary, detail_json, cost_usd)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                Utc::now().to_rfc3339(),
                event.event_type(),
                event.agent_id(),
                extract_task_id(event),
                event.category(),
                event.summary(),
                detail,
                event.cost_usd(),
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Query events with filters.
    pub fn query(&self, filter: &ActivityFilter) -> Result<Vec<ActivityEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut sql = String::from(
            "SELECT id, timestamp, event_type, agent_id, task_id, category, summary, detail_json, cost_usd \
             FROM activity_events WHERE 1=1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(ref cats) = filter.categories {
            if !cats.is_empty() {
                let placeholders: Vec<String> = cats
                    .iter()
                    .enumerate()
                    .map(|(i, _)| format!("?{}", param_values.len() + i + 1))
                    .collect();
                sql.push_str(&format!(" AND category IN ({})", placeholders.join(",")));
                for c in cats {
                    param_values.push(Box::new(c.clone()));
                }
            }
        }

        if let Some(ref agent) = filter.agent_id {
            param_values.push(Box::new(agent.clone()));
            sql.push_str(&format!(" AND agent_id = ?{}", param_values.len()));
        }

        if let Some(ref since) = filter.since {
            param_values.push(Box::new(since.to_rfc3339()));
            sql.push_str(&format!(" AND timestamp >= ?{}", param_values.len()));
        }

        if let Some(ref search) = filter.search {
            param_values.push(Box::new(format!("%{search}%")));
            sql.push_str(&format!(" AND summary LIKE ?{}", param_values.len()));
        }

        sql.push_str(" ORDER BY id DESC");

        let limit = if filter.limit == 0 { 100 } else { filter.limit };
        param_values.push(Box::new(limit as i64));
        sql.push_str(&format!(" LIMIT ?{}", param_values.len()));

        if filter.offset > 0 {
            param_values.push(Box::new(filter.offset as i64));
            sql.push_str(&format!(" OFFSET ?{}", param_values.len()));
        }

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok(ActivityEntry {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                event_type: row.get(2)?,
                agent_id: row.get(3)?,
                task_id: row.get(4)?,
                category: row.get(5)?,
                summary: row.get(6)?,
                detail_json: row.get(7)?,
                cost_usd: row.get(8)?,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    /// Summarize costs since a given timestamp.
    pub fn cost_summary(
        &self,
        agent_id: Option<&str>,
        since: DateTime<Utc>,
    ) -> Result<CostSummary> {
        let conn = self.conn.lock().unwrap();
        let since_str = since.to_rfc3339();

        // Total and count
        let (total, count): (f64, usize) = if let Some(aid) = agent_id {
            conn.query_row(
                "SELECT COALESCE(SUM(cost_usd), 0), COUNT(*) FROM activity_events \
                 WHERE event_type = 'cost_incurred' AND timestamp >= ?1 AND agent_id = ?2",
                params![since_str, aid],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?
        } else {
            conn.query_row(
                "SELECT COALESCE(SUM(cost_usd), 0), COUNT(*) FROM activity_events \
                 WHERE event_type = 'cost_incurred' AND timestamp >= ?1",
                params![since_str],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?
        };

        // By agent
        let mut stmt = conn.prepare(
            "SELECT agent_id, SUM(cost_usd) FROM activity_events \
             WHERE event_type = 'cost_incurred' AND timestamp >= ?1 GROUP BY agent_id",
        )?;
        let by_agent: Vec<(String, f64)> = stmt
            .query_map(params![since_str], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        // By model — parse from detail_json
        let mut stmt = conn.prepare(
            "SELECT detail_json, cost_usd FROM activity_events \
             WHERE event_type = 'cost_incurred' AND timestamp >= ?1",
        )?;
        let mut model_map = std::collections::HashMap::new();
        let mut rows = stmt.query(params![since_str])?;
        while let Some(row) = rows.next()? {
            let json: Option<String> = row.get(0)?;
            let cost: f64 = row.get(1)?;
            if let Some(json) = json {
                if let Ok(event) = serde_json::from_str::<ActivityEvent>(&json) {
                    if let ActivityEvent::CostIncurred { model, .. } = event {
                        *model_map.entry(model).or_insert(0.0) += cost;
                    }
                }
            }
        }
        let by_model: Vec<(String, f64)> = model_map.into_iter().collect();

        Ok(CostSummary {
            total_usd: total,
            by_agent,
            by_model,
            request_count: count,
        })
    }

    /// Total number of events.
    pub fn total_events(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM activity_events", [], |row| row.get(0))?;
        Ok(count as usize)
    }
}

/// Extract task_id from events that have one.
fn extract_task_id(event: &ActivityEvent) -> Option<&str> {
    match event {
        ActivityEvent::AgentStarted { task_id, .. } => task_id.as_deref(),
        ActivityEvent::TaskClaimed { task_id, .. }
        | ActivityEvent::TaskProgress { task_id, .. }
        | ActivityEvent::TaskCompleted { task_id, .. }
        | ActivityEvent::TaskFailed { task_id, .. }
        | ActivityEvent::HeartbeatFired { task_id, .. } => Some(task_id),
        _ => None,
    }
}
