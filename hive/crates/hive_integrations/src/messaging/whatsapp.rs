//! WhatsApp messaging provider.
//!
//! Wraps the WhatsApp Business Cloud API (Meta) at
//! `https://graph.facebook.com/v18.0/{phone_number_id}/messages`
//! using `reqwest` for HTTP and bearer-token authentication.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;
use tracing::debug;

use super::provider::{Channel, IncomingMessage, MessagingProvider, Platform, SentMessage};

const DEFAULT_BASE_URL: &str = "https://graph.facebook.com/v18.0";

// ── WhatsApp API response types ─────────────────────────────────

/// Envelope returned by WhatsApp Cloud API send-message endpoint.
#[derive(Debug, Deserialize)]
struct WhatsAppSendResponse {
    messages: Option<Vec<WhatsAppMessageRef>>,
    error: Option<WhatsAppError>,
}

/// Reference to a sent message.
#[derive(Debug, Deserialize)]
struct WhatsAppMessageRef {
    id: String,
}

/// Error object from WhatsApp Cloud API.
#[derive(Debug, Deserialize)]
struct WhatsAppError {
    message: String,
    #[allow(dead_code)]
    code: Option<i64>,
}

// ── Client ──────────────────────────────────────────────────────

/// WhatsApp messaging provider using the WhatsApp Business Cloud API.
pub struct WhatsAppProvider {
    base_url: String,
    phone_number_id: String,
    access_token: String,
    client: Client,
}

impl WhatsAppProvider {
    /// Create a new WhatsApp provider with the given phone number ID and access token.
    pub fn new(phone_number_id: &str, access_token: &str) -> Result<Self> {
        Self::with_base_url(phone_number_id, access_token, DEFAULT_BASE_URL)
    }

    /// Create a new WhatsApp provider pointing at a custom base URL (useful for tests).
    pub fn with_base_url(
        phone_number_id: &str,
        access_token: &str,
        base_url: &str,
    ) -> Result<Self> {
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
            .context("failed to build HTTP client for WhatsApp")?;

        Ok(Self {
            base_url,
            phone_number_id: phone_number_id.to_string(),
            access_token: access_token.to_string(),
            client,
        })
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Return the stored phone number ID.
    pub fn phone_number_id(&self) -> &str {
        &self.phone_number_id
    }

    /// Return the stored access token.
    pub fn access_token(&self) -> &str {
        &self.access_token
    }

    /// Build the messages endpoint URL.
    fn messages_url(&self) -> String {
        format!("{}/{}/messages", self.base_url, self.phone_number_id)
    }
}

#[async_trait]
impl MessagingProvider for WhatsAppProvider {
    fn platform(&self) -> Platform {
        Platform::WhatsApp
    }

    async fn send_message(&self, channel: &str, text: &str) -> Result<SentMessage> {
        let url = self.messages_url();
        let payload = serde_json::json!({
            "messaging_product": "whatsapp",
            "to": channel,
            "type": "text",
            "text": { "body": text }
        });

        debug!(url = %url, channel = %channel, "sending WhatsApp message");

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("WhatsApp send message request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("WhatsApp API HTTP error ({}): {}", status, body);
        }

        let envelope: WhatsAppSendResponse = resp
            .json()
            .await
            .context("failed to parse WhatsApp send response")?;

        if let Some(err) = envelope.error {
            anyhow::bail!("WhatsApp API error: {}", err.message);
        }

        let message_id = envelope
            .messages
            .and_then(|msgs| msgs.into_iter().next())
            .map(|m| m.id)
            .unwrap_or_else(|| "unknown".into());

        Ok(SentMessage {
            id: message_id,
            channel_id: channel.to_string(),
            timestamp: Utc::now(),
        })
    }

    async fn list_channels(&self) -> Result<Vec<Channel>> {
        // WhatsApp doesn't have channels in the traditional sense.
        // Return a single channel representing this phone number.
        Ok(vec![Channel {
            id: self.phone_number_id.clone(),
            name: format!("WhatsApp ({})", self.phone_number_id),
            platform: Platform::WhatsApp,
        }])
    }

    async fn get_messages(&self, _channel: &str, _limit: u32) -> Result<Vec<IncomingMessage>> {
        // WhatsApp Cloud API does not support pulling messages directly.
        // Messages are received via webhooks and must be stored externally.
        debug!("WhatsApp get_messages: not directly supported by Cloud API, returning empty");
        Ok(vec![])
    }

