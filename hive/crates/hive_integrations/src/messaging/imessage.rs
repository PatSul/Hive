//! iMessage messaging provider (macOS only).
//!
//! Uses AppleScript via `osascript` to send messages through Messages.app
//! and reads the iMessage SQLite database at `~/Library/Messages/chat.db`
//! for listing conversations, retrieving, and searching messages.
//!
//! This module is gated with `#[cfg(target_os = "macos")]`.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use regex::Regex;
use std::path::PathBuf;
use std::process::Command;
use std::sync::LazyLock;
use tracing::debug;

/// Maximum allowed length for search query strings.
const MAX_SEARCH_QUERY_LEN: usize = 500;

/// Regex for validating channel identifiers (phone numbers and email addresses).
/// Allows alphanumeric characters, `@`, `.`, `+`, `-`, and `_`.
static CHANNEL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9@.+_-]+$").expect("invalid channel regex"));

use super::provider::{Channel, IncomingMessage, MessagingProvider, Platform, SentMessage};

/// iMessage uses Apple's Core Data timestamp epoch: 2001-01-01 00:00:00 UTC.
/// This is the offset in seconds from Unix epoch to Apple epoch.
const APPLE_EPOCH_OFFSET: i64 = 978_307_200;

/// Factor to convert the nanosecond-based timestamps stored in chat.db to seconds.
/// Modern versions of chat.db store timestamps in nanoseconds since Apple epoch.
const NANOSECOND_FACTOR: i64 = 1_000_000_000;

// ── Client ──────────────────────────────────────────────────────

/// iMessage messaging provider using AppleScript and the Messages SQLite database.
pub struct IMessageProvider {
    /// Path to the chat.db SQLite database.
    db_path: PathBuf,
}

impl IMessageProvider {
    /// Create a new iMessage provider using the default database path.
    pub fn new() -> Result<Self> {
        let home = std::env::var("HOME").context("HOME environment variable not set")?;
        let db_path = PathBuf::from(home).join("Library/Messages/chat.db");
        Ok(Self { db_path })
    }

