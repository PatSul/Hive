//! Authentication and URL validation middleware for the A2A protocol.
//!
//! Provides API key validation for inbound requests and URL security
//! validation for outbound connections to remote agents.

use url::Url;

use crate::error::A2aError;

/// Validate that the provided API key matches the expected key.
///
/// Returns `Ok(())` if the keys match, or an appropriate `A2aError::Auth`
/// if the key is missing or mismatched.
pub fn validate_api_key(provided: Option<&str>, expected: &str) -> Result<(), A2aError> {
    match provided {
        Some(key) if key == expected => Ok(()),
        Some(_) => Err(A2aError::Auth("Invalid API key".into())),
        None => Err(A2aError::Auth("Missing X-Hive-Key header".into())),
    }
}

/// Validate an API key only when the server has one configured.
///
/// If `expected` is `None` (no key configured), all requests are allowed.
/// If `expected` is `Some`, delegates to [`validate_api_key`].
pub fn validate_api_key_optional(
    provided: Option<&str>,
    expected: Option<&str>,
) -> Result<(), A2aError> {
    match expected {
        None => Ok(()),
        Some(key) => validate_api_key(provided, key),
    }
}

/// Returns `true` if the host is a localhost address.
fn is_localhost(host: &str) -> bool {
    host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "[::1]"
}

/// Returns `true` if the host is a private/reserved IP that should be blocked
/// for outbound connections (SSRF protection).
fn is_private_ip(host: &str) -> bool {
    // Parse as IPv4 and check private ranges
    if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
        let octets = ip.octets();
        return octets[0] == 10 // 10.0.0.0/8
            || (octets[0] == 192 && octets[1] == 168) // 192.168.0.0/16
            || (octets[0] == 169 && octets[1] == 254) // 169.254.0.0/16 (link-local)
            || ip.is_unspecified(); // 0.0.0.0
    }
    false
}

/// Validate that an outbound URL is safe to connect to.
///
/// Rules:
/// - Localhost addresses (127.0.0.1, localhost, ::1) are allowed with any scheme
/// - Non-localhost URLs must use HTTPS
/// - Private IPs (10.x, 192.168.x, 169.254.x, 0.0.0.0) are blocked
pub fn validate_outbound_url(url: &str) -> Result<(), A2aError> {
    let parsed = Url::parse(url)
        .map_err(|e| A2aError::Security(format!("Invalid URL: {e}")))?;

    let host = parsed
        .host_str()
        .ok_or_else(|| A2aError::Security("URL has no host".into()))?;

    // Localhost is always allowed (any scheme)
    if is_localhost(host) {
        return Ok(());
    }

    // Non-localhost must be HTTPS
    if parsed.scheme() != "https" {
        return Err(A2aError::Security(format!(
            "Non-localhost URL must use HTTPS, got {}://",
            parsed.scheme()
        )));
    }

    // Block private IPs (SSRF protection)
    if is_private_ip(host) {
        return Err(A2aError::Security(format!(
            "Outbound connections to private IP {host} are blocked"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_api_key_success() {
        let result = validate_api_key(Some("correct-key"), "correct-key");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_api_key_mismatch() {
        let result = validate_api_key(Some("wrong-key"), "correct-key");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_api_key_missing() {
        let result = validate_api_key(None, "correct-key");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_api_key_error_messages() {
        let mismatch = validate_api_key(Some("wrong"), "right").unwrap_err();
        assert!(mismatch.to_string().contains("Invalid API key"));

        let missing = validate_api_key(None, "right").unwrap_err();
        assert!(missing.to_string().contains("Missing X-Hive-Key header"));
    }

    #[test]
    fn test_validate_no_key_configured_allows_all() {
        assert!(validate_api_key_optional(Some("anything"), None).is_ok());
        assert!(validate_api_key_optional(None, None).is_ok());
    }

    #[test]
    fn test_validate_key_configured_delegates() {
        assert!(validate_api_key_optional(Some("correct"), Some("correct")).is_ok());
        assert!(validate_api_key_optional(Some("wrong"), Some("correct")).is_err());
        assert!(validate_api_key_optional(None, Some("correct")).is_err());
    }

    #[test]
    fn test_outbound_url_https_required() {
        assert!(validate_outbound_url("https://agent.example.com").is_ok());
        assert!(validate_outbound_url("http://agent.example.com").is_err());
    }

    #[test]
    fn test_outbound_url_localhost_allowed() {
        assert!(validate_outbound_url("http://localhost:7420").is_ok());
        assert!(validate_outbound_url("http://127.0.0.1:8080").is_ok());
    }

    #[test]
    fn test_outbound_url_ipv6_localhost_allowed() {
        assert!(validate_outbound_url("http://[::1]:7420").is_ok());
    }

    #[test]
    fn test_outbound_url_private_ips_blocked() {
        assert!(validate_outbound_url("https://10.0.0.1").is_err());
        assert!(validate_outbound_url("https://192.168.1.1").is_err());
        assert!(validate_outbound_url("https://169.254.1.1").is_err());
    }

    #[test]
    fn test_outbound_url_zero_blocked() {
        assert!(validate_outbound_url("https://0.0.0.0").is_err());
    }

    #[test]
    fn test_outbound_url_invalid() {
        assert!(validate_outbound_url("not-a-url").is_err());
    }

    #[test]
    fn test_outbound_url_https_public_ok() {
        assert!(validate_outbound_url("https://api.openai.com/v1/agents").is_ok());
        assert!(validate_outbound_url("https://remote-agent.fly.dev/.well-known/agent.json").is_ok());
    }
}
