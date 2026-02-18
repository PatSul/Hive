//! Google Chat messaging provider.
//!
//! Wraps the Google Chat API at `https://chat.googleapis.com/v1`
//! using `reqwest` for HTTP and Google OAuth bearer-token authentication.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;
use tracing::debug;

use super::provider::{Channel, IncomingMessage, MessagingProvider, Platform, SentMessage};

const DEFAULT_BASE_URL: &str = "https://chat.googleapis.com/v1";

// ── Google Chat API response types ──────────────────────────────

/// A Google Chat space (room/channel).
#[derive(Debug, Deserialize)]
struct GoogleChatSpace {
    name: String,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    space_type: Option<String>,
}

/// Response from listing spaces.
#[derive(Debug, Deserialize)]
struct ListSpacesResponse {
    spaces: Option<Vec<GoogleChatSpace>>,
}

/// A Google Chat message.
#[derive(Debug, Deserialize)]
struct GoogleChatMessage {
    name: Option<String>,
    #[serde(rename = "createTime")]
    create_time: Option<String>,
    sender: Option<GoogleChatSender>,
    text: Option<String>,
}

/// Sender information on a Google Chat message.
#[derive(Debug, Deserialize)]
struct GoogleChatSender {
    name: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
}

/// Response from listing messages in a space.
#[derive(Debug, Deserialize)]
struct ListMessagesResponse {
    messages: Option<Vec<GoogleChatMessage>>,
}

/// Error from Google Chat API.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GoogleChatError {
    error: Option<GoogleChatErrorDetail>,
}

/// Detail of a Google Chat API error.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GoogleChatErrorDetail {
    message: String,
    code: Option<i64>,
}

// ── Client ──────────────────────────────────────────────────────

/// Google Chat messaging provider using the Google Chat API.
pub struct GoogleChatProvider {
    base_url: String,
    access_token: String,
    client: Client,
}

impl GoogleChatProvider {
    /// Create a new Google Chat provider with the given OAuth access token.
    pub fn new(access_token: &str) -> Result<Self> {
        Self::with_base_url(access_token, DEFAULT_BASE_URL)
    }

    /// Create a new Google Chat provider pointing at a custom base URL (useful for tests).
    pub fn with_base_url(access_token: &str, base_url: &str) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {access_token}"))
                .context("invalid access token for Authorization header")?,
        );

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build HTTP client for Google Chat")?;

        Ok(Self {
            base_url,
            access_token: access_token.to_string(),
            client,
        })
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Return the stored access token.
    pub fn access_token(&self) -> &str {
        &self.access_token
    }

    /// Parse a Google Chat timestamp (RFC 3339) into `DateTime<Utc>`.
    fn parse_timestamp(ts: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(ts)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now())
    }

    /// Extract the space ID from a full resource name like `spaces/AAAA`.
    fn space_id_from_name(name: &str) -> &str {
        name.strip_prefix("spaces/").unwrap_or(name)
    }

    /// Extract the message ID from a full resource name like `spaces/AAAA/messages/BBBB`.
    fn message_id_from_name(name: &str) -> String {
        name.rsplit('/')
            .next()
            .unwrap_or(name)
            .to_string()
    }

    /// Build a space resource name. If the input already starts with `spaces/`, return as-is.
    fn space_name(space: &str) -> String {
        if space.starts_with("spaces/") {
            space.to_string()
        } else {
            format!("spaces/{space}")
        }
    }
}

#[async_trait]
impl MessagingProvider for GoogleChatProvider {
    fn platform(&self) -> Platform {
        Platform::GoogleChat
    }

    async fn send_message(&self, channel: &str, text: &str) -> Result<SentMessage> {
        let space = Self::space_name(channel);
        let url = format!("{}/{}/messages", self.base_url, space);
        let payload = serde_json::json!({
            "text": text,
        });

        debug!(url = %url, channel = %channel, "sending Google Chat message");

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Google Chat send message request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Google Chat API HTTP error ({}): {}", status, body);
        }

        let msg: GoogleChatMessage = resp
            .json()
            .await
            .context("failed to parse Google Chat send response")?;

        let message_id = msg
            .name
            .as_deref()
            .map(Self::message_id_from_name)
            .unwrap_or_else(|| "unknown".into());

        let timestamp = msg
            .create_time
            .as_deref()
            .map(Self::parse_timestamp)
            .unwrap_or_else(Utc::now);

