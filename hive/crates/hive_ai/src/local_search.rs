//! Private local search via SearXNG — a self-hosted, privacy-respecting
//! metasearch engine.
//!
//! `LocalSearchService` can either connect to an already-running SearXNG
//! instance or manage a Docker container automatically.

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single web search result returned by SearXNG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub engine: String,
}

/// Search categories supported by SearXNG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchCategory {
    General,
    Images,
    Videos,
    News,
    Science,
    It,
    Files,
}

impl std::fmt::Display for SearchCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::General => write!(f, "general"),
            Self::Images => write!(f, "images"),
            Self::Videos => write!(f, "videos"),
            Self::News => write!(f, "news"),
            Self::Science => write!(f, "science"),
            Self::It => write!(f, "it"),
            Self::Files => write!(f, "files"),
        }
    }
}

/// Configuration for the local search service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalSearchConfig {
    /// SearXNG base URL (default: `http://localhost:8888`).
    pub searxng_url: String,
    /// If true, Hive manages the SearXNG Docker container lifecycle.
    pub docker_managed: bool,
    /// Docker image to use for managed SearXNG (default: `searxng/searxng:latest`).
    pub docker_image: String,
    /// Host port to bind SearXNG to (default: 8888).
    pub host_port: u16,
    /// Maximum results per search (default: 10).
    pub max_results: usize,
    /// Request timeout in seconds (default: 15).
    pub timeout_secs: u64,
}

impl Default for LocalSearchConfig {
    fn default() -> Self {
        Self {
            searxng_url: "http://localhost:8888".into(),
            docker_managed: true,
            docker_image: "searxng/searxng:latest".into(),
            host_port: 8888,
            max_results: 10,
            timeout_secs: 15,
        }
    }
}

// ---------------------------------------------------------------------------
// SearXNG JSON API response types (internal)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SearXngResponse {
    results: Vec<SearXngResult>,
}

#[derive(Debug, Deserialize)]
struct SearXngResult {
    title: String,
    url: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    engine: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Percent-encode a query string for URL inclusion.
fn url_encode(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            _ => {
                encoded.push('%');
                encoded.push_str(&format!("{byte:02X}"));
            }
        }
    }
    encoded
}

