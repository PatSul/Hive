//! Signal messaging provider.
//!
//! Wraps the Signal CLI REST API (`signal-cli-rest-api`) which users run
//! as a local service. Default base URL: `http://localhost:8080/v2`.
//! Supports optional basic authentication.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use reqwest::Client;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;
use tracing::debug;

use super::provider::{Channel, IncomingMessage, MessagingProvider, Platform, SentMessage};

const DEFAULT_BASE_URL: &str = "http://localhost:8080/v2";

// ── Signal CLI REST API response types ──────────────────────────

/// Response from the `/send` endpoint.
#[derive(Debug, Deserialize)]
struct SignalSendResponse {
    timestamp: Option<i64>,
}

/// A Signal group from `/groups`.
#[derive(Debug, Deserialize)]
struct SignalGroup {
    id: String,
    name: Option<String>,
    #[serde(rename = "internal_id")]
    #[allow(dead_code)]
    internal_id: Option<String>,
}

/// An incoming Signal message from `/receive`.
#[derive(Debug, Deserialize)]
struct SignalEnvelope {
    #[serde(rename = "sourceNumber")]
    source_number: Option<String>,
    #[serde(rename = "sourceName")]
    source_name: Option<String>,
    timestamp: Option<i64>,
    #[serde(rename = "dataMessage")]
    data_message: Option<SignalDataMessage>,
}

/// The data portion of a Signal message.
#[derive(Debug, Deserialize)]
struct SignalDataMessage {
    message: Option<String>,
    timestamp: Option<i64>,
    #[serde(rename = "groupInfo")]
    group_info: Option<SignalGroupInfo>,
}

/// Group information attached to a Signal message.
#[derive(Debug, Deserialize)]
struct SignalGroupInfo {
    #[serde(rename = "groupId")]
    group_id: String,
}

// ── Client ──────────────────────────────────────────────────────

/// Signal messaging provider using the signal-cli-rest-api.
pub struct SignalProvider {
    base_url: String,
    /// The registered phone number used by the local signal-cli instance.
    number: String,
    /// Optional basic-auth credentials (`user:password`).
    basic_auth: Option<(String, String)>,
    client: Client,
}

impl SignalProvider {
    /// Create a new Signal provider.
    ///
    /// `number` is the phone number registered with signal-cli (e.g. `+15551234567`).
    pub fn new(number: &str) -> Result<Self> {
        Self::with_base_url(number, DEFAULT_BASE_URL, None)
    }

    /// Create a new Signal provider with custom base URL and optional basic auth.
    pub fn with_base_url(
        number: &str,
        base_url: &str,
        basic_auth: Option<(&str, &str)>,
    ) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("failed to build HTTP client for Signal")?;

        Ok(Self {
            base_url,
            number: number.to_string(),
            basic_auth: basic_auth.map(|(u, p)| (u.to_string(), p.to_string())),
            client,
        })
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Return the stored phone number.
    pub fn number(&self) -> &str {
        &self.number
    }

    /// Apply optional basic auth to a request builder.
    fn apply_auth(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some((ref user, ref pass)) = self.basic_auth {
            builder.basic_auth(user, Some(pass))
        } else {
            builder
        }
    }

    /// Parse a Signal timestamp (milliseconds) into `DateTime<Utc>`.
    fn parse_signal_ts(ts_millis: i64) -> DateTime<Utc> {
        let secs = ts_millis / 1000;
        let nanos = ((ts_millis % 1000) * 1_000_000) as u32;
        Utc.timestamp_opt(secs, nanos)
            .single()
            .unwrap_or_else(Utc::now)
    }
}

#[async_trait]
impl MessagingProvider for SignalProvider {
    fn platform(&self) -> Platform {
        Platform::Signal
    }

    async fn send_message(&self, channel: &str, text: &str) -> Result<SentMessage> {
        let url = format!("{}/send", self.base_url);
        let payload = serde_json::json!({
            "message": text,
            "number": self.number,
            "recipients": [channel],
        });

        debug!(url = %url, channel = %channel, "sending Signal message");

        let builder = self.client.post(&url).json(&payload);
        let resp = self
            .apply_auth(builder)
            .send()
            .await
            .context("Signal send request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Signal API HTTP error ({}): {}", status, body);
        }

        let send_resp: SignalSendResponse = resp
            .json()
            .await
            .context("failed to parse Signal send response")?;

        let ts = send_resp.timestamp.unwrap_or_else(|| Utc::now().timestamp_millis());