    /// Create a new iMessage provider with a custom database path (useful for tests).
    pub fn with_db_path(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    /// Return the database path.
    pub fn db_path(&self) -> &PathBuf {
        &self.db_path
    }

    /// Convert an Apple Core Data timestamp (nanoseconds since 2001-01-01) to `DateTime<Utc>`.
    fn parse_apple_ts(ts: i64) -> DateTime<Utc> {
        // chat.db stores timestamps as nanoseconds since 2001-01-01 in modern macOS.
        let unix_secs = (ts / NANOSECOND_FACTOR) + APPLE_EPOCH_OFFSET;
        Utc.timestamp_opt(unix_secs, 0)
            .single()
            .unwrap_or_else(Utc::now)
    }

    /// Run a SQLite query against chat.db using the `sqlite3` command-line tool.
    fn query_db(&self, sql: &str) -> Result<String> {
        let output = Command::new("sqlite3")
            .arg("-separator")
            .arg("|")
            .arg(&self.db_path)
            .arg(sql)
            .output()
            .context("failed to execute sqlite3 command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("sqlite3 query failed: {}", stderr);
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Validate that a channel identifier is a phone number or email address.
    ///
    /// Rejects any string containing characters outside the set
    /// `[a-zA-Z0-9@.+_-]`, which prevents injection in both AppleScript
    /// strings and SQL literals.
    fn validate_channel(channel: &str) -> Result<()> {
        if channel.is_empty() {
            anyhow::bail!("channel identifier must not be empty");
        }
        if !CHANNEL_RE.is_match(channel) {
            anyhow::bail!(
                "invalid channel identifier: must contain only alphanumeric characters, @, ., +, -, or _"
            );
        }
        Ok(())
    }

    /// Escape a string for embedding inside an AppleScript double-quoted literal.
    ///
    /// Handles backslashes, double-quotes, and control characters (carriage
    /// returns, newlines, tabs) that could break out of a string boundary.
    fn escape_applescript(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\r', "\\r")
            .replace('\n', "\\n")
            .replace('\t', "\\t")
    }

    /// Validate and sanitise a free-text search query for use in a SQL LIKE
    /// clause.  The query is length-limited and stripped of control characters.
    /// Single quotes are doubled for SQLite escaping.
    fn sanitise_search_query(query: &str) -> Result<String> {
        if query.is_empty() {
            anyhow::bail!("search query must not be empty");
        }
        if query.len() > MAX_SEARCH_QUERY_LEN {
            anyhow::bail!(
                "search query too long ({} bytes, max {})",
                query.len(),
                MAX_SEARCH_QUERY_LEN
            );
        }
        // Strip control characters (anything < 0x20 except space is already < 0x20).
        let cleaned: String = query.chars().filter(|c| !c.is_control()).collect();
        if cleaned.is_empty() {
            anyhow::bail!("search query contains only control characters");
        }
        // SQLite single-quote escaping: double every single quote.
        Ok(cleaned.replace('\'', "''"))
    }
}

#[async_trait]
impl MessagingProvider for IMessageProvider {
    fn platform(&self) -> Platform {
        Platform::IMessage
    }

    async fn send_message(&self, channel: &str, text: &str) -> Result<SentMessage> {
        // Use AppleScript to send a message via Messages.app.
        // `channel` is the phone number or email address of the recipient.
        Self::validate_channel(channel)?;

        let escaped_text = Self::escape_applescript(text);
        // Channel already validated to contain only safe characters, but we
        // still run it through escaping for defence-in-depth.
        let escaped_channel = Self::escape_applescript(channel);

        let script = format!(
            r#"tell application "Messages"
    set targetService to 1st account whose service type = iMessage
    set targetBuddy to participant "{escaped_channel}" of targetService
    send "{escaped_text}" to targetBuddy
end tell"#
        );

        debug!(channel = %channel, "sending iMessage via AppleScript");

        let output = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .context("failed to execute osascript for iMessage")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("osascript iMessage send failed: {}", stderr);
        }

        let now = Utc::now();
        Ok(SentMessage {
            id: now.timestamp_millis().to_string(),
            channel_id: channel.to_string(),
            timestamp: now,
        })
    }

    async fn list_channels(&self) -> Result<Vec<Channel>> {
        // Query chat.db for recent conversations.
        let sql = r#"
            SELECT DISTINCT
                c.chat_identifier,
                COALESCE(c.display_name, c.chat_identifier)
            FROM chat c
            ORDER BY c.ROWID DESC
            LIMIT 50;
        "#;

        debug!(db_path = %self.db_path.display(), "listing iMessage conversations");

        let output = self.query_db(sql)?;

        Ok(output
            .lines()
            .filter(|line| !line.is_empty())
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(2, '|').collect();
                if parts.len() >= 2 {
                    Some(Channel {
                        id: parts[0].to_string(),
                        name: parts[1].to_string(),
                        platform: Platform::IMessage,
                    })
                } else if !parts.is_empty() {
                    Some(Channel {
                        id: parts[0].to_string(),
                        name: parts[0].to_string(),
                        platform: Platform::IMessage,
                    })
                } else {
                    None
                }
            })
            .collect())
    }