/// Make a simple HTTP GET request using `curl` and return the response body.
fn http_get(url: &str, timeout_secs: u64) -> Result<String, String> {
    let output = std::process::Command::new("curl")
        .args([
            "-s",
            "-S",
            "--max-time",
            &timeout_secs.to_string(),
            "-H",
            "Accept: application/json",
            url,
        ])
        .output()
        .map_err(|e| format!("Failed to execute curl: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("curl failed: {stderr}"));
    }

    String::from_utf8(output.stdout).map_err(|e| format!("Invalid UTF-8 in response: {e}"))
}

/// Check if a URL is reachable (returns 2xx).
fn is_url_reachable(url: &str) -> bool {
    let output = std::process::Command::new("curl")
        .args([
            "-s",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "--max-time",
            "3",
            url,
        ])
        .output();

    match output {
        Ok(out) => {
            let code = String::from_utf8_lossy(&out.stdout);
            code.starts_with('2')
        }
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// LocalSearchService
// ---------------------------------------------------------------------------

/// A privacy-respecting web search service backed by SearXNG.
///
/// Supports two modes:
/// - **Docker-managed**: Hive starts/stops a SearXNG container automatically.
/// - **External**: Connect to a user-managed SearXNG instance.
pub struct LocalSearchService {
    config: LocalSearchConfig,
    container_id: Option<String>,
}

impl LocalSearchService {
    /// Create a new service with the given configuration.
    pub fn new(config: LocalSearchConfig) -> Self {
        Self {
            config,
            container_id: None,
        }
    }

    /// Create a service with default configuration.
    pub fn default_service() -> Self {
        Self::new(LocalSearchConfig::default())
    }

    /// Check if the SearXNG instance is reachable.
    pub fn is_available(&self) -> bool {
        // Try /healthz first, then root page.
        let healthz = format!("{}/healthz", self.config.searxng_url);
        if is_url_reachable(&healthz) {
            return true;
        }
        is_url_reachable(&self.config.searxng_url)
    }

    /// Ensure SearXNG is running, starting a Docker container if needed.
    pub fn ensure_running(&mut self) -> Result<(), String> {
        if self.is_available() {
            debug!("SearXNG already available at {}", self.config.searxng_url);
            return Ok(());
        }

        if !self.config.docker_managed {
            return Err(format!(
                "SearXNG not available at {} and docker_managed is false",
                self.config.searxng_url
            ));
        }

        debug!(
            image = %self.config.docker_image,
            port = self.config.host_port,
            "starting SearXNG container"
        );

        let port_mapping = format!("{}:8080", self.config.host_port);
        let output = std::process::Command::new("docker")
            .args([
                "run",
                "-d",
                "--name",
                "hive-searxng",
                "-p",
                &port_mapping,
                &self.config.docker_image,
            ])
            .output()
            .map_err(|e| format!("Failed to start SearXNG container: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("already in use") {
                let start = std::process::Command::new("docker")
                    .args(["start", "hive-searxng"])
                    .output()
                    .map_err(|e| format!("Failed to start existing SearXNG container: {e}"))?;
                if !start.status.success() {
                    return Err(format!(
                        "Failed to start SearXNG: {}",
                        String::from_utf8_lossy(&start.stderr)
                    ));
                }
                self.container_id = Some("hive-searxng".into());
            } else {
                return Err(format!("Failed to start SearXNG: {stderr}"));
            }
        } else {
            let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
            self.container_id = Some(id);
        }

        // Wait for SearXNG to become ready.
        for _ in 0..10 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if self.is_available() {
                debug!("SearXNG is ready");
                return Ok(());
            }
        }

        warn!("SearXNG container started but not yet responding — may need more time");
        Ok(())
    }

    /// Perform a web search.
    pub fn search(
        &self,
        query: &str,
        categories: &[SearchCategory],
    ) -> Result<Vec<WebSearchResult>, String> {
        let cats = if categories.is_empty() {
            "general".to_string()
        } else {
            categories
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(",")
        };

        let url = format!(
            "{}/search?q={}&format=json&categories={}",
            self.config.searxng_url,
            url_encode(query),
            cats,
        );

        let body = http_get(&url, self.config.timeout_secs)?;

        let response: SearXngResponse = serde_json::from_str(&body)
            .map_err(|e| format!("Failed to parse SearXNG response: {e}"))?;

        let results: Vec<WebSearchResult> = response
            .results
            .into_iter()
            .take(self.config.max_results)
            .map(|r| WebSearchResult {
                title: r.title,
                url: r.url,
                snippet: r.content,
                engine: r.engine,
            })
            .collect();

        debug!(query, count = results.len(), "search completed");
        Ok(results)
    }

    /// Stop the managed SearXNG container (if any).
    pub fn stop(&mut self) -> Result<(), String> {
        if let Some(ref id) = self.container_id {
            debug!(container = %id, "stopping SearXNG container");
            let _ = std::process::Command::new("docker")
                .args(["stop", id])
                .output();
            let _ = std::process::Command::new("docker")
                .args(["rm", id])
                .output();
            self.container_id = None;
        }
        Ok(())
    }

    /// Get the service configuration.
    pub fn config(&self) -> &LocalSearchConfig {
        &self.config
    }

    /// Whether this service manages its own Docker container.
    pub fn is_docker_managed(&self) -> bool {
        self.config.docker_managed
    }

    /// Whether the managed container is running (has a container ID).
    pub fn has_container(&self) -> bool {
        self.container_id.is_some()
    }
}

impl Drop for LocalSearchService {
    fn drop(&mut self) {
        if self.container_id.is_some() {
            if let Err(e) = self.stop() {
                warn!(error = %e, "failed to cleanup SearXNG on drop");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let config = LocalSearchConfig::default();
        assert_eq!(config.searxng_url, "http://localhost:8888");
        assert!(config.docker_managed);
        assert_eq!(config.docker_image, "searxng/searxng:latest");
        assert_eq!(config.host_port, 8888);
        assert_eq!(config.max_results, 10);
        assert_eq!(config.timeout_secs, 15);
    }

    #[test]
    fn config_serialization() {
        let config = LocalSearchConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let restored: LocalSearchConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.searxng_url, config.searxng_url);
        assert_eq!(restored.max_results, config.max_results);
    }

    #[test]
    fn search_category_display() {
        assert_eq!(SearchCategory::General.to_string(), "general");
        assert_eq!(SearchCategory::News.to_string(), "news");
        assert_eq!(SearchCategory::Science.to_string(), "science");
        assert_eq!(SearchCategory::It.to_string(), "it");
    }

    #[test]
    fn service_construction() {
        let svc = LocalSearchService::default_service();
        assert!(svc.is_docker_managed());
        assert!(!svc.has_container());
        assert_eq!(svc.config().host_port, 8888);
    }

    #[test]
    fn external_service_not_docker_managed() {
        let config = LocalSearchConfig {
            docker_managed: false,
            ..Default::default()
        };
        let svc = LocalSearchService::new(config);
        assert!(!svc.is_docker_managed());
    }

    #[test]
    fn web_search_result_serialization() {
        let result = WebSearchResult {
            title: "Test".into(),
            url: "https://example.com".into(),
            snippet: "A test result".into(),
            engine: "google".into(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let restored: WebSearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.title, "Test");
        assert_eq!(restored.url, "https://example.com");
    }

    #[test]
    fn url_encode_basic() {
        assert_eq!(url_encode("hello world"), "hello+world");
        assert_eq!(url_encode("rust lang"), "rust+lang");
        assert_eq!(url_encode("a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn url_encode_safe_chars() {
        assert_eq!(url_encode("hello-world_v1.0~beta"), "hello-world_v1.0~beta");
    }

    #[test]
    fn is_available_returns_false_when_not_running() {
        let config = LocalSearchConfig {
            searxng_url: "http://localhost:19999".into(),
            ..Default::default()
        };
        let svc = LocalSearchService::new(config);
        assert!(!svc.is_available());
    }

    #[test]
    fn ensure_running_fails_when_not_docker_managed() {
        let config = LocalSearchConfig {
            docker_managed: false,
            searxng_url: "http://localhost:19999".into(),
            ..Default::default()
        };
        let mut svc = LocalSearchService::new(config);
        assert!(svc.ensure_running().is_err());
    }

    #[test]
    fn stop_without_container_is_ok() {
        let mut svc = LocalSearchService::default_service();
        assert!(svc.stop().is_ok());
    }
}