    async fn add_reaction(
        &self,
        _channel: &str,
        message_id: &str,
        emoji: &str,
    ) -> Result<()> {
        let url = self.messages_url();
        let payload = serde_json::json!({
            "messaging_product": "whatsapp",
            "recipient_type": "individual",
            "to": _channel,
            "type": "reaction",
            "reaction": {
                "message_id": message_id,
                "emoji": emoji
            }
        });

        debug!(url = %url, message_id = %message_id, emoji = %emoji, "adding WhatsApp reaction");

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("WhatsApp add reaction request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("WhatsApp API HTTP error ({}): {}", status, body);
        }

        let envelope: WhatsAppSendResponse = resp
            .json()
            .await
            .context("failed to parse WhatsApp reaction response")?;

        if let Some(err) = envelope.error {
            anyhow::bail!("WhatsApp API error: {}", err.message);
        }

        Ok(())
    }

    async fn search_messages(&self, _query: &str, _limit: u32) -> Result<Vec<IncomingMessage>> {
        // WhatsApp Cloud API does not support message search.
        // Client-side search would require stored webhook messages.
        debug!("WhatsApp search_messages: not supported by Cloud API, returning empty");
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_provider() -> WhatsAppProvider {
        WhatsAppProvider::with_base_url("123456789", "test-access-token", DEFAULT_BASE_URL)
            .unwrap()
    }

    #[test]
    fn test_whatsapp_provider_default_base_url() {
        let provider = WhatsAppProvider::new("123456789", "test-token").unwrap();
        assert_eq!(provider.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_whatsapp_provider_custom_base_url_strips_slash() {
        let provider =
            WhatsAppProvider::with_base_url("123", "tok", "https://graph.test/").unwrap();
        assert_eq!(provider.base_url(), "https://graph.test");
    }

    #[test]
    fn test_whatsapp_provider_fields_stored() {
        let provider = make_provider();
        assert_eq!(provider.phone_number_id(), "123456789");
        assert_eq!(provider.access_token(), "test-access-token");
    }

    #[test]
    fn test_whatsapp_provider_platform() {
        let provider = make_provider();
        assert_eq!(provider.platform(), Platform::WhatsApp);
    }

    #[test]
    fn test_messages_url_construction() {
        let provider = make_provider();
        let url = provider.messages_url();
        assert_eq!(
            url,
            "https://graph.facebook.com/v18.0/123456789/messages"
        );
    }

    #[test]
    fn test_send_message_payload() {
        let payload = serde_json::json!({
            "messaging_product": "whatsapp",
            "to": "+15551234567",
            "type": "text",
            "text": { "body": "Hello, WhatsApp!" }
        });
        assert_eq!(payload["messaging_product"], "whatsapp");
        assert_eq!(payload["to"], "+15551234567");
        assert_eq!(payload["type"], "text");
        assert_eq!(payload["text"]["body"], "Hello, WhatsApp!");
    }

    #[test]
    fn test_reaction_payload() {
        let payload = serde_json::json!({
            "messaging_product": "whatsapp",
            "recipient_type": "individual",
            "to": "+15551234567",
            "type": "reaction",
            "reaction": {
                "message_id": "wamid.abc123",
                "emoji": "\u{1F44D}"
            }
        });
        assert_eq!(payload["type"], "reaction");
        assert_eq!(payload["reaction"]["message_id"], "wamid.abc123");
    }

    #[test]
    fn test_send_response_deserialization_ok() {
        let json = r#"{"messages": [{"id": "wamid.abc123"}]}"#;
        let resp: WhatsAppSendResponse = serde_json::from_str(json).unwrap();
        assert!(resp.error.is_none());
        let msgs = resp.messages.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].id, "wamid.abc123");
    }

    #[test]
    fn test_send_response_deserialization_error() {
        let json = r#"{"error": {"message": "Invalid token", "code": 190}}"#;
        let resp: WhatsAppSendResponse = serde_json::from_str(json).unwrap();
        assert!(resp.messages.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.message, "Invalid token");
        assert_eq!(err.code, Some(190));
    }

    #[tokio::test]
    async fn test_list_channels_returns_phone_number() {
        let provider = make_provider();
        let channels = provider.list_channels().await.unwrap();
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].id, "123456789");
        assert_eq!(channels[0].platform, Platform::WhatsApp);
    }

    #[tokio::test]
    async fn test_get_messages_returns_empty() {
        let provider = make_provider();
        let messages = provider.get_messages("any-channel", 10).await.unwrap();
        assert!(messages.is_empty());
    }

    #[tokio::test]
    async fn test_search_messages_returns_empty() {
        let provider = make_provider();
        let messages = provider.search_messages("query", 10).await.unwrap();
        assert!(messages.is_empty());
    }
}