        Ok(SentMessage {
            id: ts.to_string(),
            channel_id: channel.to_string(),
            timestamp: Self::parse_signal_ts(ts),
        })
    }

    async fn list_channels(&self) -> Result<Vec<Channel>> {
        let url = format!("{}/groups/{}", self.base_url, self.number);

        debug!(url = %url, "listing Signal groups");

        let builder = self.client.get(&url);
        let resp = self
            .apply_auth(builder)
            .send()
            .await
            .context("Signal groups request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Signal API HTTP error ({}): {}", status, body);
        }

        let groups: Vec<SignalGroup> = resp
            .json()
            .await
            .context("failed to parse Signal groups response")?;

        Ok(groups
            .into_iter()
            .map(|g| Channel {
                id: g.id.clone(),
                name: g.name.unwrap_or_else(|| g.id),
                platform: Platform::Signal,
            })
            .collect())
    }

    async fn get_messages(&self, channel: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
        let url = format!("{}/receive/{}", self.base_url, self.number);

        debug!(url = %url, channel = %channel, "receiving Signal messages");

        let builder = self.client.get(&url);
        let resp = self
            .apply_auth(builder)
            .send()
            .await
            .context("Signal receive request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Signal API HTTP error ({}): {}", status, body);
        }

        let envelopes: Vec<SignalEnvelope> = resp
            .json()
            .await
            .context("failed to parse Signal receive response")?;

        Ok(envelopes
            .into_iter()
            .filter(|e| {
                e.data_message
                    .as_ref()
                    .and_then(|dm| dm.group_info.as_ref())
                    .map(|gi| gi.group_id == channel)
                    .unwrap_or_else(|| {
                        // For 1:1 chats, match on source number
                        e.source_number.as_deref() == Some(channel)
                    })
            })
            .filter_map(|e| {
                let dm = e.data_message.as_ref()?;
                let content = dm.message.clone().unwrap_or_default();
                let ts = dm
                    .timestamp
                    .or(e.timestamp)
                    .unwrap_or_else(|| Utc::now().timestamp_millis());

                Some(IncomingMessage {
                    id: ts.to_string(),
                    channel_id: channel.to_string(),
                    author: e
                        .source_name
                        .clone()
                        .or_else(|| e.source_number.clone())
                        .unwrap_or_else(|| "unknown".into()),
                    content,
                    timestamp: Self::parse_signal_ts(ts),
                    attachments: vec![],
                    platform: Platform::Signal,
                })
            })
            .take(limit as usize)
            .collect())
    }

    async fn add_reaction(
        &self,
        _channel: &str,
        message_id: &str,
        emoji: &str,
    ) -> Result<()> {
        let url = format!("{}/reactions/{}", self.base_url, self.number);
        let payload = serde_json::json!({
            "recipient": _channel,
            "reaction": emoji,
            "target_author": _channel,
            "timestamp": message_id.parse::<i64>().unwrap_or(0),
        });

        debug!(url = %url, message_id = %message_id, emoji = %emoji, "adding Signal reaction");

        let builder = self.client.post(&url).json(&payload);
        let resp = self
            .apply_auth(builder)
            .send()
            .await
            .context("Signal reaction request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Signal API HTTP error ({}): {}", status, body);
        }

        Ok(())
    }

    async fn search_messages(&self, query: &str, limit: u32) -> Result<Vec<IncomingMessage>> {
        // Signal CLI REST API has no search endpoint. Fetch recent messages
        // and filter client-side.
        let url = format!("{}/receive/{}", self.base_url, self.number);

        debug!(url = %url, query = %query, "searching Signal messages (client-side filter)");

        let builder = self.client.get(&url);
        let resp = self
            .apply_auth(builder)
            .send()
            .await
            .context("Signal receive request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Signal API HTTP error ({}): {}", status, body);
        }

        let envelopes: Vec<SignalEnvelope> = resp
            .json()
            .await
            .context("failed to parse Signal receive response")?;

        let query_lower = query.to_lowercase();

        Ok(envelopes
            .into_iter()
            .filter_map(|e| {
                let dm = e.data_message.as_ref()?;
                let content = dm.message.clone()?;
                if !content.to_lowercase().contains(&query_lower) {
                    return None;
                }
                let ts = dm
                    .timestamp
                    .or(e.timestamp)
                    .unwrap_or_else(|| Utc::now().timestamp_millis());
                let channel_id = dm
                    .group_info
                    .as_ref()
                    .map(|gi| gi.group_id.clone())
                    .or_else(|| e.source_number.clone())
                    .unwrap_or_else(|| "unknown".into());

                Some(IncomingMessage {
                    id: ts.to_string(),
                    channel_id,
                    author: e
                        .source_name
                        .clone()
                        .or_else(|| e.source_number.clone())
                        .unwrap_or_else(|| "unknown".into()),
                    content,
                    timestamp: Self::parse_signal_ts(ts),
                    attachments: vec![],
                    platform: Platform::Signal,
                })
            })
            .take(limit as usize)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_provider() -> SignalProvider {
        SignalProvider::with_base_url("+15551234567", DEFAULT_BASE_URL, None).unwrap()
    }

    fn make_provider_with_auth() -> SignalProvider {
        SignalProvider::with_base_url(
            "+15551234567",
            "http://signal.local:9090/v2",
            Some(("admin", "secret")),
        )
        .unwrap()
    }

    #[test]
    fn test_signal_provider_default_base_url() {
        let provider = SignalProvider::new("+15551234567").unwrap();
        assert_eq!(provider.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_signal_provider_custom_base_url_strips_slash() {
        let provider =
            SignalProvider::with_base_url("+1555", "http://localhost:8080/v2/", None).unwrap();
        assert_eq!(provider.base_url(), "http://localhost:8080/v2");
    }

    #[test]
    fn test_signal_provider_fields_stored() {
        let provider = make_provider();
        assert_eq!(provider.number(), "+15551234567");
        assert!(provider.basic_auth.is_none());
    }

    #[test]
    fn test_signal_provider_with_auth() {
        let provider = make_provider_with_auth();
        assert!(provider.basic_auth.is_some());
        let (user, pass) = provider.basic_auth.as_ref().unwrap();
        assert_eq!(user, "admin");
        assert_eq!(pass, "secret");
    }

    #[test]
    fn test_signal_provider_platform() {
        let provider = make_provider();
        assert_eq!(provider.platform(), Platform::Signal);
    }

    #[test]
    fn test_send_payload() {
        let payload = serde_json::json!({
            "message": "Hello, Signal!",
            "number": "+15551234567",
            "recipients": ["+15559876543"],
        });
        assert_eq!(payload["message"], "Hello, Signal!");
        assert_eq!(payload["number"], "+15551234567");
        assert_eq!(payload["recipients"][0], "+15559876543");
    }

    #[test]
    fn test_reaction_payload() {
        let payload = serde_json::json!({
            "recipient": "+15559876543",
            "reaction": "\u{1F44D}",
            "target_author": "+15559876543",
            "timestamp": 1609459200000_i64,
        });
        assert_eq!(payload["reaction"], "\u{1F44D}");
        assert_eq!(payload["timestamp"], 1609459200000_i64);
    }

    #[test]
    fn test_parse_signal_ts() {
        let dt = SignalProvider::parse_signal_ts(1609459200000);
        assert_eq!(dt.timestamp(), 1609459200);
    }

    #[test]
    fn test_parse_signal_ts_zero() {
        let dt = SignalProvider::parse_signal_ts(0);
        assert_eq!(dt.timestamp(), 0);
    }

    #[test]
    fn test_signal_send_response_deserialization() {
        let json = r#"{"timestamp": 1609459200000}"#;
        let resp: SignalSendResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.timestamp, Some(1609459200000));
    }

    #[test]
    fn test_signal_group_deserialization() {
        let json = r#"{"id": "group-abc", "name": "Test Group"}"#;
        let group: SignalGroup = serde_json::from_str(json).unwrap();
        assert_eq!(group.id, "group-abc");
        assert_eq!(group.name.as_deref(), Some("Test Group"));
    }

    #[test]
    fn test_signal_envelope_deserialization() {
        let json = r#"{
            "sourceNumber": "+15559876543",
            "sourceName": "Alice",
            "timestamp": 1609459200000,
            "dataMessage": {
                "message": "Hello from Signal",
                "timestamp": 1609459200000
            }
        }"#;
        let envelope: SignalEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.source_number.as_deref(), Some("+15559876543"));
        assert_eq!(envelope.source_name.as_deref(), Some("Alice"));
        let dm = envelope.data_message.unwrap();
        assert_eq!(dm.message.as_deref(), Some("Hello from Signal"));
    }

    #[test]
    fn test_signal_envelope_with_group() {
        let json = r#"{
            "sourceNumber": "+15559876543",
            "timestamp": 1609459200000,
            "dataMessage": {
                "message": "Group message",
                "timestamp": 1609459200000,
                "groupInfo": {
                    "groupId": "grp-123"
                }
            }
        }"#;
        let envelope: SignalEnvelope = serde_json::from_str(json).unwrap();
        let dm = envelope.data_message.unwrap();
        let gi = dm.group_info.unwrap();
        assert_eq!(gi.group_id, "grp-123");
    }
}