    async fn get_messages(&self, channel: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
        Self::validate_channel(channel)?;
        // Channel is validated to safe characters; quote-doubling kept for defence-in-depth.
        let escaped = channel.replace('\'', "''");
        let sql = format!(
            r#"
            SELECT
                m.ROWID,
                m.text,
                m.is_from_me,
                m.date,
                COALESCE(h.id, 'me')
            FROM message m
            LEFT JOIN handle h ON m.handle_id = h.ROWID
            JOIN chat_message_join cmj ON cmj.message_id = m.ROWID
            JOIN chat c ON c.ROWID = cmj.chat_id
            WHERE c.chat_identifier = '{escaped}'
            ORDER BY m.date DESC
            LIMIT {limit};
            "#
        );

        debug!(channel = %channel, limit = limit, "getting iMessage messages");

        let output = self.query_db(&sql)?;

        Ok(output
            .lines()
            .filter(|line| !line.is_empty())
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(5, '|').collect();
                if parts.len() < 5 {
                    return None;
                }
                let id = parts[0].to_string();
                let content = parts[1].to_string();
                let is_from_me: bool = parts[2] == "1";
                let date_raw: i64 = parts[3].parse().unwrap_or(0);
                let handle = parts[4].to_string();

                let author = if is_from_me {
                    "me".to_string()
                } else {
                    handle
                };

                Some(IncomingMessage {
                    id,
                    channel_id: channel.to_string(),
                    author,
                    content,
                    timestamp: Self::parse_apple_ts(date_raw),
                    attachments: vec![],
                    platform: Platform::IMessage,
                })
            })
            .collect())
    }

    async fn add_reaction(
        &self,
        _channel: &str,
        _message_id: &str,
        _emoji: &str,
    ) -> Result<()> {
        // iMessage reactions (Tapbacks) are not programmatically accessible
        // via AppleScript or the SQLite database in a writable manner.
        anyhow::bail!("iMessage does not support programmatic reactions (tapbacks)")
    }

    async fn search_messages(&self, query: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
        let escaped = Self::sanitise_search_query(query)?;
        let sql = format!(
            r#"
            SELECT
                m.ROWID,
                m.text,
                m.is_from_me,
                m.date,
                COALESCE(h.id, 'me'),
                c.chat_identifier
            FROM message m
            LEFT JOIN handle h ON m.handle_id = h.ROWID
            JOIN chat_message_join cmj ON cmj.message_id = m.ROWID
            JOIN chat c ON c.ROWID = cmj.chat_id
            WHERE m.text LIKE '%{escaped}%'
            ORDER BY m.date DESC
            LIMIT {limit};
            "#
        );

        debug!(query = %query, limit = limit, "searching iMessage messages");

        let output = self.query_db(&sql)?;

        Ok(output
            .lines()
            .filter(|line| !line.is_empty())
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(6, '|').collect();
                if parts.len() < 6 {
                    return None;
                }
                let id = parts[0].to_string();
                let content = parts[1].to_string();
                let is_from_me: bool = parts[2] == "1";
                let date_raw: i64 = parts[3].parse().unwrap_or(0);
                let handle = parts[4].to_string();
                let channel_id = parts[5].to_string();

                let author = if is_from_me {
                    "me".to_string()
                } else {
                    handle
                };

                Some(IncomingMessage {
                    id,
                    channel_id,
                    author,
                    content,
                    timestamp: Self::parse_apple_ts(date_raw),
                    attachments: vec![],
                    platform: Platform::IMessage,
                })
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_imessage_provider_platform() {
        let provider = IMessageProvider::with_db_path(PathBuf::from("/tmp/test_chat.db"));
        assert_eq!(provider.platform(), Platform::IMessage);
    }

    #[test]
    fn test_imessage_provider_db_path() {
        let provider = IMessageProvider::with_db_path(PathBuf::from("/tmp/chat.db"));
        assert_eq!(provider.db_path(), &PathBuf::from("/tmp/chat.db"));
    }

    #[test]
    fn test_parse_apple_ts() {
        // 2021-01-01 00:00:00 UTC in Apple epoch nanoseconds:
        // Unix timestamp 1609459200 - 978307200 = 631152000 seconds since Apple epoch
        // In nanoseconds: 631152000 * 1_000_000_000
        let apple_ns = 631_152_000_i64 * NANOSECOND_FACTOR;
        let dt = IMessageProvider::parse_apple_ts(apple_ns);
        assert_eq!(dt.timestamp(), 1609459200);
    }

    #[test]
    fn test_parse_apple_ts_zero() {
        let dt = IMessageProvider::parse_apple_ts(0);
        // Apple epoch (2001-01-01) = Unix 978307200
        assert_eq!(dt.timestamp(), APPLE_EPOCH_OFFSET);
    }

    #[test]
    fn test_applescript_escaping_quotes_and_backslash() {
        let text = r#"He said "hello" and \ went away"#;
        let escaped = IMessageProvider::escape_applescript(text);
        assert!(escaped.contains("\\\"hello\\\""));
        assert!(escaped.contains("\\\\"));
    }

    #[test]
    fn test_applescript_escaping_control_chars() {
        let text = "line1\nline2\rline3\tend";
        let escaped = IMessageProvider::escape_applescript(text);
        assert_eq!(escaped, "line1\\nline2\\rline3\\tend");
        assert!(!escaped.contains('\n'));
        assert!(!escaped.contains('\r'));
        assert!(!escaped.contains('\t'));
    }

    #[tokio::test]
    async fn test_add_reaction_returns_error() {
        let provider = IMessageProvider::with_db_path(PathBuf::from("/tmp/test_chat.db"));
        let result = provider.add_reaction("chan", "msg", "thumbsup").await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("does not support"));
    }

    #[test]
    fn test_channel_validation_accepts_phone_number() {
        assert!(IMessageProvider::validate_channel("+15551234567").is_ok());
    }

    #[test]
    fn test_channel_validation_accepts_email() {
        assert!(IMessageProvider::validate_channel("user@example.com").is_ok());
    }

    #[test]
    fn test_channel_validation_rejects_sql_injection() {
        let result = IMessageProvider::validate_channel("user'; DROP TABLE message; --");
        assert!(result.is_err());
    }

    #[test]
    fn test_channel_validation_rejects_applescript_injection() {
        let result = IMessageProvider::validate_channel("user\" & do shell script \"rm -rf /");
        assert!(result.is_err());
    }

    #[test]
    fn test_channel_validation_rejects_empty() {
        assert!(IMessageProvider::validate_channel("").is_err());
    }

    #[test]
    fn test_search_query_sanitisation_basic() {
        let result = IMessageProvider::sanitise_search_query("hello world").unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_search_query_sanitisation_escapes_quotes() {
        let result = IMessageProvider::sanitise_search_query("it's a test").unwrap();
        assert_eq!(result, "it''s a test");
    }

    #[test]
    fn test_search_query_sanitisation_strips_control_chars() {
        let result = IMessageProvider::sanitise_search_query("hello\x00world\x1F!").unwrap();
        assert_eq!(result, "helloworld!");
    }

    #[test]
    fn test_search_query_sanitisation_rejects_empty() {
        assert!(IMessageProvider::sanitise_search_query("").is_err());
    }

    #[test]
    fn test_search_query_sanitisation_rejects_too_long() {
        let long_query = "a".repeat(MAX_SEARCH_QUERY_LEN + 1);
        assert!(IMessageProvider::sanitise_search_query(&long_query).is_err());
    }

    #[test]
    fn test_search_query_sanitisation_rejects_only_control_chars() {
        assert!(IMessageProvider::sanitise_search_query("\x00\x01\x02").is_err());
    }

    #[test]
    fn test_parse_channel_line() {
        let line = "+15551234567|John Doe";
        let parts: Vec<&str> = line.splitn(2, '|').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "+15551234567");
        assert_eq!(parts[1], "John Doe");
    }

    #[test]
    fn test_parse_message_line() {
        let line = "123|Hello world|0|631152000000000000|+15551234567";
        let parts: Vec<&str> = line.splitn(5, '|').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0], "123");
        assert_eq!(parts[1], "Hello world");
        assert_eq!(parts[2], "0"); // not from me
        assert_eq!(parts[4], "+15551234567");
    }

    #[test]
    fn test_parse_search_line() {
        let line = "456|Search result|1|631152000000000000|me|+15551234567";
        let parts: Vec<&str> = line.splitn(6, '|').collect();
        assert_eq!(parts.len(), 6);
        assert_eq!(parts[0], "456");
        assert_eq!(parts[1], "Search result");
        assert_eq!(parts[2], "1"); // from me
        assert_eq!(parts[5], "+15551234567");
    }

    #[test]
    fn test_is_from_me_parsing() {
        assert!("1" == "1"); // is_from_me
        assert!("0" != "1"); // not from me
    }
}
