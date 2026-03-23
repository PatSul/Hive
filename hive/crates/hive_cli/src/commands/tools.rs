//! hive tool command handler.

use anyhow::{Context, Result};
use hive_agents::integration_tools::IntegrationServices;
use hive_agents::mcp_server::McpServer;
use hive_core::config::ConfigManager;
use hive_core::HiveConfig;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

pub async fn list(workspace: Option<PathBuf>, as_json: bool) -> Result<()> {
    let workspace_root = resolve_workspace_root(workspace)?;
    let server = build_server(workspace_root.clone()).await?;
    let tools = server.list_tools();

    if as_json {
        println!("{}", serde_json::to_string_pretty(&tools)?);
        return Ok(());
    }

    let tool_count = tools.len();
    println!("Hive Local Tools");
    println!("  Workspace: {}", workspace_root.display());
    println!();
    println!("  {:<28} {}", "NAME", "DESCRIPTION");
    println!("  {}", "-".repeat(96));
    for tool in tools {
        println!("  {:<28} {}", tool.name, tool.description);
    }
    println!();
    println!("  {} tools available", tool_count);
    Ok(())
}

pub async fn call(workspace: Option<PathBuf>, name: &str, args_json: &str) -> Result<()> {
    let workspace_root = resolve_workspace_root(workspace)?;
    let server = build_server(workspace_root).await?;
    let args: Value = serde_json::from_str(args_json)
        .with_context(|| format!("Failed to parse --args JSON: {args_json}"))?;
    let result = server
        .call_tool_value(name, args)
        .map_err(anyhow::Error::msg)?;

    match result {
        Value::String(text) => println!("{text}"),
        other => println!("{}", serde_json::to_string_pretty(&other)?),
    }

    Ok(())
}

fn resolve_workspace_root(workspace: Option<PathBuf>) -> Result<PathBuf> {
    let root = match workspace {
        Some(path) => path,
        None => std::env::current_dir().context("Failed to resolve current directory")?,
    };

    match root.canonicalize() {
        Ok(path) => Ok(path),
        Err(_) => Ok(root),
    }
}

async fn build_server(workspace_root: PathBuf) -> Result<McpServer> {
    let config_manager = ConfigManager::new()?;
    let config = config_manager.get();

    let mut knowledge_hub = hive_integrations::knowledge::KnowledgeHub::new();

    if let Some(ref vault_path) = config.obsidian_vault_path {
        if !vault_path.is_empty() {
            let mut obsidian = hive_integrations::knowledge::ObsidianProvider::new(vault_path);
            let _ = obsidian.index_vault().await;
            knowledge_hub.register_provider(Box::new(obsidian));
        }
    }

    if let Some(ref notion_key) = config.notion_api_key {
        if !notion_key.is_empty() {
            if let Ok(notion) = hive_integrations::knowledge::NotionClient::new(notion_key) {
                knowledge_hub.register_provider(Box::new(notion));
            }
        }
    }

    let docs_indexer = hive_integrations::docs_indexer::DocsIndexer::new()
        .map(Arc::new)
        .unwrap_or_else(|_| Arc::new(hive_integrations::docs_indexer::DocsIndexer::empty()));

    let a2a_config_path = HiveConfig::base_dir()
        .unwrap_or_else(|_| PathBuf::from(".hive"))
        .join("a2a.toml");
    let a2a = hive_a2a::A2aClientService::load_or_create(&a2a_config_path)
        .map(Arc::new)
        .unwrap_or_else(|_| {
            Arc::new(hive_a2a::A2aClientService::with_config(
                a2a_config_path,
                hive_a2a::A2aConfig::default(),
            ))
        });

    let hue = config
        .hue_bridge_ip
        .as_deref()
        .zip(config.hue_api_key.as_deref())
        .map(|(bridge_ip, api_key)| {
            Arc::new(hive_integrations::smart_home::PhilipsHueClient::new(
                bridge_ip, api_key,
            ))
        });

    let services = IntegrationServices {
        messaging: Arc::new(hive_integrations::messaging::MessagingHub::new()),
        project_management: Arc::new(
            hive_integrations::project_management::ProjectManagementHub::new(),
        ),
        knowledge: Arc::new(knowledge_hub),
        database: Arc::new(hive_integrations::database::DatabaseHub::new()),
        docker: Arc::new(hive_integrations::docker::DockerClient::new()),
        kubernetes: Arc::new(hive_integrations::kubernetes::KubernetesClient::new()),
        a2a,
        browser: Arc::new(hive_integrations::browser::BrowserAutomation::new()),
        ollama: Arc::new(hive_terminal::local_ai::OllamaManager::new(Some(
            config.ollama_url.clone(),
        ))),
        hue,
        aws: Arc::new(hive_integrations::cloud::AwsClient::new(None, None)),
        azure: Arc::new(hive_integrations::cloud::AzureClient::new(None)),
        gcp: Arc::new(hive_integrations::cloud::GcpClient::new(None)),
        docs_indexer,
        google_drive: None,
        google_sheets: None,
        google_docs: None,
        google_tasks: None,
        google_contacts: None,
        bitbucket: None,
        gitlab: None,
        webhooks: Arc::new(std::sync::Mutex::new(
            hive_integrations::webhooks::WebhookRegistry::new(),
        )),
        rag: None,
        automation: None,
        wallet_store: None,
        rpc_config: None,
    };

    let mut server = McpServer::new(workspace_root);
    server.wire_integrations(services);
    Ok(server)
}
