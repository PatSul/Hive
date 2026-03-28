use std::sync::Arc;

use gpui::*;
use tracing::{error, info, warn};

use hive_ai::speculative::{self, SpeculativeConfig};
use hive_ai::types::{ChatRequest, ToolDefinition as AiToolDefinition};
use hive_ui_core::{AppCollectiveMemory, AppCortexInteractionTracker};

use super::{
    AiProvider, AppActivityService, AppAgentNotifications, AppAiService, AppConfig,
    AppContextEngine, AppContextSelection, AppHiveMemory, AppKnowledge, AppKnowledgeFiles,
    AppLearning, AppMcpServer, AppQuickIndex, AppRagService, AppSecurity, AppSemanticSearch,
    AppShield, AppSkillManager, AppTts, ApplyAllEdits, ApplyCodeBlock, ChatReadAloud, ClearChat,
    HiveWorkspace, MessageRole, NewConversation, Panel, data_refresh,
};

pub(super) fn handle_new_conversation(
    workspace: &mut HiveWorkspace,
    _action: &NewConversation,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("NewConversation action triggered");
    workspace.chat_service.update(cx, |svc, _cx| {
        svc.new_conversation();
    });
    workspace.cached_chat_data.markdown_cache.clear();
    data_refresh::refresh_history(workspace);
    workspace.sidebar.active_panel = Panel::Chat;
    workspace.session_dirty = true;
    cx.notify();
}

pub(super) fn handle_clear_chat(
    workspace: &mut HiveWorkspace,
    _action: &ClearChat,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("ClearChat action triggered");
    workspace.chat_service.update(cx, |svc, _cx| {
        svc.clear();
    });
    workspace.cached_chat_data.markdown_cache.clear();
    cx.notify();
}

pub(super) fn handle_chat_read_aloud(
    _workspace: &mut HiveWorkspace,
    action: &ChatReadAloud,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if cx.has_global::<AppTts>() {
        let tts = cx.global::<AppTts>().0.clone();
        let text = action.content.clone();
        cx.spawn(async move |_this, _app: &mut AsyncApp| {
            if let Err(e) = tts.speak_and_play(&text).await {
                tracing::warn!("TTS read-aloud playback failed: {e}");
            }
        })
        .detach();
    }
}

pub(super) fn handle_apply_code_block(
    workspace: &mut HiveWorkspace,
    action: &ApplyCodeBlock,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let file_path = workspace.current_project_root.join(&action.file_path);

    if cx.has_global::<AppSecurity>() {
        if let Err(e) = cx.global::<AppSecurity>().0.check_path(&file_path) {
            error!("Apply: blocked path: {e}");
            workspace.chat_service.update(cx, |svc, cx| {
                svc.set_error(format!("Apply blocked: {e}"), cx);
            });
            return;
        }
    }

    let old_content = std::fs::read_to_string(&file_path).ok();
    let new_content = action.content.clone();

    let diff_lines = if let Some(ref old) = old_content {
        hive_ui_panels::components::diff_viewer::compute_diff_lines_public(old, &new_content)
    } else {
        new_content
            .lines()
            .map(|line| hive_ui_panels::components::DiffLine::Added(line.to_string()))
            .collect()
    };

    workspace.chat_service.update(cx, |svc, cx| {
        svc.pending_approval = Some(crate::chat_service::PendingToolApproval {
            tool_call_id: format!("apply-{}", action.file_path),
            tool_name: "apply_code_block".to_string(),
            file_path: file_path.to_string_lossy().to_string(),
            new_content: new_content.clone(),
            old_content,
            diff_lines,
        });
        cx.notify();
    });
    cx.notify();
}

pub(super) fn handle_apply_all_edits(
    workspace: &mut HiveWorkspace,
    _action: &ApplyAllEdits,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let last_assistant_content = workspace
        .chat_service
        .read(cx)
        .messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::Assistant)
        .map(|message| message.content.clone())
        .unwrap_or_default();

    let edits = hive_agents::parse_edits(&last_assistant_content);
    if edits.is_empty() {
        workspace.chat_service.update(cx, |svc, cx| {
            svc.set_error("No file edits found in the last response", cx);
        });
        return;
    }

    for edit in &edits {
        let file_path = workspace.current_project_root.join(&edit.file_path);
        if let Err(e) = std::fs::write(&file_path, &edit.new_content) {
            error!("Apply all: failed to write {}: {e}", edit.file_path);
        } else {
            info!("Applied edit to {}", edit.file_path);
        }
    }

    info!("Applied {} file edit(s) from response", edits.len());
    cx.notify();
}

pub(super) fn handle_copy_to_clipboard(
    _workspace: &mut HiveWorkspace,
    action: &super::CopyToClipboard,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    cx.write_to_clipboard(ClipboardItem::new_string(action.content.clone()));
}