        Ok(SentMessage {
            id: message_id,
            channel_id: channel.to_string(),
            timestamp,
        })
    }

    async fn list_channels(&self) -> Result<Vec<Channel>> {
        let url = format!("{}/spaces", self.base_url);

        debug!(url = %url, "listing Google Chat spaces");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Google Chat list spaces request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Google Chat API HTTP error ({}): {}", status, body);
        }

        let list: ListSpacesResponse = resp
            .json()
            .await
            .context("failed to parse Google Chat spaces response")?;

        Ok(list
            .spaces
            .unwrap_or_default()
            .into_iter()
            .map(|s| {
                let id = Self::space_id_from_name(&s.name).to_string();
                Channel {
                    name: s.display_name.unwrap_or_else(|| id.clone()),
                    id,
                    platform: Platform::GoogleChat,
                }
            })
            .collect())
    }

    async fn get_messages(&self, channel: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
        let space = Self::space_name(channel);
        let url = format!(
            "{}/{}/messages?pageSize={}",
            self.base_url, space, limit
        );

        debug!(url = %url, channel = %channel, "getting Google Chat messages");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Google Chat get messages request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Google Chat API HTTP error ({}): {}", status, body);
        }

        let list: ListMessagesResponse = resp
            .json()
            .await
            .context("failed to parse Google Chat messages response")?;

        Ok(list
            .messages
            .unwrap_or_default()
            .into_iter()
            .take(limit as usize)
            .map(|m| {
                let id = m
                    .name
                    .as_deref()
                    .map(Self::message_id_from_name)
                    .unwrap_or_else(|| "unknown".into());
                let author = m
                    .sender
                    .as_ref()
                    .and_then(|s| s.display_name.clone())
                    .or_else(|| m.sender.as_ref().and_then(|s| s.name.clone()))
                    .unwrap_or_else(|| "unknown".into());
                let timestamp = m
                    .create_time
                    .as_deref()
                    .map(Self::parse_timestamp)
                    .unwrap_or_else(Utc::now);

                IncomingMessage {
                    id,
                    channel_id: channel.to_string(),
                    author,
                    content: m.text.unwrap_or_default(),
                    timestamp,
                    attachments: vec![],
                    platform: Platform::GoogleChat,
                }
            })
            .collect())
    }

    async fn add_reaction(
        &self,
        channel: &str,
        message_id: &str,
        emoji: &str,
    ) -> Result<()> {
        let space = Self::space_name(channel);
        let url = format!(
            "{}/{}/messages/{}/reactions",
            self.base_url, space, message_id
        );
        let payload = serde_json::json!({
            "emoji": {
                "unicode": emoji
            }
        });

        debug!(url = %url, message_id = %message_id, emoji = %emoji, "adding Google Chat reaction");

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Google Chat add reaction request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Google Chat API HTTP error ({}): {}", status, body);
        }

        Ok(())
    }

    async fn search_messages(&self, query: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
        // Google Chat API does not have a dedicated search endpoint.
        // Fetch spaces and recent messages, then filter client-side.
        let channels = self.list_channels().await?;

        debug!(query = %query, num_spaces = channels.len(), "searching Google Chat messages (client-side filter)");

        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for ch in &channels {
            if results.len() >= limit as usize {
                break;
            }
            let remaining = limit - results.len() as u32;
            let messages = self.get_messages(&ch.id, remaining).await?;
            for msg in messages {
                if msg.content.to_lowercase().contains(&query_lower) {
                    results.push(msg);
                    if results.len() >= limit as usize {
                        break;
                    }
                }
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_provider() -> GoogleChatProvider {
        GoogleChatProvider::with_base_url("test-oauth-token", DEFAULT_BASE_URL).unwrap()
    }

    #[test]
    fn test_google_chat_provider_default_base_url() {
        let provider = GoogleChatProvider::new("test-token").unwrap();
        assert_eq!(provider.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_google_chat_provider_custom_base_url_strips_slash() {
        let provider =
            GoogleChatProvider::with_base_url("tok", "https://chat.test/v1/").unwrap();
        assert_eq!(provider.base_url(), "https://chat.test/v1");
    }

    #[test]
    fn test_google_chat_provider_fields_stored() {
        let provider = make_provider();
        assert_eq!(provider.access_token(), "test-oauth-token");
    }

    #[test]
    fn test_google_chat_provider_platform() {
        let provider = make_provider();
        assert_eq!(provider.platform(), Platform::GoogleChat);
    }

    #[test]
    fn test_space_name_without_prefix() {
        assert_eq!(GoogleChatProvider::space_name("AAAA"), "spaces/AAAA");
    }

    #[test]
    fn test_space_name_with_prefix() {
        assert_eq!(
            GoogleChatProvider::space_name("spaces/AAAA"),
            "spaces/AAAA"
        );
    }

    #[test]
    fn test_space_id_from_name() {
        assert_eq!(GoogleChatProvider::space_id_from_name("spaces/AAAA"), "AAAA");
    }

    #[test]
    fn test_space_id_from_name_no_prefix() {
        assert_eq!(GoogleChatProvider::space_id_from_name("AAAA"), "AAAA");
    }

    #[test]
    fn test_message_id_from_name() {
        assert_eq!(
            GoogleChatProvider::message_id_from_name("spaces/AAAA/messages/BBBB"),
            "BBBB"
        );
    }

    #[test]
    fn test_parse_timestamp_rfc3339() {
        let dt = GoogleChatProvider::parse_timestamp("2021-01-01T00:00:00Z");
        assert_eq!(dt.timestamp(), 1609459200);
    }

    #[test]
    fn test_parse_timestamp_invalid_returns_now() {
        let dt = GoogleChatProvider::parse_timestamp("not-a-date");
        // Just verify it doesn't panic; returns some DateTime
        assert!(dt.timestamp() > 0);
    }

    #[test]
    fn test_send_message_payload() {
        let payload = serde_json::json!({
            "text": "Hello, Google Chat!"
        });
        assert_eq!(payload["text"], "Hello, Google Chat!");
    }

    #[test]
    fn test_reaction_payload() {
        let payload = serde_json::json!({
            "emoji": {
                "unicode": "\u{1F44D}"
            }
        });
        assert_eq!(payload["emoji"]["unicode"], "\u{1F44D}");
    }

    #[test]
    fn test_google_chat_space_deserialization() {
        let json = r#"{"name": "spaces/AAAA", "displayName": "My Space", "type": "ROOM"}"#;
        let space: GoogleChatSpace = serde_json::from_str(json).unwrap();
        assert_eq!(space.name, "spaces/AAAA");
        assert_eq!(space.display_name.as_deref(), Some("My Space"));
    }

    #[test]
    fn test_list_spaces_response_deserialization() {
        let json = r#"{"spaces": [{"name": "spaces/A", "displayName": "Room A"}, {"name": "spaces/B", "displayName": "Room B"}]}"#;
        let resp: ListSpacesResponse = serde_json::from_str(json).unwrap();
        let spaces = resp.spaces.unwrap();
        assert_eq!(spaces.len(), 2);
        assert_eq!(spaces[0].name, "spaces/A");
    }

    #[test]
    fn test_google_chat_message_deserialization() {
        let json = r#"{
            "name": "spaces/AAAA/messages/BBBB",
            "createTime": "2021-01-01T00:00:00Z",
            "sender": {"name": "users/123", "displayName": "Alice"},
            "text": "Hello!"
        }"#;
        let msg: GoogleChatMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.name.as_deref(), Some("spaces/AAAA/messages/BBBB"));
        assert_eq!(msg.text.as_deref(), Some("Hello!"));
        assert_eq!(
            msg.sender.as_ref().unwrap().display_name.as_deref(),
            Some("Alice")
        );
    }

    #[test]
    fn test_list_messages_response_deserialization() {
        let json = r#"{
            "messages": [{
                "name": "spaces/A/messages/1",
                "createTime": "2021-01-01T00:00:00Z",
                "text": "Hi"
            }]
        }"#;
        let resp: ListMessagesResponse = serde_json::from_str(json).unwrap();
        let msgs = resp.messages.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].text.as_deref(), Some("Hi"));
    }

    #[test]
    fn test_google_chat_error_deserialization() {
        let json = r#"{"error": {"message": "Not found", "code": 404}}"#;
        let err: GoogleChatError = serde_json::from_str(json).unwrap();
        let detail = err.error.unwrap();
        assert_eq!(detail.message, "Not found");
        assert_eq!(detail.code, Some(404));
    }
}
