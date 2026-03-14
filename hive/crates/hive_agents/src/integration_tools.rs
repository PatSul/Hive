//! Integration tool definitions for the MCP server.
//!
//! Defines MCP tools that bridge to the integration hubs (messaging,
//! project management, knowledge bases, databases, Docker, Kubernetes,
//! cloud providers, etc.).
//!
//! Tool handlers start as stubs and are replaced with real implementations
//! via [`wire_integration_handlers`] once the integration services are
//! initialized as GPUI globals.

use async_trait::async_trait;
use crate::mcp_client::McpTool;
use crate::mcp_server::ToolHandler;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::runtime::Handle;

/// MCP-facing summary of a configured outbound A2A agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aAgentRecord {
    pub name: String,
    pub url: String,
    pub api_key_configured: bool,
    pub discovered: bool,
    pub card_name: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub skills: Vec<String>,
}

/// MCP-facing result of running a prompt against a remote A2A agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aTaskRecord {
    pub agent_name: String,
    pub url: String,
    pub task_id: String,
    pub state: String,
    pub skill_id: Option<String>,
    pub output: String,
}

/// Trait implemented by the app-owned outbound A2A service.
#[async_trait]
pub trait OutboundA2aService: Send + Sync {
    async fn list_agents(&self) -> Result<Vec<A2aAgentRecord>, String>;
    async fn discover_agent(&self, identifier: &str) -> Result<A2aAgentRecord, String>;
    async fn run_task(
        &self,
        identifier: &str,
        prompt: &str,
        skill_id: Option<&str>,
    ) -> Result<A2aTaskRecord, String>;
}

/// Return all integration tool definitions with default (stub) handlers.
///
/// These stubs are used at startup before integration services are ready.
/// Call [`wire_integration_handlers`] afterwards to replace them with real
/// implementations.
pub fn integration_tools() -> Vec<(McpTool, ToolHandler)> {
    vec![
        // --- Messaging ---
        (send_message_tool(), stub("Connect a messaging platform in Settings to send messages")),
        // --- Project Management ---
        (create_issue_tool(), stub("Connect your project management platform in Settings to create issues")),
        (list_issues_tool(), stub("Connect your project management platform in Settings to list issues")),
        // --- Knowledge Base ---
        (search_knowledge_tool(), stub("Connect a knowledge base in Settings to search content")),
        // --- Database ---
        (query_database_tool(), Box::new(|args| {
            let query = args["query"].as_str().unwrap_or("");
            if !query.trim_start().to_lowercase().starts_with("select") {
                return Err("Only SELECT queries are allowed for safety".into());
            }
            if query.contains(';') {
                return Err("Semicolons are not allowed in queries — multi-statement execution is blocked for safety".into());
            }
            Ok(json!({
                "connection": args["connection"].as_str().unwrap_or("default"),
                "query": query,
                "rows": [],
                "note": "Connect a database in Settings to run real queries"
            }))
        })),
        (describe_schema_tool(), stub("Connect a database in Settings to see real schema")),
        // --- Docker ---
        (docker_list_tool(), stub("Docker integration active — ensure Docker daemon is running")),
        (docker_logs_tool(), stub("Docker integration active — ensure Docker daemon is running")),
        // --- Kubernetes ---
        (k8s_pods_tool(), stub("Ensure kubeconfig is configured to see real pods")),
        // --- Cloud ---
        (cloud_resources_tool(), stub("Configure cloud credentials in Settings to see real resources")),
        // --- A2A ---
        (a2a_list_agents_tool(), stub("Configure remote A2A agents in ~/.hive/a2a.toml to list them here")),
        (a2a_discover_agent_tool(), stub("Configure remote A2A agents in ~/.hive/a2a.toml to discover their skills")),
        (a2a_run_task_tool(), stub("Configure remote A2A agents in ~/.hive/a2a.toml to run remote tasks")),
        // --- Browser ---
        (browse_url_tool(), stub("Browser automation available — content extraction pending connection")),
        (browser_navigate_tool(), stub("Browser automation available — requires Playwright installation")),
        (browser_screenshot_tool(), stub("Browser automation available — requires Playwright installation")),
        (browser_fill_form_tool(), stub("Browser automation available — requires Playwright installation")),
        (browser_click_tool(), stub("Browser automation available — requires Playwright installation")),
        (browser_evaluate_script_tool(), stub("Browser automation available — requires Playwright installation")),
        (browser_wait_for_selector_tool(), stub("Browser automation available — requires Playwright installation")),
        (browser_scrape_structured_tool(), stub("Browser automation available — requires Playwright installation")),
        (browser_pdf_export_tool(), stub("Browser automation available — requires Playwright installation")),
        (browser_run_test_tool(), stub("Browser automation available — requires Playwright installation")),
        (browser_crawl_site_tool(), stub("Browser automation available — requires Playwright installation")),
        (browser_monitor_changes_tool(), stub("Browser automation available — requires Playwright installation")),
        (browser_intercept_network_tool(), stub("Browser automation available — requires Playwright installation")),
        (browser_accessibility_audit_tool(), stub("Browser automation available — requires Playwright installation")),
        (browser_performance_metrics_tool(), stub("Browser automation available — requires Playwright installation")),
        // --- Local AI / Ollama ---
        (ollama_list_models_tool(), stub("Point Settings > Local AI at a running Ollama instance to list models")),
        (ollama_pull_model_tool(), stub("Point Settings > Local AI at a running Ollama instance to pull models")),
        (ollama_show_model_tool(), stub("Point Settings > Local AI at a running Ollama instance to inspect models")),
        (ollama_delete_model_tool(), stub("Point Settings > Local AI at a running Ollama instance to delete models")),
        // --- Smart Home / Hue ---
        (hue_discover_bridges_tool(), stub("Hue bridge discovery is available when local networking is reachable")),
        (hue_list_lights_tool(), stub("Configure a Hue bridge and API key in Settings to list lights")),
        (hue_set_light_state_tool(), stub("Configure a Hue bridge and API key in Settings to control lights")),
        (hue_list_scenes_tool(), stub("Configure a Hue bridge and API key in Settings to list scenes")),
        (hue_activate_scene_tool(), stub("Configure a Hue bridge and API key in Settings to activate scenes")),
        // --- Docs Search ---
        (search_docs_tool(), stub("Run /index-docs to build the documentation index first")),
        // --- Deploy ---
        (deploy_trigger_tool(), stub("Configure deployment workflows in Settings")),
    ]
}