pub(super) fn handle_copy_full_prompt(
    workspace: &mut HiveWorkspace,
    _action: &super::CopyFullPrompt,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let mut prompt = selected_context_prompt(workspace, cx);
    let text = workspace.chat_input.read(cx).current_text(cx);
    if !text.is_empty() {
        prompt.push_str(&format!("## Instruction\n{}\n", text));
    }

    cx.write_to_clipboard(ClipboardItem::new_string(prompt));
}

pub(super) fn handle_export_prompt(
    workspace: &mut HiveWorkspace,
    _action: &super::ExportPrompt,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    let prompt = selected_context_prompt(workspace, cx);
    let export_path = workspace.current_project_root.join("hive-prompt-export.md");
    if let Err(e) = std::fs::write(&export_path, &prompt) {
        error!("Export prompt failed: {e}");
    } else {
        info!("Exported prompt to {}", export_path.display());
    }
}

/// Called when `ChatInputView` emits `SubmitMessage`. The input has
/// already been cleared by the view before this is invoked.
///
/// 1. Records the text in `ChatService`.
/// 2. Extracts the provider + request from the `AppAiService` global.
/// 3. Spawns an async task that calls `provider.stream_chat()` and feeds
///    the resulting receiver back into `ChatService::attach_stream`.
pub(super) fn handle_send_text(
    workspace: &mut HiveWorkspace,
    text: String,
    context_files: Vec<std::path::PathBuf>,
    window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    if text.trim().is_empty() {
        return;
    }

    if cx.has_global::<AppCortexInteractionTracker>() {
        cx.global::<AppCortexInteractionTracker>().0.store(
            chrono::Utc::now().timestamp(),
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    let model = workspace.chat_service.read(cx).current_model().to_string();

    // Shield: scan outgoing text before sending to AI.
    // Check if the shield is enabled in config.
    let shield_enabled = if cx.has_global::<AppConfig>() {
        cx.global::<AppConfig>().0.get().shield_enabled
    } else {
        true // default to enabled if no config
    };

    let send_text = if shield_enabled && cx.has_global::<AppShield>() {
        let shield = &cx.global::<AppShield>().0;
        let result = shield.process_outgoing(&text, &model);
        match result.action {
            hive_shield::ShieldAction::Allow => text,
            hive_shield::ShieldAction::CloakAndAllow(ref cloaked) => {
                info!("Shield: PII cloaked in outgoing message");
                cloaked.text.clone()
            }
            hive_shield::ShieldAction::Block(ref reason) => {
                warn!("Shield: blocked outgoing message: {reason}");
                workspace.chat_service.update(cx, |svc, cx| {
                    svc.set_error(format!("Message blocked by privacy shield: {reason}"), cx);
                });
                return;
            }
            hive_shield::ShieldAction::Warn(ref warning) => {
                warn!("Shield: warning on outgoing message: {warning}");
                text
            }
        }
    } else {
        text
    };

    // Budget enforcement: block sends when daily/monthly limit exceeded.
    if cx.has_global::<AppAiService>() {
        let tracker = cx.global::<AppAiService>().0.cost_tracker();
        if tracker.is_daily_budget_exceeded() {
            workspace.chat_service.update(cx, |svc, cx| {
                svc.set_error(
                    "Daily cost budget exceeded. Adjust your limit in Settings -> Costs."
                        .to_string(),
                    cx,
                );
            });
            return;
        }
        if tracker.is_monthly_budget_exceeded() {
            workspace.chat_service.update(cx, |svc, cx| {
                svc.set_error(
                    "Monthly cost budget exceeded. Adjust your limit in Settings -> Costs."
                        .to_string(),
                    cx,
                );
            });
            return;
        }
    }

    if try_handle_swarm_send(workspace, send_text.clone(), &model, cx) {
        return;
    }

    // Save the user text for RAG query before it is consumed by send_message.
    let user_query_text = send_text.clone();

    // 1. Record user message + create placeholder assistant message.
    workspace.chat_service.update(cx, |svc, cx| {
        svc.send_message(send_text, &model, cx);
    });

    // 2. Build the AI wire-format messages.
    let ai_messages = workspace.chat_service.read(cx).build_ai_messages();

    // 2b. Classify user intent and load context by tier (L0/L1/L2).
    //     L0 = always (knowledge files, preferences)
    //     L1 = project structure, open files
    //     L2 = RAG, semantic search, memory recall
    let context_tier = {
        let classify_msgs = workspace.chat_service.read(cx).build_ai_messages();
        let task_type = hive_ai::routing::classify_task(&classify_msgs);
        hive_ai::ContextTier::from_task_keyword(&format!("{:?}", task_type))
    };
    tracing::debug!("Context tier for query: {:?}", context_tier);

    let ai_messages = {
        let mut all_context = String::new();

        // Clear ephemeral sources from the context engine to prevent bloat.
        if cx.has_global::<AppContextEngine>() {
            if let Ok(mut ctx_engine) = cx.global::<AppContextEngine>().0.lock() {
                ctx_engine.clear_ephemeral();
            }
        }

        // L2 only: Pull from RAG document chunks
        if context_tier == hive_ai::ContextTier::L2 && cx.has_global::<AppRagService>() {
            if let Ok(rag_svc) = cx.global::<AppRagService>().0.lock() {
                let rag_query = hive_ai::RagQuery {
                    query: user_query_text.clone(),
                    max_results: 10,
                    min_similarity: 0.1,
                };
                if let Ok(result) = rag_svc.query(&rag_query) {
                    if !result.context.is_empty() {
                        all_context.push_str(&result.context);
                        all_context.push_str("\n\n");
                    }
                }
            }
        }

        // L2 only: Semantic search
        if context_tier == hive_ai::ContextTier::L2 && cx.has_global::<AppSemanticSearch>() {
            let mut candidate_paths = Vec::new();

            if cx.has_global::<AppQuickIndex>() {
                let quick_index = &cx.global::<AppQuickIndex>().0;
                let mut seen = std::collections::HashSet::new();
                for symbol in quick_index.key_symbols.iter().take(32) {
                    let path = quick_index.project_root.join(&symbol.file);
                    if seen.insert(path.clone()) {
                        candidate_paths.push(path);
                    }
                }
            }

            if candidate_paths.is_empty() {
                candidate_paths.push(workspace.current_project_root.clone());
            }

            let candidate_refs: Vec<&std::path::Path> =
                candidate_paths.iter().map(|path| path.as_path()).collect();

            if let Ok(mut semantic_search) = cx.global::<AppSemanticSearch>().0.lock() {
                let results =
                    semantic_search.search_with_context(&user_query_text, &candidate_refs, 5, 1);

                if !results.is_empty() {
                    let semantic_context = results
                        .iter()
                        .map(|result| {
                            format!(
                                "--- {}:{} ---\n{}\n{}\n{}",
                                result.file_path,
                                result.line_number,
                                result.context_before,
                                result.content,
                                result.context_after
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n\n");

                    all_context.push_str("## Semantic Search Matches\n\n");
                    all_context.push_str(&semantic_context);
                    all_context.push_str("\n\n");
                }
            }
        }

        // HiveMemory + KnowledgeHub are async - queried in the spawn
        // blocks below. memory_context stays empty here; the real
        // enrichment happens off the UI thread via enrich_request().
        let memory_context = String::new();

        // For now, we seed the ContextEngine with whatever RAG found, plus we can index the current directory.
        if cx.has_global::<AppContextEngine>() {
            if let Ok(mut ctx_engine) = cx.global::<AppContextEngine>().0.lock() {
                // Seed the engine with the retrieved context so TF-IDF
                // curation can blend RAG and semantic-search matches.
                if !all_context.is_empty() {
                    ctx_engine.add_file("retrieved_context.txt", &all_context);
                }

                // Seed engine with project knowledge files so they
                // participate in TF-IDF scoring alongside RAG results.
                if cx.has_global::<AppKnowledgeFiles>() {
                    for ks in &cx.global::<AppKnowledgeFiles>().0 {
                        let label = ks
                            .path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "knowledge".to_string());
                        ctx_engine.add_project_knowledge(&label, &ks.content);
                    }
                }

                // Use ContextEngine to curate sources into a token budget.
                // Budget scales with tier: L0=1000, L1=2000, L2=dynamic.
                let budget_tokens = match context_tier {
                    hive_ai::ContextTier::L0 => 1000,
                    hive_ai::ContextTier::L1 => 2000,
                    hive_ai::ContextTier::L2 => 4000,
                };
                let budget = hive_ai::context_engine::ContextBudget {
                    max_tokens: budget_tokens,
                    max_sources: 10,
                    reserved_tokens: 0,
                };
                let curated = ctx_engine.curate(&user_query_text, &budget);

                all_context.clear();
                for source in curated.sources {
                    all_context.push_str(&source.content);
                    all_context.push_str("\n\n");
                }
            }
        }

        let mut augmented = ai_messages.clone();

        // Inject project knowledge files (HIVE.md, README.md, etc.) as the
        // highest-priority system context. Re-scan on each message for freshness.
        {
            let fresh_sources =
                hive_ai::KnowledgeFileScanner::scan(&workspace.current_project_root);
            let knowledge_text = hive_ai::KnowledgeFileScanner::format_for_context(&fresh_sources);

            // Update the global so other systems see the latest state.
            cx.set_global(AppKnowledgeFiles(fresh_sources));

            if !knowledge_text.trim().is_empty() {
                let kf_idx = augmented
                    .iter()
                    .position(|m| m.role != hive_ai::types::MessageRole::System)
                    .unwrap_or(0);
                augmented.insert(
                    kf_idx,
                    hive_ai::types::ChatMessage {
                        role: hive_ai::types::MessageRole::System,
                        content: knowledge_text,
                        timestamp: chrono::Utc::now(),
                        tool_call_id: None,
                        tool_calls: None,
                    },
                );
            }
        }

        // Determine context format for AI prompt encoding.
        let ctx_format = if cx.has_global::<AppConfig>() {
            hive_ai::ContextFormat::from_config_str(
                &cx.global::<AppConfig>().0.get().context_format,
            )
        } else {
            hive_ai::ContextFormat::Markdown
        };

        // L1+: Inject fast-path project index as lightweight project context.
        // This gives the AI immediate awareness of the project structure,
        // key symbols, dependencies, and recent git activity -- available
        // even before the deeper RAG index has populated.
        if context_tier != hive_ai::ContextTier::L0 && cx.has_global::<AppQuickIndex>() {
            let quick_ctx = match ctx_format {
                hive_ai::ContextFormat::Toon => {
                    cx.global::<AppQuickIndex>().0.to_context_string_toon()
                }
                hive_ai::ContextFormat::Xml => {
                    cx.global::<AppQuickIndex>().0.to_context_string_xml()
                }
                _ => cx.global::<AppQuickIndex>().0.to_context_string(),
            };
            if !quick_ctx.trim().is_empty() {
                let qi_idx = augmented
                    .iter()
                    .position(|m| m.role != hive_ai::types::MessageRole::System)
                    .unwrap_or(0);
                augmented.insert(
                    qi_idx,
                    hive_ai::types::ChatMessage {
                        role: hive_ai::types::MessageRole::System,
                        content: quick_ctx,
                        timestamp: chrono::Utc::now(),
                        tool_call_id: None,
                        tool_calls: None,
                    },
                );
            }
        }

        let insert_idx = augmented
            .iter()
            .position(|m| m.role != hive_ai::types::MessageRole::System)
            .unwrap_or(0);

        // Inject recalled memories as a dedicated system message
        if !memory_context.trim().is_empty() {
            augmented.insert(
                insert_idx,
                hive_ai::types::ChatMessage {
                    role: hive_ai::types::MessageRole::System,
                    content: format!(
                        "# Recalled Memories\n\nRelevant context from previous conversations:\n{}",
                        memory_context
                    ),
                    timestamp: chrono::Utc::now(),
                    tool_call_id: None,
                    tool_calls: None,
                },
            );
        }

        // Inject retrieved code context
        if !all_context.trim().is_empty() {
            let ctx_idx = augmented
                .iter()
                .position(|m| m.role != hive_ai::types::MessageRole::System)
                .unwrap_or(0);
            augmented.insert(
                ctx_idx,
                hive_ai::types::ChatMessage {
                    role: hive_ai::types::MessageRole::System,
                    content: format!("# Retrieved Context\n\n{}", all_context),
                    timestamp: chrono::Utc::now(),
                    tool_call_id: None,
                    tool_calls: None,
                },
            );
        }

        // Inject user-selected context files (checked in Files panel).
        if !context_files.is_empty() {
            let use_toon = ctx_format == hive_ai::ContextFormat::Toon;
            let use_xml = ctx_format == hive_ai::ContextFormat::Xml;
            let mut ctx_block = if use_toon {
                String::from("context_files:\n")
            } else if use_xml {
                String::from("<context_files>\n")
            } else {
                String::from("# Selected Context Files\n\n")
            };
            for path in &context_files {
                let rel = path
                    .strip_prefix(&workspace.current_project_root)
                    .unwrap_or(path);
                let content = std::fs::read_to_string(path).unwrap_or_default();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                let tokens = content.len().div_ceil(4);
                if use_toon {
                    // TOON: compact file{path,tokens}:content format
                    ctx_block.push_str(&format!(
                        "  file{{path:{},ext:{},tok:{}}}\n{}\n---\n",
                        rel.display(),
                        ext,
                        tokens,
                        content
                    ));
                } else if use_xml {
                    ctx_block.push_str(&format!(
                        "<file path=\"{}\" tokens=\"{}\"><![CDATA[{}]]></file>\n",
                        rel.display(),
                        tokens,
                        content
                    ));
                } else {
                    ctx_block.push_str(&format!(
                        "## {}\n```{}\n{}\n```\n\n",
                        rel.display(),
                        ext,
                        content
                    ));
                }
            }
            if use_xml {
                ctx_block.push_str("</context_files>");
            }
            let cf_idx = augmented
                .iter()
                .position(|m| m.role != hive_ai::types::MessageRole::System)
                .unwrap_or(0);
            augmented.insert(
                cf_idx,
                hive_ai::types::ChatMessage {
                    role: hive_ai::types::MessageRole::System,
                    content: ctx_block,
                    timestamp: chrono::Utc::now(),
                    tool_call_id: None,
                    tool_calls: None,
                },
            );
        }

        augmented
    };

    // 2c. Check for /command skill activation and inject instructions
    let ai_messages = {
        let mut msgs = ai_messages;
        let trimmed_query = user_query_text.trim();
        if trimmed_query.starts_with('/') {
            let cmd_name = trimmed_query[1..].split_whitespace().next().unwrap_or("");
            let mut skill_instructions: Option<String> = None;

            // Check built-in skills registry
            if cx.has_global::<hive_ui_core::AppSkills>() {
                if let Ok(instructions) =
                    cx.global::<hive_ui_core::AppSkills>().0.dispatch(cmd_name)
                {
                    skill_instructions = Some(instructions.to_string());
                }
            }
            // Check user-created skills (file-based)
            if skill_instructions.is_none() && cx.has_global::<AppSkillManager>() {
                if let Ok(Some(skill)) = cx.global::<AppSkillManager>().0.get(cmd_name) {
                    if skill.enabled {
                        skill_instructions = Some(skill.instructions.clone());
                    }
                }
            }

            if let Some(instructions) = skill_instructions {
                let insert_idx = msgs
                    .iter()
                    .position(|m| m.role != hive_ai::types::MessageRole::System)
                    .unwrap_or(0);
                msgs.insert(
                    insert_idx,
                    hive_ai::types::ChatMessage {
                        role: hive_ai::types::MessageRole::System,
                        content: format!("# Active Skill: /{}\n\n{}", cmd_name, instructions),
                        timestamp: chrono::Utc::now(),
                        tool_call_id: None,
                        tool_calls: None,
                    },
                );
            }
        }
        msgs
    };

    // 3. Build tool definitions from built-in + MCP integration tools.
    let agent_defs = hive_agents::tool_use::builtin_tool_definitions();
    let mut tool_defs: Vec<AiToolDefinition> = agent_defs
        .into_iter()
        .map(|d| AiToolDefinition {
            name: d.name,
            description: d.description,
            input_schema: d.input_schema,
        })
        .collect();

    // Include MCP integration tools (messaging, project mgmt, browser, etc.)
    if cx.has_global::<AppMcpServer>() {
        let mcp = &cx.global::<AppMcpServer>().0;
        for tool in mcp.list_tools() {
            // Skip builtins already included to avoid duplicates.
            if tool_defs.iter().any(|t| t.name == tool.name) {
                continue;
            }
            tool_defs.push(AiToolDefinition {
                name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: tool.input_schema.clone(),
            });
        }
    }

    // 4a. Build system prompt from learned preferences (if any).
    let mut system_prompt = if cx.has_global::<AppLearning>() {
        let learning = &cx.global::<AppLearning>().0;
        match learning.preference_model.prompt_addendum() {
            Ok(addendum) if !addendum.is_empty() => {
                info!("Injecting learned preferences into system prompt");
                Some(addendum)
            }
            _ => None,
        }
    } else {
        None
    };

    // When XML context format is active, instruct the AI to use <edit> tags.
    let ctx_format_for_prompt = if cx.has_global::<AppConfig>() {
        hive_ai::ContextFormat::from_config_str(&cx.global::<AppConfig>().0.get().context_format)
    } else {
        hive_ai::ContextFormat::Markdown
    };
    if ctx_format_for_prompt == hive_ai::ContextFormat::Xml {
        let xml_instruction = "\n\nWhen suggesting code changes, wrap each file edit in an XML tag: <edit path=\"relative/path\" lang=\"language\">new file content</edit>";
        system_prompt = Some(
            system_prompt
                .map(|s| s + xml_instruction)
                .unwrap_or_else(|| xml_instruction.to_string()),
        );
    }

    // 4b. Check if speculative decoding is enabled.
    let spec_config = if cx.has_global::<AppConfig>() {
        let cfg = cx.global::<AppConfig>().0.get();
        SpeculativeConfig {
            enabled: cfg.speculative_decoding,
            draft_model: cfg.speculative_draft_model.clone(),
            show_metrics: cfg.speculative_show_metrics,
        }
    } else {
        SpeculativeConfig::default()
    };

    // 4c. Extract provider + request from the global (sync - no await).
    //     If speculative decoding is enabled, also prepare the draft stream.
    let use_speculative = spec_config.enabled
        && cx.has_global::<AppAiService>()
        && cx
            .global::<AppAiService>()
            .0
            .prepare_speculative_stream(
                ai_messages.clone(),
                &model,
                system_prompt.clone(),
                Some(tool_defs.clone()),
                &spec_config,
            )
            .is_some();

    let stream_setup: Option<(Arc<dyn AiProvider>, ChatRequest)> =
        if cx.has_global::<AppAiService>() {
            cx.global::<AppAiService>().0.prepare_stream(
                ai_messages.clone(),
                &model,
                system_prompt.clone(),
                Some(tool_defs.clone()),
            )
        } else {
            None
        };

    let Some((provider, request)) = stream_setup else {
        workspace.chat_service.update(cx, |svc, cx| {
            svc.set_error(
                "No AI providers configured. Check Settings -> API Keys.",
                cx,
            );
        });
        return;
    };

    // 5. Spawn async: call provider.stream_chat, then attach with tool loop.
    let chat_svc = workspace.chat_service.downgrade();
    let model_for_attach = model.clone();
    let provider_for_loop = provider.clone();
    let request_for_loop = request.clone();

    // Clone async-capable globals for capture by the spawn blocks.
    let hive_mem_for_async: Option<
        std::sync::Arc<tokio::sync::Mutex<hive_ai::memory::HiveMemory>>,
    > = if cx.has_global::<AppHiveMemory>() {
        Some(cx.global::<AppHiveMemory>().0.clone())
    } else {
        None
    };
    let knowledge_hub_for_async: Option<
        std::sync::Arc<hive_integrations::knowledge::KnowledgeHub>,
    > = if cx.has_global::<AppKnowledge>() {
        let kb = cx.global::<AppKnowledge>().0.clone();
        if kb.provider_count() > 0 {
            Some(kb)
        } else {
            None
        }
    } else {
        None
    };
    let query_for_memory = user_query_text.clone();

    let task = if use_speculative {
        // Speculative decoding path: dual-stream from draft + primary.
        let speculative_setup = cx.global::<AppAiService>().0.prepare_speculative_stream(
            ai_messages,
            &model,
            system_prompt,
            Some(tool_defs),
            &spec_config,
        );

        if let Some((draft_provider, mut draft_request, primary_provider, mut primary_request)) =
            speculative_setup
        {
            let spec_config_clone = spec_config.clone();
            let hm = hive_mem_for_async.clone();
            let kb = knowledge_hub_for_async.clone();
            let qm = query_for_memory.clone();
            cx.spawn(async move |_this, app: &mut AsyncApp| {
                // Enrich both draft and primary requests with memory/knowledge.
                super::enrich_request_with_memory(&mut draft_request, &hm, &kb, &qm).await;
                super::enrich_request_with_memory(&mut primary_request, &hm, &kb, &qm).await;

                match speculative::speculative_stream(
                    draft_provider,
                    draft_request,
                    primary_provider.clone(),
                    primary_request.clone(),
                    spec_config_clone,
                )
                .await
                {
                    Ok(mut spec_rx) => {
                        // Convert speculative chunks into regular StreamChunk stream.
                        // Draft chunks get a "[speculating] " visual prefix.
                        // When primary starts, we send a reset-content signal.
                        let (tx, rx) = tokio::sync::mpsc::channel(256);
                        let _model_for_metrics = model_for_attach.clone();

                        tokio::spawn(async move {
                            let mut in_draft_phase = true;
                            while let Some(spec_chunk) = spec_rx.recv().await {
                                if spec_chunk.is_draft {
                                    // Forward draft content as-is (UI can style it)
                                    let _ = tx.send(spec_chunk.chunk).await;
                                } else {
                                    if in_draft_phase {
                                        // Transition: send a special "reset" chunk
                                        // The content field carries a marker the UI can detect
                                        let _ = tx
                                            .send(hive_ai::types::StreamChunk {
                                                content: "\n\n---\n\n".to_string(),
                                                done: false,
                                                thinking: None,
                                                usage: None,
                                                tool_calls: None,
                                                stop_reason: None,
                                            })
                                            .await;
                                        in_draft_phase = false;
                                    }

                                    // Append metrics info to the final chunk if available
                                    let mut chunk = spec_chunk.chunk;
                                    if let Some(metrics) = spec_chunk.metrics {
                                        if chunk.done {
                                            let metrics_text = format!(
                                                "\n\n> Speculative decoding saved ~{}ms | Draft: {} ({}ms) | Primary: {} ({}ms)",
                                                metrics.time_saved_ms,
                                                metrics.draft_model,
                                                metrics.draft_first_token_ms,
                                                metrics.primary_model,
                                                metrics.primary_first_token_ms,
                                            );
                                            chunk.content.push_str(&metrics_text);
                                        }
                                    }
                                    let _ = tx.send(chunk).await;
                                }
                            }
                        });

                        let _ = chat_svc.update(app, |svc, cx| {
                            svc.attach_tool_stream(
                                rx,
                                model_for_attach,
                                primary_provider,
                                primary_request,
                                cx,
                            );
                        });
                    }
                    Err(e) => {
                        error!("Speculative stream error: {e}");
                        // Fall back to normal stream (already enriched via
                        // the same hm/kb/qm captured by this spawn block).
                        let mut fallback_req = request.clone();
                        super::enrich_request_with_memory(&mut fallback_req, &hm, &kb, &qm).await;
                        match provider.stream_chat(&fallback_req).await {
                            Ok(rx) => {
                                let _ = chat_svc.update(app, |svc, cx| {
                                    svc.attach_tool_stream(
                                        rx,
                                        model_for_attach,
                                        provider_for_loop,
                                        request_for_loop,
                                        cx,
                                    );
                                });
                            }
                            Err(e2) => {
                                let _ = chat_svc.update(app, |svc, cx| {
                                    svc.set_error(format!("AI request failed: {e2}"), cx);
                                });
                            }
                        }
                    }
                }
            })
        } else {
            // Speculative setup failed, fall back to normal
            let hm = hive_mem_for_async.clone();
            let kb = knowledge_hub_for_async.clone();
            let qm = query_for_memory.clone();
            cx.spawn(async move |_this, app: &mut AsyncApp| {
                let mut enriched_request = request.clone();
                super::enrich_request_with_memory(&mut enriched_request, &hm, &kb, &qm).await;
                match provider.stream_chat(&enriched_request).await {
                    Ok(rx) => {
                        let _ = chat_svc.update(app, |svc, cx| {
                            svc.attach_tool_stream(
                                rx,
                                model_for_attach,
                                provider_for_loop,
                                request_for_loop,
                                cx,
                            );
                        });
                    }
                    Err(e) => {
                        error!("Stream error: {e}");
                        let _ = chat_svc.update(app, |svc, cx| {
                            svc.set_error(format!("AI request failed: {e}"), cx);
                        });
                    }
                }
            })
        }
    } else {
        // Normal (non-speculative) path
        cx.spawn(async move |_this, app: &mut AsyncApp| {
            let mut enriched_request = request.clone();
            super::enrich_request_with_memory(
                &mut enriched_request,
                &hive_mem_for_async,
                &knowledge_hub_for_async,
                &query_for_memory,
            )
            .await;
            match provider.stream_chat(&enriched_request).await {
                Ok(rx) => {
                    let _ = chat_svc.update(app, |svc, cx| {
                        svc.attach_tool_stream(
                            rx,
                            model_for_attach,
                            provider_for_loop,
                            request_for_loop,
                            cx,
                        );
                    });
                }
                Err(e) => {
                    error!("Stream error: {e}");
                    let _ = chat_svc.update(app, |svc, cx| {
                        svc.set_error(format!("AI request failed: {e}"), cx);
                    });
                }
            }
        })
    };

    workspace._stream_task = Some(task);
    workspace.chat_input.update(cx, |input, cx| {
        input.set_sending(true, window, cx);
    });

    info!("Send initiated (model={})", model);
    cx.notify();
}

pub(super) fn try_handle_swarm_send(
    workspace: &mut HiveWorkspace,
    send_text: String,
    model: &str,
    cx: &mut Context<HiveWorkspace>,
) -> bool {
    if !send_text.trim().starts_with("/swarm ") {
        return false;
    }

    let goal = send_text
        .trim()
        .strip_prefix("/swarm ")
        .unwrap_or("")
        .trim()
        .to_string();
    if goal.is_empty() {
        workspace.chat_service.update(cx, |svc, cx| {
            svc.set_error("Usage: /swarm <goal description>".to_string(), cx);
        });
        return true;
    }

    workspace.chat_service.update(cx, |svc, cx| {
        svc.send_message(send_text, model, cx);
    });

    let provider: Option<Arc<dyn AiProvider>> = if cx.has_global::<AppAiService>() {
        cx.global::<AppAiService>().0.first_provider()
    } else {
        None
    };
    let Some(provider) = provider else {
        workspace.chat_service.update(cx, |svc, cx| {
            svc.set_error("No AI provider available for swarm execution", cx);
        });
        return true;
    };

    let memory = if cx.has_global::<AppCollectiveMemory>() {
        Some(Arc::clone(&cx.global::<AppCollectiveMemory>().0))
    } else {
        None
    };

    let rag_service = cx
        .has_global::<AppRagService>()
        .then(|| cx.global::<AppRagService>().0.clone());
    let activity_service = cx
        .has_global::<AppActivityService>()
        .then(|| cx.global::<AppActivityService>().0.clone());
    let notification_service = cx
        .has_global::<AppAgentNotifications>()
        .then(|| cx.global::<AppAgentNotifications>().0.clone());

    let budget_enforcer = if cx.has_global::<AppConfig>() {
        let cfg = cx.global::<AppConfig>().0.get();
        if cfg.daily_budget_usd > 0.0 || cfg.monthly_budget_usd > 0.0 {
            let log_path = hive_core::config::HiveConfig::base_dir()
                .map(|d| d.join("activity.db"))
                .unwrap_or_else(|_| std::path::PathBuf::from("activity.db"));
            if let Ok(log) = hive_agents::ActivityLog::open(&log_path) {
                let budget_config = hive_agents::BudgetConfig {
                    global_daily_limit_usd: if cfg.daily_budget_usd > 0.0 {
                        Some(cfg.daily_budget_usd)
                    } else {
                        None
                    },
                    global_monthly_limit_usd: if cfg.monthly_budget_usd > 0.0 {
                        Some(cfg.monthly_budget_usd)
                    } else {
                        None
                    },
                    ..Default::default()
                };
                Some(Arc::new(hive_agents::BudgetEnforcer::new(
                    budget_config,
                    Arc::new(log),
                )))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let model_for_exec = model.to_string();
    let chat_svc = workspace.chat_service.downgrade();
    cx.spawn(async move |_this, app: &mut AsyncApp| {
        struct ProviderExecutor {
            provider: Arc<dyn AiProvider>,
        }

        impl hive_agents::AiExecutor for ProviderExecutor {
            async fn execute(
                &self,
                request: &hive_ai::types::ChatRequest,
            ) -> Result<hive_ai::types::ChatResponse, String> {
                self.provider.chat(request).await.map_err(|e| e.to_string())
            }
        }

        let executor = Arc::new(ProviderExecutor { provider });

        let mut queen =
            hive_agents::Queen::new(hive_agents::swarm::SwarmConfig::default(), executor);
        if let Some(mem) = memory {
            queen = queen.with_memory(mem);
        }
        if let Some(rag) = rag_service.clone() {
            queen = queen.with_rag(rag);
        }
        if let Some(ref activity) = activity_service {
            queen = queen.with_activity(activity.clone());
        }
        if let Some(ref budget) = budget_enforcer {
            queen = queen.with_budget(budget.clone());
        }
        if let Some(ref notifications) = notification_service {
            queen = queen.with_notifications(notifications.clone());
        }

        let result_text = match queen.execute(&goal).await {
            Ok(result) => {
                use hive_ui_panels::components::task_tree::{
                    TaskDisplay, TaskDisplayStatus, TaskTreeState,
                };

                let tasks: Vec<TaskDisplay> = result
                    .team_results
                    .iter()
                    .map(|tr| {
                        let status = match tr.status {
                            hive_agents::swarm::TeamStatus::Completed => {
                                TaskDisplayStatus::Completed
                            }
                            hive_agents::swarm::TeamStatus::Failed => {
                                TaskDisplayStatus::Failed(tr.error.clone().unwrap_or_default())
                            }
                            hive_agents::swarm::TeamStatus::Running => TaskDisplayStatus::Running,
                            _ => TaskDisplayStatus::Pending,
                        };
                        TaskDisplay {
                            id: tr.team_id.clone(),
                            description: tr.team_name.clone(),
                            persona: "Swarm".into(),
                            status,
                            duration_ms: Some(tr.duration_ms),
                            cost: Some(tr.cost),
                            output_preview: tr.inner.as_ref().map(|i| {
                                let s = match i {
                                    hive_agents::swarm::InnerResult::Native { content, .. }
                                    | hive_agents::swarm::InnerResult::SingleShot {
                                        content, ..
                                    } => content.clone(),
                                    _ => String::new(),
                                };
                                s.chars().take(200).collect()
                            }),
                            expanded: false,
                            model_override: None,
                        }
                    })
                    .collect();
                let tree = TaskTreeState {
                    title: format!("Swarm: {}", &result.goal),
                    plan_id: result.run_id.clone(),
                    tasks,
                    collapsed: false,
                    total_cost: result.total_cost,
                    elapsed_ms: result.total_duration_ms,
                };
                let _ = _this.update(app, |ws, _cx| {
                    ws.swarm_task_trees.push(tree);
                });

                format!(
                    "## Swarm Result\n\n\
                     **Goal:** {}\n\
                     **Status:** {:?}\n\
                     **Teams:** {}\n\
                     **Cost:** ${:.4}\n\
                     **Duration:** {}ms\n\n\
                     ---\n\n{}",
                    result.goal,
                    result.status,
                    result.team_results.len(),
                    result.total_cost,
                    result.total_duration_ms,
                    result.synthesized_output,
                )
            }
            Err(e) => format!("Swarm execution failed: {e}"),
        };

        let _ = app.update(|cx| {
            if let Some(svc) = chat_svc.upgrade() {
                svc.update(cx, |svc, _cx| {
                    let idx = svc.messages.len().saturating_sub(1);
                    svc.finalize_stream(idx, &result_text, &model_for_exec, None);
                });
            }
        });
    })
    .detach();

    true
}

fn selected_context_prompt(workspace: &HiveWorkspace, cx: &App) -> String {
    let mut prompt = String::new();
    if !cx.has_global::<AppContextSelection>() {
        return prompt;
    }

    let selection = cx.global::<AppContextSelection>().0.clone();
    if let Ok(guard) = selection.lock() {
        for path in &guard.selected_files {
            let relative_path = path
                .strip_prefix(&workspace.current_project_root)
                .unwrap_or(path);
            let content = std::fs::read_to_string(path).unwrap_or_default();
            let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
            prompt.push_str(&format!(
                "## {}\n```{}\n{}\n```\n\n",
                relative_path.display(),
                extension,
                content
            ));
        }
    }

    prompt
}