/// Replace stub handlers with real implementations backed by service `Arc`s.
///
/// Each handler bridges the sync MCP tool handler signature to the async
/// service methods using a blocking spawn on the tokio runtime.
pub fn wire_integration_handlers(services: IntegrationServices) -> Vec<(McpTool, ToolHandler)> {
    let mut tools = Vec::new();

    // --- Messaging ---
    {
        let svc = Arc::clone(&services.messaging);
        tools.push((send_message_tool(), Box::new(move |args: serde_json::Value| {
            let platform = args["platform"].as_str().unwrap_or("slack").to_string();
            let channel = args["channel"].as_str().unwrap_or("").to_string();
            let message = args["message"].as_str().unwrap_or("").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let plat = parse_messaging_platform(&platform);
                match svc.send_message(plat, &channel, &message).await {
                    Ok(sent) => Ok(json!({
                        "status": "sent",
                        "platform": platform,
                        "channel": channel,
                        "timestamp": sent.timestamp
                    })),
                    Err(e) => Err(format!("Failed to send message: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // --- Project Management ---
    {
        let svc = Arc::clone(&services.project_management);
        tools.push((create_issue_tool(), Box::new(move |args: serde_json::Value| {
            let platform = args["platform"].as_str().unwrap_or("jira").to_string();
            let project = args["project"].as_str().unwrap_or("").to_string();
            let title = args["title"].as_str().unwrap_or("").to_string();
            let description = args["description"].as_str().unwrap_or("").to_string();
            let priority = args["priority"].as_str().unwrap_or("medium").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let plat = parse_pm_platform(&platform);
                let request = hive_integrations::project_management::CreateIssueRequest {
                    project_id: project,
                    title,
                    description: if description.is_empty() { None } else { Some(description) },
                    priority: Some(parse_priority(&priority)),
                    assignee: None,
                    labels: vec![],
                };
                match svc.create_issue(plat, &request).await {
                    Ok(issue) => Ok(json!({
                        "status": "created",
                        "platform": platform,
                        "id": issue.id,
                        "key": issue.key,
                        "title": issue.title,
                        "url": issue.url
                    })),
                    Err(e) => Err(format!("Failed to create issue: {e}")),
                }
            })
        }) as ToolHandler));
    }

    {
        let svc = Arc::clone(&services.project_management);
        tools.push((list_issues_tool(), Box::new(move |args: serde_json::Value| {
            let platform = args["platform"].as_str().unwrap_or("jira").to_string();
            let project = args["project"].as_str().unwrap_or("").to_string();
            let status_filter = args["status"].as_str().unwrap_or("all").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let plat = parse_pm_platform(&platform);
                let filters = hive_integrations::project_management::IssueFilters {
                    status: parse_issue_status_filter(&status_filter),
                    ..Default::default()
                };
                match svc.list_issues(plat, &project, &filters).await {
                    Ok(issues) => {
                        let items: Vec<serde_json::Value> = issues.iter().map(|i| json!({
                            "id": i.id,
                            "key": i.key,
                            "title": i.title,
                            "status": format!("{:?}", i.status),
                            "priority": format!("{:?}", i.priority),
                            "url": i.url
                        })).collect();
                        Ok(json!({
                            "platform": platform,
                            "project": project,
                            "count": items.len(),
                            "issues": items
                        }))
                    }
                    Err(e) => Err(format!("Failed to list issues: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // --- Knowledge Base ---
    {
        let svc = Arc::clone(&services.knowledge);
        tools.push((search_knowledge_tool(), Box::new(move |args: serde_json::Value| {
            let query = args["query"].as_str().unwrap_or("").to_string();
            let platform = args["platform"].as_str().unwrap_or("all").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let results = if platform == "all" {
                    svc.search_all(&query, 20).await
                } else {
                    let plat = parse_kb_platform(&platform);
                    svc.search(plat, &query, 20).await.unwrap_or_default()
                };
                let items: Vec<serde_json::Value> = results.iter().map(|r| json!({
                    "title": r.title,
                    "snippet": r.snippet,
                    "url": r.url,
                    "score": r.relevance_score
                })).collect();
                Ok(json!({
                    "query": query,
                    "count": items.len(),
                    "results": items
                }))
            })
        }) as ToolHandler));
    }

    // --- Database ---
    {
        let svc = Arc::clone(&services.database);
        tools.push((query_database_tool(), Box::new(move |args: serde_json::Value| {
            let connection = args["connection"].as_str().unwrap_or("default").to_string();
            let query = args["query"].as_str().unwrap_or("").to_string();
            if !query.trim_start().to_lowercase().starts_with("select") {
                return Err("Only SELECT queries are allowed for safety".into());
            }
            if query.contains(';') {
                return Err("Semicolons are not allowed in queries — multi-statement execution is blocked for safety".into());
            }
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let provider = svc.get_provider(&connection)
                    .ok_or_else(|| format!("No database connection named '{connection}'. Use Settings to configure one."))?;
                match provider.execute_query(&query).await {
                    Ok(result) => Ok(json!({
                        "connection": connection,
                        "query": query,
                        "columns": result.columns,
                        "rows": result.rows,
                        "rows_affected": result.rows_affected,
                        "execution_time_ms": result.execution_time_ms
                    })),
                    Err(e) => Err(format!("Query failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    {
        let svc = Arc::clone(&services.database);
        tools.push((describe_schema_tool(), Box::new(move |args: serde_json::Value| {
            let connection = args["connection"].as_str().unwrap_or("default").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let provider = svc.get_provider(&connection)
                    .ok_or_else(|| format!("No database connection named '{connection}'. Use Settings to configure one."))?;
                match provider.list_tables("public").await {
                    Ok(tables) => {
                        let items: Vec<serde_json::Value> = tables.iter().map(|t| json!({
                            "name": t.name,
                            "schema": t.schema,
                            "row_count_estimate": t.row_count_estimate,
                            "size_bytes": t.size_bytes
                        })).collect();
                        Ok(json!({
                            "connection": connection,
                            "table_count": items.len(),
                            "tables": items
                        }))
                    }
                    Err(e) => Err(format!("Schema introspection failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // --- Docker ---
    {
        let svc = Arc::clone(&services.docker);
        tools.push((docker_list_tool(), Box::new(move |args: serde_json::Value| {
            let all = args["all"].as_bool().unwrap_or(false);
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.list_containers(all).await {
                    Ok(containers) => {
                        let items: Vec<serde_json::Value> = containers.iter().map(|c| json!({
                            "id": c.id,
                            "name": c.name,
                            "image": c.image,
                            "status": c.status,
                            "state": c.state,
                            "ports": c.ports
                        })).collect();
                        Ok(json!({
                            "count": items.len(),
                            "containers": items
                        }))
                    }
                    Err(e) => Err(format!("Docker list failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    {
        let svc = Arc::clone(&services.docker);
        tools.push((docker_logs_tool(), Box::new(move |args: serde_json::Value| {
            let container = args["container"].as_str().unwrap_or("").to_string();
            let tail = args["tail"].as_u64().map(|n| n as u32).unwrap_or(100);
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.container_logs(&container, Some(tail)).await {
                    Ok(logs) => Ok(json!({
                        "container": container,
                        "logs": logs
                    })),
                    Err(e) => Err(format!("Docker logs failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // --- Kubernetes ---
    {
        let svc = Arc::clone(&services.kubernetes);
        tools.push((k8s_pods_tool(), Box::new(move |args: serde_json::Value| {
            let namespace = args["namespace"].as_str().map(String::from);
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.list_pods(namespace.as_deref()).await {
                    Ok(pods) => {
                        let items: Vec<serde_json::Value> = pods.iter().map(|p| json!({
                            "name": p.name,
                            "namespace": p.namespace,
                            "status": p.status,
                            "node": p.node,
                            "restarts": p.restarts,
                            "age": p.age
                        })).collect();
                        Ok(json!({
                            "namespace": namespace.as_deref().unwrap_or("default"),
                            "count": items.len(),
                            "pods": items
                        }))
                    }
                    Err(e) => Err(format!("Kubernetes list pods failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // --- Cloud ---
    {
        let aws = Arc::clone(&services.aws);
        let azure = Arc::clone(&services.azure);
        let gcp = Arc::clone(&services.gcp);
        tools.push((cloud_resources_tool(), Box::new(move |args: serde_json::Value| {
            let provider = args["provider"].as_str().unwrap_or("aws").to_string();
            let resource_type = args["resource_type"].as_str().unwrap_or("").to_string();
            let aws = Arc::clone(&aws);
            let azure = Arc::clone(&azure);
            let gcp = Arc::clone(&gcp);
            block_on_async(async move {
                match provider.as_str() {
                    "aws" => list_aws_resources(&aws, &resource_type).await,
                    "azure" => list_azure_resources(&azure, &resource_type).await,
                    "gcp" => list_gcp_resources(&gcp, &resource_type).await,
                    _ => Err(format!("Unknown cloud provider: {provider}")),
                }
            })
        }) as ToolHandler));
    }

    // --- A2A ---
    {
        let svc = Arc::clone(&services.a2a);
        tools.push((a2a_list_agents_tool(), Box::new(move |_args: serde_json::Value| {
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let agents = svc.list_agents().await?;
                Ok(json!({
                    "count": agents.len(),
                    "agents": agents,
                }))
            })
        }) as ToolHandler));
    }

    {
        let svc = Arc::clone(&services.a2a);
        tools.push((a2a_discover_agent_tool(), Box::new(move |args: serde_json::Value| {
            let identifier = args["agent"].as_str().unwrap_or("").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let agent = svc.discover_agent(&identifier).await?;
                Ok(json!(agent))
            })
        }) as ToolHandler));
    }

    {
        let svc = Arc::clone(&services.a2a);
        tools.push((a2a_run_task_tool(), Box::new(move |args: serde_json::Value| {
            let identifier = args["agent"].as_str().unwrap_or("").to_string();
            let prompt = args["prompt"].as_str().unwrap_or("").to_string();
            let skill_id = args["skill_id"].as_str().map(ToOwned::to_owned);
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let result = svc.run_task(&identifier, &prompt, skill_id.as_deref()).await?;
                Ok(json!(result))
            })
        }) as ToolHandler));
    }

    // --- Browser ---
    {
        let svc = Arc::clone(&services.browser);
        tools.push((browse_url_tool(), Box::new(move |args: serde_json::Value| {
            let url = args["url"].as_str().unwrap_or("").to_string();
            // SSRF validation: only allow http/https and block private/local hosts
            let parsed = url::Url::parse(&url)
                .map_err(|e| format!("Invalid URL: {e}"))?;
            if parsed.scheme() != "https" && parsed.scheme() != "http" {
                return Err("Only http/https URLs are allowed".into());
            }
            if let Some(host) = parsed.host_str() {
                if is_private_or_local(host) {
                    return Err("Access to private/internal hosts is blocked".into());
                }
            }
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.get_page_content(&url).await {
                    Ok(content) => Ok(json!({
                        "url": url,
                        "title": content.title,
                        "content": content.text_content,
                        "links": content.links
                    })),
                    Err(e) => Err(format!("Browse failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // browser_navigate
    {
        let svc = Arc::clone(&services.browser);
        tools.push((browser_navigate_tool(), Box::new(move |args: serde_json::Value| {
            let url = args["url"].as_str().unwrap_or("").to_string();
            validate_url(&url)?;
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.navigate(&url).await {
                    Ok(info) => Ok(json!({
                        "url": info.url,
                        "title": info.title,
                        "status_code": info.status_code
                    })),
                    Err(e) => Err(format!("Navigate failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // browser_screenshot
    {
        let svc = Arc::clone(&services.browser);
        tools.push((browser_screenshot_tool(), Box::new(move |args: serde_json::Value| {
            let url = args["url"].as_str().unwrap_or("").to_string();
            validate_url(&url)?;
            let full_page = args["full_page"].as_bool().unwrap_or(false);
            let width = args["width"].as_u64().unwrap_or(1280) as u32;
            let height = args["height"].as_u64().unwrap_or(720) as u32;
            let selector = args["selector"].as_str().map(String::from);
            let format = args["format"].as_str().unwrap_or("png").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let options = hive_integrations::browser::ScreenshotOptions {
                    full_page,
                    width,
                    height,
                    selector,
                    format: format.clone(),
                };
                match svc.screenshot(&url, options).await {
                    Ok(bytes) => {
                        let b64 = encode_base64(&bytes);
                        Ok(json!({
                            "url": url,
                            "format": format,
                            "size_bytes": bytes.len(),
                            "data_base64": b64
                        }))
                    }
                    Err(e) => Err(format!("Screenshot failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // browser_fill_form
    {
        let svc = Arc::clone(&services.browser);
        tools.push((browser_fill_form_tool(), Box::new(move |args: serde_json::Value| {
            let url = args["url"].as_str().unwrap_or("").to_string();
            validate_url(&url)?;
            let fields_val = args["fields"].as_array().cloned().unwrap_or_default();
            let fields: Vec<hive_integrations::browser::FormField> = fields_val
                .iter()
                .map(|f| hive_integrations::browser::FormField {
                    selector: f["selector"].as_str().unwrap_or("").to_string(),
                    value: f["value"].as_str().unwrap_or("").to_string(),
                })
                .collect();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.fill_form(&url, fields).await {
                    Ok(result) => Ok(json!({
                        "success": result.success,
                        "submitted_url": result.submitted_url,
                        "response_status": result.response_status
                    })),
                    Err(e) => Err(format!("Fill form failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // browser_click
    {
        let svc = Arc::clone(&services.browser);
        tools.push((browser_click_tool(), Box::new(move |args: serde_json::Value| {
            let url = args["url"].as_str().unwrap_or("").to_string();
            validate_url(&url)?;
            let selector = args["selector"].as_str().unwrap_or("").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.click(&url, &selector).await {
                    Ok(()) => Ok(json!({
                        "url": url,
                        "selector": selector,
                        "status": "clicked"
                    })),
                    Err(e) => Err(format!("Click failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // browser_evaluate_script
    {
        let svc = Arc::clone(&services.browser);
        tools.push((browser_evaluate_script_tool(), Box::new(move |args: serde_json::Value| {
            let url = args["url"].as_str().unwrap_or("").to_string();
            validate_url(&url)?;
            let js_code = args["js_code"].as_str().unwrap_or("").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.evaluate_script(&url, &js_code).await {
                    Ok(value) => Ok(json!({
                        "url": url,
                        "result": value
                    })),
                    Err(e) => Err(format!("Script evaluation failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // browser_wait_for_selector
    {
        let svc = Arc::clone(&services.browser);
        tools.push((browser_wait_for_selector_tool(), Box::new(move |args: serde_json::Value| {
            let url = args["url"].as_str().unwrap_or("").to_string();
            validate_url(&url)?;
            let selector = args["selector"].as_str().unwrap_or("").to_string();
            let timeout_ms = args["timeout_ms"].as_u64().unwrap_or(30_000);
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.wait_for_selector(&url, &selector, timeout_ms).await {
                    Ok(found) => Ok(json!({
                        "url": url,
                        "selector": selector,
                        "found": found,
                        "timeout_ms": timeout_ms
                    })),
                    Err(e) => Err(format!("Wait for selector failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // browser_scrape_structured
    {
        let svc = Arc::clone(&services.browser);
        tools.push((browser_scrape_structured_tool(), Box::new(move |args: serde_json::Value| {
            let url = args["url"].as_str().unwrap_or("").to_string();
            validate_url(&url)?;
            let selectors_val = args["selectors"].as_array().cloned().unwrap_or_default();
            let selectors: Vec<hive_integrations::browser::ScrapeSelector> = selectors_val
                .iter()
                .map(|s| hive_integrations::browser::ScrapeSelector {
                    name: s["name"].as_str().unwrap_or("").to_string(),
                    css_selector: s["css_selector"].as_str().unwrap_or("").to_string(),
                    attribute: s["attribute"].as_str().map(String::from),
                })
                .collect();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.scrape_structured(&url, selectors).await {
                    Ok(data) => {
                        let result: serde_json::Value = serde_json::to_value(&data)
                            .unwrap_or_else(|_| json!({}));
                        Ok(json!({
                            "url": url,
                            "data": result
                        }))
                    }
                    Err(e) => Err(format!("Structured scrape failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // browser_pdf_export
    {
        let svc = Arc::clone(&services.browser);
        tools.push((browser_pdf_export_tool(), Box::new(move |args: serde_json::Value| {
            let url = args["url"].as_str().unwrap_or("").to_string();
            validate_url(&url)?;
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.pdf_export(&url).await {
                    Ok(bytes) => {
                        let b64 = encode_base64(&bytes);
                        Ok(json!({
                            "url": url,
                            "size_bytes": bytes.len(),
                            "data_base64": b64
                        }))
                    }
                    Err(e) => Err(format!("PDF export failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // browser_run_test
    {
        let svc = Arc::clone(&services.browser);
        tools.push((browser_run_test_tool(), Box::new(move |args: serde_json::Value| {
            let test_script = args["test_script"].as_str().unwrap_or("").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.run_test(&test_script).await {
                    Ok(result) => Ok(json!({
                        "passed": result.passed,
                        "failed": result.failed,
                        "duration_ms": result.duration_ms,
                        "output": result.output
                    })),
                    Err(e) => Err(format!("Test run failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // browser_crawl_site
    {
        let svc = Arc::clone(&services.browser);
        tools.push((browser_crawl_site_tool(), Box::new(move |args: serde_json::Value| {
            let base_url = args["base_url"].as_str().unwrap_or("").to_string();
            validate_url(&base_url)?;
            let max_pages = args["max_pages"].as_u64().unwrap_or(10) as usize;
            let extract_selector = args["extract_selector"].as_str().map(String::from);
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.crawl_site(&base_url, max_pages, extract_selector.as_deref()).await {
                    Ok(pages) => {
                        let items: Vec<serde_json::Value> = pages.iter().map(|p| json!({
                            "url": p.url,
                            "title": p.title,
                            "content": p.content,
                            "links": p.links,
                            "depth": p.depth
                        })).collect();
                        Ok(json!({
                            "base_url": base_url,
                            "pages_crawled": items.len(),
                            "pages": items
                        }))
                    }
                    Err(e) => Err(format!("Crawl failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // browser_monitor_changes — collect up to 5 change events with a 30s total timeout
    {
        let svc = Arc::clone(&services.browser);
        tools.push((browser_monitor_changes_tool(), Box::new(move |args: serde_json::Value| {
            let url = args["url"].as_str().unwrap_or("").to_string();
            validate_url(&url)?;
            let selector = args["selector"].as_str().unwrap_or("").to_string();
            let interval_secs = args["interval_secs"].as_u64().unwrap_or(5);
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let mut rx = svc.monitor_changes(&url, &selector, interval_secs).await
                    .map_err(|e| format!("Monitor setup failed: {e}"))?;
                let mut events: Vec<serde_json::Value> = Vec::new();
                let deadline = tokio::time::Instant::now()
                    + tokio::time::Duration::from_secs(30);
                let max_events = 5usize;
                while events.len() < max_events {
                    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                    if remaining.is_zero() {
                        break;
                    }
                    match tokio::time::timeout(remaining, rx.recv()).await {
                        Ok(Some(event)) => {
                            events.push(json!({
                                "timestamp": event.timestamp.to_rfc3339(),
                                "old_content": event.old_content,
                                "new_content": event.new_content,
                                "selector": event.selector
                            }));
                        }
                        Ok(None) => break, // channel closed
                        Err(_) => break,   // timeout
                    }
                }
                Ok(json!({
                    "url": url,
                    "selector": selector,
                    "changes_detected": events.len(),
                    "events": events
                }))
            })
        }) as ToolHandler));
    }

    // browser_intercept_network
    {
        let svc = Arc::clone(&services.browser);
        tools.push((browser_intercept_network_tool(), Box::new(move |args: serde_json::Value| {
            let url = args["url"].as_str().unwrap_or("").to_string();
            validate_url(&url)?;
            let url_pattern = args["url_pattern"].as_str().unwrap_or("").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.intercept_network(&url, &url_pattern).await {
                    Ok(requests) => {
                        let items: Vec<serde_json::Value> = requests.iter().map(|r| json!({
                            "url": r.url,
                            "method": r.method,
                            "status": r.status,
                            "content_type": r.content_type,
                            "body_size": r.body_size
                        })).collect();
                        Ok(json!({
                            "page_url": url,
                            "pattern": url_pattern,
                            "requests_captured": items.len(),
                            "requests": items
                        }))
                    }
                    Err(e) => Err(format!("Network intercept failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // browser_accessibility_audit
    {
        let svc = Arc::clone(&services.browser);
        tools.push((browser_accessibility_audit_tool(), Box::new(move |args: serde_json::Value| {
            let url = args["url"].as_str().unwrap_or("").to_string();
            validate_url(&url)?;
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.accessibility_audit(&url).await {
                    Ok(report) => {
                        let violations: Vec<serde_json::Value> = report.violations.iter().map(|v| json!({
                            "id": v.id,
                            "description": v.description,
                            "impact": v.impact,
                            "nodes": v.nodes
                        })).collect();
                        Ok(json!({
                            "url": url,
                            "violations_count": violations.len(),
                            "violations": violations,
                            "passes": report.passes,
                            "total": report.total
                        }))
                    }
                    Err(e) => Err(format!("Accessibility audit failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // browser_performance_metrics
    {
        let svc = Arc::clone(&services.browser);
        tools.push((browser_performance_metrics_tool(), Box::new(move |args: serde_json::Value| {
            let url = args["url"].as_str().unwrap_or("").to_string();
            validate_url(&url)?;
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                match svc.performance_metrics(&url).await {
                    Ok(metrics) => Ok(json!({
                        "url": url,
                        "first_contentful_paint_ms": metrics.first_contentful_paint_ms,
                        "largest_contentful_paint_ms": metrics.largest_contentful_paint_ms,
                        "time_to_interactive_ms": metrics.time_to_interactive_ms,
                        "total_blocking_time_ms": metrics.total_blocking_time_ms,
                        "cumulative_layout_shift": metrics.cumulative_layout_shift
                    })),
                    Err(e) => Err(format!("Performance metrics failed: {e}")),
                }
            })
        }) as ToolHandler));
    }

    // --- Local AI / Ollama ---
    {
        let svc = Arc::clone(&services.ollama);
        tools.push((ollama_list_models_tool(), Box::new(move |_args: serde_json::Value| {
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let models = svc.list_models().await?;
                Ok(json!({
                    "count": models.len(),
                    "models": models,
                }))
            })
        }) as ToolHandler));
    }

    {
        let svc = Arc::clone(&services.ollama);
        tools.push((ollama_pull_model_tool(), Box::new(move |args: serde_json::Value| {
            let model = args["model"].as_str().unwrap_or("").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let (tx, mut rx) = tokio::sync::mpsc::channel(64);
                let model_name = model.clone();
                let pull = tokio::spawn(async move { svc.pull_model(&model_name, tx).await });
                let mut progress = Vec::new();
                while let Some(update) = rx.recv().await {
                    progress.push(json!(update));
                }
                match pull.await.map_err(|e| format!("Ollama pull task panicked: {e}"))? {
                    Ok(()) => Ok(json!({
                        "status": "pulled",
                        "model": model,
                        "progress": progress,
                    })),
                    Err(e) => Err(e),
                }
            })
        }) as ToolHandler));
    }

    {
        let svc = Arc::clone(&services.ollama);
        tools.push((ollama_show_model_tool(), Box::new(move |args: serde_json::Value| {
            let model = args["model"].as_str().unwrap_or("").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                let info = svc.show_model(&model).await?;
                Ok(json!(info))
            })
        }) as ToolHandler));
    }

    {
        let svc = Arc::clone(&services.ollama);
        tools.push((ollama_delete_model_tool(), Box::new(move |args: serde_json::Value| {
            let model = args["model"].as_str().unwrap_or("").to_string();
            let svc = Arc::clone(&svc);
            block_on_async(async move {
                svc.delete_model(&model).await?;
                Ok(json!({
                    "status": "deleted",
                    "model": model,
                }))
            })
        }) as ToolHandler));
    }

    // --- Smart Home / Hue ---
    tools.push((hue_discover_bridges_tool(), Box::new(move |_args: serde_json::Value| {
        block_on_async(async move {
            let bridges = hive_integrations::smart_home::PhilipsHueClient::discover_bridges()
                .await
                .map_err(|e| format!("Hue discovery failed: {e}"))?;
            Ok(json!({
                "count": bridges.len(),
                "bridges": bridges,
            }))
        })
    }) as ToolHandler));

    {
        let svc = services.hue.clone();
        tools.push((hue_list_lights_tool(), Box::new(move |_args: serde_json::Value| {
            let svc = svc.clone();
            block_on_async(async move {
                let client = svc
                    .ok_or_else(|| "Hue bridge is not configured in Settings".to_string())?;
                let lights = client
                    .list_lights()
                    .await
                    .map_err(|e| format!("Hue lights request failed: {e}"))?;
                Ok(json!({
                    "count": lights.len(),
                    "lights": lights,
                }))
            })
        }) as ToolHandler));
    }

    {
        let svc = services.hue.clone();
        tools.push((hue_set_light_state_tool(), Box::new(move |args: serde_json::Value| {
            let light_id = args["light_id"].as_str().unwrap_or("").to_string();
            let on = args["on"].as_bool().unwrap_or(false);
            let brightness = args["brightness"].as_u64().map(|value| value as u8);
            let svc = svc.clone();
            block_on_async(async move {
                let client = svc
                    .ok_or_else(|| "Hue bridge is not configured in Settings".to_string())?;
                client
                    .set_light_state(&light_id, on, brightness)
                    .await
                    .map_err(|e| format!("Hue light state update failed: {e}"))?;
                Ok(json!({
                    "status": "updated",
                    "light_id": light_id,
                    "on": on,
                    "brightness": brightness,
                }))
            })
        }) as ToolHandler));
    }

    {
        let svc = services.hue.clone();
        tools.push((hue_list_scenes_tool(), Box::new(move |_args: serde_json::Value| {
            let svc = svc.clone();
            block_on_async(async move {
                let client = svc
                    .ok_or_else(|| "Hue bridge is not configured in Settings".to_string())?;
                let scenes = client
                    .list_scenes()
                    .await
                    .map_err(|e| format!("Hue scenes request failed: {e}"))?;
                Ok(json!({
                    "count": scenes.len(),
                    "scenes": scenes,
                }))
            })
        }) as ToolHandler));
    }

    {
        let svc = services.hue.clone();
        tools.push((hue_activate_scene_tool(), Box::new(move |args: serde_json::Value| {
            let scene_id = args["scene_id"].as_str().unwrap_or("").to_string();
            let svc = svc.clone();
            block_on_async(async move {
                let client = svc
                    .ok_or_else(|| "Hue bridge is not configured in Settings".to_string())?;
                client
                    .activate_scene(&scene_id)
                    .await
                    .map_err(|e| format!("Hue scene activation failed: {e}"))?;
                Ok(json!({
                    "status": "activated",
                    "scene_id": scene_id,
                }))
            })
        }) as ToolHandler));
    }

    // --- Docs Search ---
    {
        let svc = Arc::clone(&services.docs_indexer);
        tools.push((search_docs_tool(), Box::new(move |args: serde_json::Value| {
            let query = args["query"].as_str().unwrap_or("").to_string();
            let max_results = args["max_results"].as_u64().unwrap_or(10) as usize;
            let svc = Arc::clone(&svc);
            // search is sync
            let results = svc.search("default", &query, max_results);
            let items: Vec<serde_json::Value> = results.iter().map(|r| json!({
                "title": r.title,
                "url": r.page_url,
                "snippet": r.snippet,
                "score": r.relevance_score
            })).collect();
            Ok(json!({
                "query": query,
                "count": items.len(),
                "results": items
            }))
        }) as ToolHandler));
    }

    // --- Deploy (dispatches via deploy scripts, Makefile, or GitHub Actions) ---
    tools.push((deploy_trigger_tool(), Box::new(|args: serde_json::Value| {
        let environment = args["environment"].as_str().unwrap_or("staging").to_string();
        let branch = args["branch"].as_str().unwrap_or("main").to_string();

        // Validate inputs: only allow safe characters to prevent any injection.
        if !is_safe_deploy_param(&environment) {
            return Err("Invalid environment: only alphanumeric characters, '.', '_', and '-' are allowed".into());
        }
        if !is_safe_deploy_param(&branch) {
            return Err("Invalid branch: only alphanumeric characters, '.', '_', '-', and '/' are allowed".into());
        }

        // Try common deployment dispatch mechanisms in order of preference:
        // 1. deploy.sh in current directory
        // 2. Makefile "deploy" target
        // 3. GitHub Actions via `gh workflow run`
        //
        // Each mechanism is invoked directly without `sh -c` to avoid shell
        // injection. Environment variables and arguments are passed via the
        // safe `Command` builder API.

        if std::path::Path::new("deploy.sh").exists() {
            let deploy_result = std::process::Command::new("bash")
                .arg("deploy.sh")
                .env("DEPLOY_ENV", &environment)
                .env("DEPLOY_BRANCH", &branch)
                .output();
            return format_deploy_output(deploy_result, &environment, &branch);
        }

        if std::path::Path::new("Makefile").exists() {
            // Check if the Makefile has a deploy target (safe: no user input in this command).
            let has_deploy_target = std::process::Command::new("grep")
                .arg("-q")
                .arg("^deploy:")
                .arg("Makefile")
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if has_deploy_target {
                let deploy_result = std::process::Command::new("make")
                    .arg("deploy")
                    .env("DEPLOY_ENV", &environment)
                    .env("DEPLOY_BRANCH", &branch)
                    .output();
                return format_deploy_output(deploy_result, &environment, &branch);
            }
        }

        // Check if gh CLI is available (safe: no user input).
        let gh_available = std::process::Command::new("gh")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if gh_available {
            let deploy_result = std::process::Command::new("gh")
                .arg("workflow")
                .arg("run")
                .arg("deploy.yml")
                .arg("-f")
                .arg(format!("environment={}", environment))
                .arg("-f")
                .arg(format!("branch={}", branch))
                .output();
            return format_deploy_output(deploy_result, &environment, &branch);
        }

        Ok(json!({
            "status": "no_deploy_mechanism",
            "environment": environment,
            "branch": branch,
            "note": "No deploy.sh, Makefile deploy target, or gh CLI found. Add a deploy.sh to your project root or configure a GitHub Actions deploy workflow."
        }))
    }) as ToolHandler));

    tools
}

/// Services needed by integration tool handlers.
pub struct IntegrationServices {
    pub messaging: Arc<hive_integrations::messaging::MessagingHub>,
    pub project_management: Arc<hive_integrations::project_management::ProjectManagementHub>,
    pub knowledge: Arc<hive_integrations::knowledge::KnowledgeHub>,
    pub database: Arc<hive_integrations::database::DatabaseHub>,
    pub docker: Arc<hive_integrations::docker::DockerClient>,
    pub kubernetes: Arc<hive_integrations::kubernetes::KubernetesClient>,
    pub a2a: Arc<dyn OutboundA2aService>,
    pub browser: Arc<hive_integrations::browser::BrowserAutomation>,
    pub ollama: Arc<hive_terminal::local_ai::OllamaManager>,
    pub hue: Option<Arc<hive_integrations::smart_home::PhilipsHueClient>>,
    pub aws: Arc<hive_integrations::cloud::AwsClient>,
    pub azure: Arc<hive_integrations::cloud::AzureClient>,
    pub gcp: Arc<hive_integrations::cloud::GcpClient>,
    pub docs_indexer: Arc<hive_integrations::docs_indexer::DocsIndexer>,
}

// ---------------------------------------------------------------------------
// Tool definitions (shared between stubs and wired handlers)
// ---------------------------------------------------------------------------

fn send_message_tool() -> McpTool {
    McpTool {
        name: "send_message".into(),
        description: "Send a message via Slack, Discord, Teams, Telegram, Matrix, WebChat, WhatsApp, Signal, Google Chat, or iMessage".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "platform": { "type": "string", "enum": ["slack", "discord", "teams", "telegram", "matrix", "web_chat", "whatsapp", "signal", "google_chat", "imessage"] },
                "channel": { "type": "string", "description": "Channel name or ID" },
                "message": { "type": "string", "description": "Message content" }
            },
            "required": ["platform", "channel", "message"]
        }),
    }
}

fn create_issue_tool() -> McpTool {
    McpTool {
        name: "create_issue".into(),
        description: "Create an issue/ticket in Jira, Linear, or Asana".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "platform": { "type": "string", "enum": ["jira", "linear", "asana"] },
                "project": { "type": "string" },
                "title": { "type": "string" },
                "description": { "type": "string" },
                "priority": { "type": "string", "enum": ["low", "medium", "high", "critical"] }
            },
            "required": ["platform", "project", "title"]
        }),
    }
}

fn list_issues_tool() -> McpTool {
    McpTool {
        name: "list_issues".into(),
        description: "List issues/tickets from Jira, Linear, or Asana".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "platform": { "type": "string", "enum": ["jira", "linear", "asana"] },
                "project": { "type": "string" },
                "status": { "type": "string", "enum": ["open", "in_progress", "done", "all"] }
            },
            "required": ["platform", "project"]
        }),
    }
}

fn search_knowledge_tool() -> McpTool {
    McpTool {
        name: "search_knowledge".into(),
        description: "Search knowledge bases (Notion, Obsidian) for relevant information".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "platform": { "type": "string", "enum": ["notion", "obsidian", "all"] }
            },
            "required": ["query"]
        }),
    }
}

fn query_database_tool() -> McpTool {
    McpTool {
        name: "query_database".into(),
        description: "Run a read-only SQL query against a connected database".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "connection": { "type": "string", "description": "Database connection name" },
                "query": { "type": "string", "description": "SQL query (SELECT only)" }
            },
            "required": ["connection", "query"]
        }),
    }
}

fn describe_schema_tool() -> McpTool {
    McpTool {
        name: "describe_schema".into(),
        description: "Get the schema (tables, columns) of a connected database".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "connection": { "type": "string", "description": "Database connection name" }
            },
            "required": ["connection"]
        }),
    }
}

fn docker_list_tool() -> McpTool {
    McpTool {
        name: "docker_list".into(),
        description: "List Docker containers (running and stopped)".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "all": { "type": "boolean", "description": "Include stopped containers" }
            }
        }),
    }
}

fn docker_logs_tool() -> McpTool {
    McpTool {
        name: "docker_logs".into(),
        description: "Fetch logs from a Docker container".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "container": { "type": "string", "description": "Container name or ID" },
                "tail": { "type": "integer", "description": "Number of lines from the end (default 100)" }
            },
            "required": ["container"]
        }),
    }
}

fn k8s_pods_tool() -> McpTool {
    McpTool {
        name: "k8s_pods".into(),
        description: "List Kubernetes pods in a namespace".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "namespace": { "type": "string", "description": "Kubernetes namespace (default: default)" }
            }
        }),
    }
}

fn cloud_resources_tool() -> McpTool {
    McpTool {
        name: "cloud_resources".into(),
        description: "List cloud resources from AWS, Azure, or GCP".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "provider": { "type": "string", "enum": ["aws", "azure", "gcp"] },
                "resource_type": { "type": "string", "description": "Resource type (e.g. ec2, storage, functions)" }
            },
            "required": ["provider"]
        }),
    }
}

fn a2a_list_agents_tool() -> McpTool {
    McpTool {
        name: "a2a_list_agents".into(),
        description: "List configured outbound A2A agents from ~/.hive/a2a.toml".into(),
        input_schema: json!({
            "type": "object",
            "properties": {}
        }),
    }
}

fn a2a_discover_agent_tool() -> McpTool {
    McpTool {
        name: "a2a_discover_agent".into(),
        description: "Discover a configured remote A2A agent and return its advertised skills".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "agent": { "type": "string", "description": "Configured agent name or URL" }
            },
            "required": ["agent"]
        }),
    }
}

fn a2a_run_task_tool() -> McpTool {
    McpTool {
        name: "a2a_run_task".into(),
        description: "Run a text prompt against a configured remote A2A agent".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "agent": { "type": "string", "description": "Configured agent name or URL" },
                "prompt": { "type": "string", "description": "Prompt to send" },
                "skill_id": { "type": "string", "description": "Optional remote skill id" }
            },
            "required": ["agent", "prompt"]
        }),
    }
}

fn browse_url_tool() -> McpTool {
    McpTool {
        name: "browse_url".into(),
        description: "Fetch and extract content from a URL".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to browse" },
                "selector": { "type": "string", "description": "Optional CSS selector to extract specific content" }
            },
            "required": ["url"]
        }),
    }
}

fn browser_navigate_tool() -> McpTool {
    McpTool {
        name: "browser_navigate".into(),
        description: "Navigate to a URL and return page info (url, title, status code)".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to navigate to" }
            },
            "required": ["url"]
        }),
    }
}

fn browser_screenshot_tool() -> McpTool {
    McpTool {
        name: "browser_screenshot".into(),
        description: "Take a screenshot of a URL. Returns base64-encoded image bytes.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to screenshot" },
                "full_page": { "type": "boolean", "description": "Capture the full scrollable page (default false)" },
                "width": { "type": "integer", "description": "Viewport width in pixels (default 1280)" },
                "height": { "type": "integer", "description": "Viewport height in pixels (default 720)" },
                "selector": { "type": "string", "description": "Optional CSS selector to screenshot a specific element" },
                "format": { "type": "string", "enum": ["png", "jpeg"], "description": "Image format (default png)" }
            },
            "required": ["url"]
        }),
    }
}

fn browser_fill_form_tool() -> McpTool {
    McpTool {
        name: "browser_fill_form".into(),
        description: "Fill form fields on a page and submit. Each field needs a CSS selector and value.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL of the page containing the form" },
                "fields": {
                    "type": "array",
                    "description": "Form fields to fill",
                    "items": {
                        "type": "object",
                        "properties": {
                            "selector": { "type": "string", "description": "CSS selector for the field" },
                            "value": { "type": "string", "description": "Value to fill" }
                        },
                        "required": ["selector", "value"]
                    }
                }
            },
            "required": ["url", "fields"]
        }),
    }
}

fn browser_click_tool() -> McpTool {
    McpTool {
        name: "browser_click".into(),
        description: "Click an element on a page identified by a CSS selector".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL of the page" },
                "selector": { "type": "string", "description": "CSS selector of the element to click" }
            },
            "required": ["url", "selector"]
        }),
    }
}

fn browser_evaluate_script_tool() -> McpTool {
    McpTool {
        name: "browser_evaluate_script".into(),
        description: "Evaluate JavaScript in the page context. WARNING: The script runs with full page access and can read/modify page data, cookies, and DOM.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL of the page to evaluate script on" },
                "js_code": { "type": "string", "description": "JavaScript code to evaluate in the page context" }
            },
            "required": ["url", "js_code"]
        }),
    }
}

fn browser_wait_for_selector_tool() -> McpTool {
    McpTool {
        name: "browser_wait_for_selector".into(),
        description: "Wait for a CSS selector to appear on a page within a timeout".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL of the page" },
                "selector": { "type": "string", "description": "CSS selector to wait for" },
                "timeout_ms": { "type": "integer", "description": "Maximum time to wait in milliseconds (default 30000)" }
            },
            "required": ["url", "selector"]
        }),
    }
}

fn browser_scrape_structured_tool() -> McpTool {
    McpTool {
        name: "browser_scrape_structured".into(),
        description: "Scrape structured data from a page using named CSS selectors with optional attribute extraction".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL of the page to scrape" },
                "selectors": {
                    "type": "array",
                    "description": "Named selectors to extract data from",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "Name for this data field" },
                            "css_selector": { "type": "string", "description": "CSS selector to match elements" },
                            "attribute": { "type": "string", "description": "Optional attribute to extract (e.g. 'href', 'src'). Omit to get text content." }
                        },
                        "required": ["name", "css_selector"]
                    }
                }
            },
            "required": ["url", "selectors"]
        }),
    }
}

fn browser_pdf_export_tool() -> McpTool {
    McpTool {
        name: "browser_pdf_export".into(),
        description: "Export a page as PDF. Returns base64-encoded PDF bytes.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL of the page to export as PDF" }
            },
            "required": ["url"]
        }),
    }
}

fn browser_run_test_tool() -> McpTool {
    McpTool {
        name: "browser_run_test".into(),
        description: "Run a Playwright test script and return results (passed, failed, duration, output)".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "test_script": { "type": "string", "description": "Playwright test script content to execute" }
            },
            "required": ["test_script"]
        }),
    }
}

fn browser_crawl_site_tool() -> McpTool {
    McpTool {
        name: "browser_crawl_site".into(),
        description: "Crawl a website starting from a base URL, visiting up to max_pages pages and extracting content".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "base_url": { "type": "string", "description": "Starting URL for the crawl" },
                "max_pages": { "type": "integer", "description": "Maximum number of pages to visit (default 10)" },
                "extract_selector": { "type": "string", "description": "Optional CSS selector to extract specific content from each page" }
            },
            "required": ["base_url"]
        }),
    }
}

fn browser_monitor_changes_tool() -> McpTool {
    McpTool {
        name: "browser_monitor_changes".into(),
        description: "Monitor a page element for content changes. Checks up to 5 times at the given interval and returns any detected changes.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL of the page to monitor" },
                "selector": { "type": "string", "description": "CSS selector of the element to watch" },
                "interval_secs": { "type": "integer", "description": "Seconds between checks (default 5)" }
            },
            "required": ["url", "selector"]
        }),
    }
}

fn browser_intercept_network_tool() -> McpTool {
    McpTool {
        name: "browser_intercept_network".into(),
        description: "Intercept and capture network requests matching a URL pattern during page load".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL of the page to load" },
                "url_pattern": { "type": "string", "description": "URL substring pattern to match requests against" }
            },
            "required": ["url", "url_pattern"]
        }),
    }
}

fn browser_accessibility_audit_tool() -> McpTool {
    McpTool {
        name: "browser_accessibility_audit".into(),
        description: "Run an accessibility audit on a page and return violations, passes, and total checks".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL of the page to audit" }
            },
            "required": ["url"]
        }),
    }
}

fn browser_performance_metrics_tool() -> McpTool {
    McpTool {
        name: "browser_performance_metrics".into(),
        description: "Collect Core Web Vitals and performance metrics for a page (FCP, LCP, TTI, TBT, CLS)".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL of the page to measure" }
            },
            "required": ["url"]
        }),
    }
}

fn ollama_list_models_tool() -> McpTool {
    McpTool {
        name: "ollama_list_models".into(),
        description: "List models available on the configured Ollama endpoint".into(),
        input_schema: json!({
            "type": "object",
            "properties": {}
        }),
    }
}

fn ollama_pull_model_tool() -> McpTool {
    McpTool {
        name: "ollama_pull_model".into(),
        description: "Pull a model onto the configured Ollama endpoint".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "model": { "type": "string", "description": "Model name, e.g. llama3.2:latest" }
            },
            "required": ["model"]
        }),
    }
}

fn ollama_show_model_tool() -> McpTool {
    McpTool {
        name: "ollama_show_model".into(),
        description: "Show metadata for a model on the configured Ollama endpoint".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "model": { "type": "string", "description": "Model name" }
            },
            "required": ["model"]
        }),
    }
}

fn ollama_delete_model_tool() -> McpTool {
    McpTool {
        name: "ollama_delete_model".into(),
        description: "Delete a model from the configured Ollama endpoint".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "model": { "type": "string", "description": "Model name" }
            },
            "required": ["model"]
        }),
    }
}

fn hue_discover_bridges_tool() -> McpTool {
    McpTool {
        name: "hue_discover_bridges".into(),
        description: "Discover Philips Hue bridges on the local network".into(),
        input_schema: json!({
            "type": "object",
            "properties": {}
        }),
    }
}

fn hue_list_lights_tool() -> McpTool {
    McpTool {
        name: "hue_list_lights".into(),
        description: "List lights from the configured Philips Hue bridge".into(),
        input_schema: json!({
            "type": "object",
            "properties": {}
        }),
    }
}

fn hue_set_light_state_tool() -> McpTool {
    McpTool {
        name: "hue_set_light_state".into(),
        description: "Set on/off and optional brightness for a Hue light".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "light_id": { "type": "string", "description": "Hue light id" },
                "on": { "type": "boolean", "description": "Desired on/off state" },
                "brightness": { "type": "integer", "description": "Optional Hue brightness value (1-254)" }
            },
            "required": ["light_id", "on"]
        }),
    }
}

fn hue_list_scenes_tool() -> McpTool {
    McpTool {
        name: "hue_list_scenes".into(),
        description: "List scenes from the configured Philips Hue bridge".into(),
        input_schema: json!({
            "type": "object",
            "properties": {}
        }),
    }
}

fn hue_activate_scene_tool() -> McpTool {
    McpTool {
        name: "hue_activate_scene".into(),
        description: "Activate a scene on the configured Philips Hue bridge".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "scene_id": { "type": "string", "description": "Hue scene id" }
            },
            "required": ["scene_id"]
        }),
    }
}

fn search_docs_tool() -> McpTool {
    McpTool {
        name: "search_docs".into(),
        description: "Search indexed documentation for the current project".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "max_results": { "type": "integer", "description": "Maximum results to return" }
            },
            "required": ["query"]
        }),
    }
}

fn deploy_trigger_tool() -> McpTool {
    McpTool {
        name: "deploy_trigger".into(),
        description: "Trigger a deployment workflow to a target environment".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "environment": { "type": "string", "enum": ["staging", "production", "development"] },
                "branch": { "type": "string", "description": "Branch or tag to deploy (default: main)" }
            },
            "required": ["environment"]
        }),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Validate that a deploy parameter contains only safe characters.
///
/// Allows alphanumeric characters, '.', '_', '-', and '/' (for branch names
/// like `feature/foo`). Rejects empty strings and anything else.
fn is_safe_deploy_param(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
}

/// Format the output of a deploy command into a JSON result.
fn format_deploy_output(
    result: std::io::Result<std::process::Output>,
    environment: &str,
    branch: &str,
) -> Result<serde_json::Value, String> {
    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

            if output.status.success() {
                Ok(json!({
                    "status": "triggered",
                    "environment": environment,
                    "branch": branch,
                    "output": if stdout.len() > 500 { stdout[..500].to_string() } else { stdout },
                    "note": "Deployment initiated successfully."
                }))
            } else {
                Ok(json!({
                    "status": "failed",
                    "environment": environment,
                    "branch": branch,
                    "error": if stderr.is_empty() { stdout } else { stderr },
                    "exit_code": output.status.code()
                }))
            }
        }
        Err(e) => Err(format!("Failed to execute deploy command: {e}")),
    }
}

/// Create a stub handler that returns a note.
fn stub(note: &'static str) -> ToolHandler {
    Box::new(move |_args| {
        Ok(json!({ "note": note }))
    })
}

/// Bridge sync handler to async by running on the tokio runtime.
fn block_on_async<F, T>(future: F) -> Result<T, String>
where
    F: std::future::Future<Output = Result<T, String>> + Send + 'static,
    T: Send + 'static,
{
    if Handle::try_current().is_ok() {
        // Already inside a tokio runtime — spawn on a separate thread
        // to avoid blocking the runtime.
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| format!("Failed to create runtime: {e}"))?;
            rt.block_on(future)
        })
        .join()
        .map_err(|_| "Async task panicked".to_string())?
    } else {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| format!("Failed to create runtime: {e}"))?;
        rt.block_on(future)
    }
}

/// Returns `true` if the host is a private, loopback, or link-local address (SSRF protection).
fn is_private_or_local(host: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']');

    if matches!(h, "localhost" | "127.0.0.1" | "::1") {
        return true;
    }
    if h.ends_with(".local") {
        return true;
    }
    if h == "169.254.169.254" {
        return true;
    }

    if let Ok(ip) = h.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v4) => {
                let o = v4.octets();
                o[0] == 10
                    || (o[0] == 172 && (16..=31).contains(&o[1]))
                    || (o[0] == 192 && o[1] == 168)
                    || (o[0] == 169 && o[1] == 254)
                    || v4.is_unspecified()
            }
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback()
                    || (v6.segments()[0] & 0xfe00) == 0xfc00
                    || (v6.segments()[0] & 0xffc0) == 0xfe80
            }
        };
    }

    false
}

/// Validate that a URL is safe for browser automation (SSRF protection).
///
/// Allows only `http` and `https` schemes and blocks private/local hosts.
fn validate_url(url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err("Only http/https URLs are allowed".into());
    }
    if let Some(host) = parsed.host_str() {
        if is_private_or_local(host) {
            return Err("Access to private/internal hosts is blocked".into());
        }
    }
    Ok(())
}

/// Encode bytes as a standard base64 string (RFC 4648).
fn encode_base64(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;

        out.push(ALPHABET[((n >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((n >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(n & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn parse_messaging_platform(s: &str) -> hive_integrations::messaging::Platform {
    match s {
        "discord" => hive_integrations::messaging::Platform::Discord,
        "teams" => hive_integrations::messaging::Platform::Teams,
        _ => hive_integrations::messaging::Platform::Slack,
    }
}

fn parse_pm_platform(s: &str) -> hive_integrations::project_management::PMPlatform {
    match s {
        "linear" => hive_integrations::project_management::PMPlatform::Linear,
        "asana" => hive_integrations::project_management::PMPlatform::Asana,
        _ => hive_integrations::project_management::PMPlatform::Jira,
    }
}

fn parse_priority(s: &str) -> hive_integrations::project_management::IssuePriority {
    match s {
        "low" => hive_integrations::project_management::IssuePriority::Low,
        "high" => hive_integrations::project_management::IssuePriority::High,
        "critical" => hive_integrations::project_management::IssuePriority::Critical,
        _ => hive_integrations::project_management::IssuePriority::Medium,
    }
}

fn parse_issue_status_filter(s: &str) -> Option<hive_integrations::project_management::IssueStatus> {
    match s {
        "open" => Some(hive_integrations::project_management::IssueStatus::Todo),
        "in_progress" => Some(hive_integrations::project_management::IssueStatus::InProgress),
        "done" => Some(hive_integrations::project_management::IssueStatus::Done),
        _ => None, // "all"
    }
}

fn parse_kb_platform(s: &str) -> hive_integrations::knowledge::KBPlatform {
    match s {
        "obsidian" => hive_integrations::knowledge::KBPlatform::Obsidian,
        _ => hive_integrations::knowledge::KBPlatform::Notion,
    }
}

async fn list_aws_resources(
    aws: &hive_integrations::cloud::AwsClient,
    resource_type: &str,
) -> Result<serde_json::Value, String> {
    match resource_type {
        "s3" | "storage" => {
            let buckets = aws.list_s3_buckets().await.map_err(|e| format!("AWS error: {e}"))?;
            let items: Vec<serde_json::Value> = buckets.iter().map(|b| json!({
                "name": b.name, "region": b.region, "created": b.creation_date
            })).collect();
            Ok(json!({ "provider": "aws", "resource_type": "s3", "count": items.len(), "resources": items }))
        }
        "lambda" | "functions" => {
            let fns = aws.list_lambda_functions().await.map_err(|e| format!("AWS error: {e}"))?;
            let items: Vec<serde_json::Value> = fns.iter().map(|f| json!({
                "name": f.name, "runtime": f.runtime, "memory_mb": f.memory_mb
            })).collect();
            Ok(json!({ "provider": "aws", "resource_type": "lambda", "count": items.len(), "resources": items }))
        }
        _ => {
            // Default: list EC2 instances
            let instances = aws.list_ec2_instances().await.map_err(|e| format!("AWS error: {e}"))?;
            let items: Vec<serde_json::Value> = instances.iter().map(|i| json!({
                "id": i.id, "name": i.name, "state": i.state, "instance_type": i.instance_type
            })).collect();
            Ok(json!({ "provider": "aws", "resource_type": "ec2", "count": items.len(), "resources": items }))
        }
    }
}

async fn list_azure_resources(
    azure: &hive_integrations::cloud::AzureClient,
    resource_type: &str,
) -> Result<serde_json::Value, String> {
    match resource_type {
        "storage" => {
            let accounts = azure.list_storage_accounts().await.map_err(|e| format!("Azure error: {e}"))?;
            let items: Vec<serde_json::Value> = accounts.iter().map(|a| json!({
                "name": a.name, "kind": a.kind, "location": a.location
            })).collect();
            Ok(json!({ "provider": "azure", "resource_type": "storage", "count": items.len(), "resources": items }))
        }
        "functions" => {
            let fns = azure.list_functions().await.map_err(|e| format!("Azure error: {e}"))?;
            let items: Vec<serde_json::Value> = fns.iter().map(|f| json!({
                "name": f.name, "runtime": f.runtime, "state": f.state
            })).collect();
            Ok(json!({ "provider": "azure", "resource_type": "functions", "count": items.len(), "resources": items }))
        }
        _ => {
            let vms = azure.list_vms().await.map_err(|e| format!("Azure error: {e}"))?;
            let items: Vec<serde_json::Value> = vms.iter().map(|v| json!({
                "name": v.name, "size": v.vm_size, "status": v.status, "location": v.location
            })).collect();
            Ok(json!({ "provider": "azure", "resource_type": "vms", "count": items.len(), "resources": items }))
        }
    }
}

async fn list_gcp_resources(
    gcp: &hive_integrations::cloud::GcpClient,
    resource_type: &str,
) -> Result<serde_json::Value, String> {
    match resource_type {
        "storage" => {
            let buckets = gcp.list_gcs_buckets().await.map_err(|e| format!("GCP error: {e}"))?;
            let items: Vec<serde_json::Value> = buckets.iter().map(|b| json!({
                "name": b.name, "location": b.location, "storage_class": b.storage_class
            })).collect();
            Ok(json!({ "provider": "gcp", "resource_type": "storage", "count": items.len(), "resources": items }))
        }
        "functions" => {
            let fns = gcp.list_cloud_functions().await.map_err(|e| format!("GCP error: {e}"))?;
            let items: Vec<serde_json::Value> = fns.iter().map(|f| json!({
                "name": f.name, "runtime": f.runtime, "status": f.status
            })).collect();
            Ok(json!({ "provider": "gcp", "resource_type": "functions", "count": items.len(), "resources": items }))
        }
        _ => {
            let instances = gcp.list_compute_instances().await.map_err(|e| format!("GCP error: {e}"))?;
            let items: Vec<serde_json::Value> = instances.iter().map(|i| json!({
                "name": i.name, "machine_type": i.machine_type, "status": i.status, "zone": i.zone
            })).collect();
            Ok(json!({ "provider": "gcp", "resource_type": "compute", "count": items.len(), "resources": items }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::Path;
    use axum::http::{HeaderValue, StatusCode, header};
    use axum::response::{IntoResponse, Response};
    use axum::routing::{delete, get, post, put};
    use axum::{Json, Router};
    use serde_json::Value;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::net::TcpListener;

    #[derive(Default)]
    struct MockA2aService {
        runs: AtomicUsize,
    }

    #[async_trait]
    impl OutboundA2aService for MockA2aService {
        async fn list_agents(&self) -> Result<Vec<A2aAgentRecord>, String> {
            Ok(vec![A2aAgentRecord {
                name: "remote-builder".into(),
                url: "http://remote.example.test".into(),
                api_key_configured: true,
                discovered: true,
                card_name: Some("Remote Builder".into()),
                description: Some("Builds remotely".into()),
                version: Some("1.0.0".into()),
                skills: vec!["build".into(), "review".into()],
            }])
        }

        async fn discover_agent(&self, identifier: &str) -> Result<A2aAgentRecord, String> {
            if identifier == "missing" {
                return Err("agent not found".into());
            }
            Ok(A2aAgentRecord {
                name: identifier.into(),
                url: "http://remote.example.test".into(),
                api_key_configured: true,
                discovered: true,
                card_name: Some("Remote Builder".into()),
                description: Some("Builds remotely".into()),
                version: Some("1.0.0".into()),
                skills: vec!["build".into()],
            })
        }

        async fn run_task(
            &self,
            identifier: &str,
            prompt: &str,
            skill_id: Option<&str>,
        ) -> Result<A2aTaskRecord, String> {
            self.runs.fetch_add(1, Ordering::SeqCst);
            if identifier == "missing" {
                return Err("agent not found".into());
            }
            Ok(A2aTaskRecord {
                agent_name: identifier.into(),
                url: "http://remote.example.test".into(),
                task_id: "task-1".into(),
                state: "Completed".into(),
                skill_id: skill_id.map(ToOwned::to_owned),
                output: format!("ran: {prompt}"),
            })
        }
    }

    fn test_services(
        a2a: Arc<dyn OutboundA2aService>,
        ollama_url: Option<String>,
        hue: Option<Arc<hive_integrations::smart_home::PhilipsHueClient>>,
    ) -> IntegrationServices {
        IntegrationServices {
            messaging: Arc::new(hive_integrations::messaging::MessagingHub::new()),
            project_management: Arc::new(
                hive_integrations::project_management::ProjectManagementHub::new(),
            ),
            knowledge: Arc::new(hive_integrations::knowledge::KnowledgeHub::new()),
            database: Arc::new(hive_integrations::database::DatabaseHub::new()),
            docker: Arc::new(hive_integrations::docker::DockerClient::new()),
            kubernetes: Arc::new(hive_integrations::kubernetes::KubernetesClient::new()),
            a2a,
            browser: Arc::new(hive_integrations::browser::BrowserAutomation::new()),
            ollama: Arc::new(hive_terminal::local_ai::OllamaManager::new(ollama_url)),
            hue,
            aws: Arc::new(hive_integrations::cloud::AwsClient::new(None, None)),
            azure: Arc::new(hive_integrations::cloud::AzureClient::new(None)),
            gcp: Arc::new(hive_integrations::cloud::GcpClient::new(None)),
            docs_indexer: Arc::new(hive_integrations::docs_indexer::DocsIndexer::empty()),
        }
    }

    fn call_tool(
        tools: &[(McpTool, ToolHandler)],
        name: &str,
        args: Value,
    ) -> Result<Value, String> {
        let (_, handler) = tools
            .iter()
            .find(|(tool, _)| tool.name == name)
            .unwrap_or_else(|| panic!("missing tool {name}"));
        handler(args)
    }

    async fn start_ollama_server() -> String {
        async fn tags() -> Json<Value> {
            Json(serde_json::json!({
                "models": [
                    { "name": "llama3.2:latest", "size": 42, "modified_at": "2026-03-09T00:00:00Z" }
                ]
            }))
        }

        async fn show() -> Json<Value> {
            Json(serde_json::json!({
                "size": 42,
                "modified_at": "2026-03-09T00:00:00Z"
            }))
        }

        async fn delete_model() -> StatusCode {
            StatusCode::OK
        }

        async fn pull() -> Response {
            let body = Body::from(
                "{\"status\":\"pulling manifest\",\"completed\":1,\"total\":2}\n{\"status\":\"success\"}\n",
            );
            (
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/x-ndjson"),
                )],
                body,
            )
                .into_response()
        }

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/api/tags", get(tags))
            .route("/api/show", post(show))
            .route("/api/delete", delete(delete_model))
            .route("/api/pull", post(pull));

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        format!("http://{addr}")
    }

    async fn start_hue_server() -> String {
        async fn lights() -> Json<Value> {
            Json(serde_json::json!({
                "1": {
                    "name": "Desk Lamp",
                    "state": { "on": true, "bri": 200, "reachable": true }
                }
            }))
        }

        async fn set_light(Path(_id): Path<String>) -> StatusCode {
            StatusCode::OK
        }

        async fn scenes() -> Json<Value> {
            Json(serde_json::json!({
                "scene-1": { "name": "Focus" }
            }))
        }

        async fn activate_scene() -> StatusCode {
            StatusCode::OK
        }

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/lights", get(lights))
            .route("/lights/:id/state", put(set_light))
            .route("/scenes", get(scenes))
            .route("/groups/0/action", put(activate_scene));

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        format!("http://{addr}")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a2a_tools_use_shared_service() {
        let services = test_services(Arc::new(MockA2aService::default()), None, None);
        let tools = wire_integration_handlers(services);

        let listed = call_tool(&tools, "a2a_list_agents", json!({})).unwrap();
        assert_eq!(listed["count"], 1);
        assert_eq!(listed["agents"][0]["name"], "remote-builder");

        let discovered = call_tool(&tools, "a2a_discover_agent", json!({"agent": "remote-builder"})).unwrap();
        assert_eq!(discovered["skills"][0], "build");

        let ran = call_tool(
            &tools,
            "a2a_run_task",
            json!({"agent": "remote-builder", "prompt": "ship it", "skill_id": "build"}),
        )
        .unwrap();
        assert_eq!(ran["agent_name"], "remote-builder");
        assert_eq!(ran["skill_id"], "build");
        assert!(ran["output"].as_str().unwrap().contains("ship it"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ollama_tools_use_configured_endpoint() {
        let ollama_url = start_ollama_server().await;
        let services = test_services(
            Arc::new(MockA2aService::default()),
            Some(ollama_url),
            None,
        );
        let tools = wire_integration_handlers(services);

        let listed = call_tool(&tools, "ollama_list_models", json!({})).unwrap();
        assert_eq!(listed["count"], 1);
        assert_eq!(listed["models"][0]["name"], "llama3.2:latest");

        let shown = call_tool(&tools, "ollama_show_model", json!({"model": "llama3.2:latest"})).unwrap();
        assert_eq!(shown["name"], "llama3.2:latest");

        let pulled = call_tool(&tools, "ollama_pull_model", json!({"model": "llama3.2:latest"})).unwrap();
        assert_eq!(pulled["status"], "pulled");
        assert!(pulled["progress"].as_array().unwrap().len() >= 2);

        let deleted = call_tool(&tools, "ollama_delete_model", json!({"model": "llama3.2:latest"})).unwrap();
        assert_eq!(deleted["status"], "deleted");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn hue_tools_use_configured_client_and_fail_without_one() {
        let hue_url = start_hue_server().await;
        let hue_client = Arc::new(hive_integrations::smart_home::PhilipsHueClient::with_base_url(
            "127.0.0.1",
            "key",
            &hue_url,
        ));
        let tools = wire_integration_handlers(test_services(
            Arc::new(MockA2aService::default()),
            None,
            Some(hue_client),
        ));

        let lights = call_tool(&tools, "hue_list_lights", json!({})).unwrap();
        assert_eq!(lights["count"], 1);
        assert_eq!(lights["lights"][0]["name"], "Desk Lamp");

        let scenes = call_tool(&tools, "hue_list_scenes", json!({})).unwrap();
        assert_eq!(scenes["count"], 1);
        assert_eq!(scenes["scenes"][0]["name"], "Focus");

        let updated = call_tool(
            &tools,
            "hue_set_light_state",
            json!({"light_id": "1", "on": true, "brightness": 180}),
        )
        .unwrap();
        assert_eq!(updated["status"], "updated");

        let activated = call_tool(&tools, "hue_activate_scene", json!({"scene_id": "scene-1"})).unwrap();
        assert_eq!(activated["status"], "activated");

        let missing = wire_integration_handlers(test_services(
            Arc::new(MockA2aService::default()),
            None,
            None,
        ));
        let err = call_tool(&missing, "hue_list_lights", json!({})).unwrap_err();
        assert!(err.contains("not configured"));
    }
}
