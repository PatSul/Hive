use gpui::*;
use tracing::{info, warn};

use super::{
    AppMarketplace, AppSkillManager, HiveWorkspace, SkillsAddSource, SkillsClearSearch,
    SkillsCreate, SkillsInstall, SkillsRefresh, SkillsRemove, SkillsRemoveSource,
    SkillsSetCategory, SkillsSetSearch, SkillsSetTab, SkillsToggle,
};

pub(super) fn refresh_skills_data(workspace: &mut HiveWorkspace, cx: &App) {
    use hive_ui_panels::panels::skills::{
        DirectorySkill, InstalledSkill as UiSkill, SkillCategory as UiCat, SkillSource as UiSource,
    };

    let mut installed = Vec::new();

    if cx.has_global::<hive_ui_core::AppSkills>() {
        for skill in cx.global::<hive_ui_core::AppSkills>().0.list() {
            installed.push(UiSkill {
                id: format!("builtin:{}", skill.name),
                name: skill.name.clone(),
                description: skill.description.clone(),
                version: "built-in".into(),
                enabled: skill.enabled,
                integrity_hash: skill.integrity_hash.clone(),
            });
        }
    }

    if cx.has_global::<AppSkillManager>() {
        let manager = &cx.global::<AppSkillManager>().0;
        if let Ok(user_skills) = manager.list() {
            for skill in user_skills {
                installed.push(UiSkill {
                    id: format!("user:{}", skill.name),
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                    version: "custom".into(),
                    enabled: skill.enabled,
                    integrity_hash: String::new(),
                });
            }
        }
    }

    let mut installed_triggers: Vec<String> = Vec::new();
    if cx.has_global::<AppMarketplace>() {
        let marketplace = &cx.global::<AppMarketplace>().0;
        for skill in marketplace.list_installed() {
            installed.push(UiSkill {
                id: skill.id.clone(),
                name: skill.name.clone(),
                description: skill.description.clone(),
                version: skill.installed_at.format("%Y-%m-%d").to_string(),
                enabled: skill.enabled,
                integrity_hash: skill.integrity_hash.clone(),
            });
            installed_triggers.push(skill.trigger.clone());
        }

        workspace.skills_data.sources = marketplace
            .list_sources()
            .iter()
            .map(|source| UiSource {
                url: source.url.clone(),
                name: source.name.clone(),
                skill_count: 0,
            })
            .collect();
    }

    if cx.has_global::<AppMarketplace>() {
        let marketplace = &cx.global::<AppMarketplace>().0;
        workspace.skills_data.installed_plugins = marketplace
            .installed_plugins()
            .iter()
            .map(|plugin| {
                use hive_ui_panels::panels::skills::{UiInstalledPlugin, UiPluginSkill};

                UiInstalledPlugin {
                    id: plugin.id.clone(),
                    name: plugin.name.clone(),
                    version: plugin.version.clone(),
                    author: plugin.author.name.clone(),
                    description: plugin.description.clone(),
                    skills: plugin
                        .skills
                        .iter()
                        .map(|skill| UiPluginSkill {
                            name: skill.name.clone(),
                            description: skill.description.clone(),
                            enabled: skill.enabled,
                        })
                        .collect(),
                    expanded: false,
                    update_available: None,
                }
            })
            .collect();
    }

    let catalog = hive_agents::skill_marketplace::SkillMarketplace::default_directory();
    let mut directory = Vec::new();
    for (index, available) in catalog.iter().enumerate() {
        use hive_agents::skill_marketplace::SkillCategory as MarketplaceCategory;

        let ui_category = match available.category {
            MarketplaceCategory::CodeGeneration => UiCat::CodeQuality,
            MarketplaceCategory::Documentation => UiCat::Documentation,
            MarketplaceCategory::Testing => UiCat::Testing,
            MarketplaceCategory::Security => UiCat::Security,
            MarketplaceCategory::Refactoring => UiCat::Productivity,
            MarketplaceCategory::Analysis => UiCat::Other,
            MarketplaceCategory::Communication => UiCat::Productivity,
            MarketplaceCategory::Custom => UiCat::Other,
        };
        let author = if available.repo_url.contains("anthropic.com") {
            "Anthropic"
        } else if available.repo_url.contains("openai.com") {
            "OpenAI"
        } else if available.repo_url.contains("google.dev") {
            "Google"
        } else if available.repo_url.contains("hive-community") {
            "Community"
        } else {
            "ClawdHub"
        };
        let is_installed = installed_triggers.contains(&available.trigger);
        directory.push(DirectorySkill {
            id: available.name.clone(),
            name: available.name.clone(),
            description: available.description.clone(),
            author: author.to_string(),
            version: "1.0.0".to_string(),
            downloads: (12_400 - index * 300).max(800),
            rating: 4.8 - (index as f32 * 0.03),
            category: ui_category,
            installed: is_installed,
        });
    }
    workspace.skills_data.directory = directory;
    workspace.skills_data.installed = installed;

    if workspace.skills_data.sources.is_empty() {
        let clawdhub_count = catalog
            .iter()
            .filter(|skill| skill.repo_url.contains("clawdhub.hive.dev"))
            .count();
        let anthropic_count = catalog
            .iter()
            .filter(|skill| skill.repo_url.contains("anthropic.com"))
            .count();
        let openai_count = catalog
            .iter()
            .filter(|skill| skill.repo_url.contains("openai.com"))
            .count();
        let google_count = catalog
            .iter()
            .filter(|skill| skill.repo_url.contains("google.dev"))
            .count();
        let community_count = catalog
            .iter()
            .filter(|skill| skill.repo_url.contains("hive-community"))
            .count();
        workspace.skills_data.sources.extend([
            UiSource {
                url: "https://clawdhub.hive.dev/registry".into(),
                name: "ClawdHub".into(),
                skill_count: clawdhub_count,
            },
            UiSource {
                url: "https://skills.anthropic.com".into(),
                name: "Anthropic Official".into(),
                skill_count: anthropic_count,
            },
            UiSource {
                url: "https://skills.openai.com".into(),
                name: "OpenAI Official".into(),
                skill_count: openai_count,
            },
            UiSource {
                url: "https://skills.google.dev".into(),
                name: "Google Official".into(),
                skill_count: google_count,
            },
            UiSource {
                url: "https://github.com/hive-community/skills".into(),
                name: "Community".into(),
                skill_count: community_count,
            },
        ]);
    }
}

pub(super) fn handle_skills_refresh(
    workspace: &mut HiveWorkspace,
    _action: &SkillsRefresh,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("Skills: refresh");
    refresh_skills_data(workspace, cx);
    cx.notify();
}

pub(super) fn handle_skills_install(
    workspace: &mut HiveWorkspace,
    action: &SkillsInstall,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("ClawdHub: install skill {}", action.skill_id);

    let catalog = hive_agents::skill_marketplace::SkillMarketplace::default_directory();
    if let Some(available) = catalog.iter().find(|skill| skill.name == action.skill_id) {
        if cx.has_global::<AppMarketplace>() {
            let marketplace = &mut cx.global_mut::<AppMarketplace>().0;
            let prompt = format!(
                "You are an expert assistant for: {}. {}",
                available.name, available.description
            );
            if let Err(e) = marketplace.install_skill(
                &available.name,
                &available.trigger,
                available.category,
                &prompt,
                Some(&available.repo_url),
            ) {
                warn!("Failed to install skill {}: {e}", available.name);
            }
        }
    } else {
        warn!("Skill '{}' not found in catalog", action.skill_id);
    }

    refresh_skills_data(workspace, cx);
    cx.notify();
}

pub(super) fn handle_skills_remove(
    workspace: &mut HiveWorkspace,
    action: &SkillsRemove,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("ClawdHub: remove skill {}", action.skill_id);

    if action.skill_id.starts_with("builtin:") {
        let name = action
            .skill_id
            .strip_prefix("builtin:")
            .unwrap_or(&action.skill_id);
        if cx.has_global::<hive_ui_core::AppSkills>() {
            cx.global_mut::<hive_ui_core::AppSkills>().0.uninstall(name);
        }
    } else if cx.has_global::<AppMarketplace>() {
        let marketplace = &mut cx.global_mut::<AppMarketplace>().0;
        if let Err(e) = marketplace.remove_skill(&action.skill_id) {
            warn!("Failed to remove skill {}: {e}", action.skill_id);
        }
    }

    refresh_skills_data(workspace, cx);
    cx.notify();
}

pub(super) fn handle_skills_toggle(
    workspace: &mut HiveWorkspace,
    action: &SkillsToggle,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("ClawdHub: toggle skill {}", action.skill_id);

    if action.skill_id.starts_with("builtin:") {
        let name = action
            .skill_id
            .strip_prefix("builtin:")
            .unwrap_or(&action.skill_id);
        if cx.has_global::<hive_ui_core::AppSkills>() {
            cx.global_mut::<hive_ui_core::AppSkills>().0.toggle(name);
        }
    } else if cx.has_global::<AppMarketplace>() {
        let marketplace = &mut cx.global_mut::<AppMarketplace>().0;
        if let Err(e) = marketplace.toggle_skill(&action.skill_id) {
            warn!("Failed to toggle skill {}: {e}", action.skill_id);
        }
    }

    refresh_skills_data(workspace, cx);
    cx.notify();
}

pub(super) fn handle_skills_create(
    workspace: &mut HiveWorkspace,
    action: &SkillsCreate,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("ClawdHub: create skill '{}'", action.name);

    if cx.has_global::<hive_ui_core::AppSkills>() {
        let registry = &mut cx.global_mut::<hive_ui_core::AppSkills>().0;
        if let Err(e) = registry.install(
            action.name.clone(),
            action.description.clone(),
            action.instructions.clone(),
            hive_agents::skills::SkillSource::Custom,
        ) {
            warn!("Failed to create skill '{}': {e}", action.name);
        }
    }

    workspace.skills_data.create_draft = hive_ui_panels::panels::skills::CreateSkillDraft::empty();
    refresh_skills_data(workspace, cx);
    cx.notify();
}

pub(super) fn handle_skills_add_source(
    workspace: &mut HiveWorkspace,
    action: &SkillsAddSource,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("ClawdHub: add source '{}'", action.url);

    if !action.url.is_empty() && cx.has_global::<AppMarketplace>() {
        let marketplace = &mut cx.global_mut::<AppMarketplace>().0;
        if let Err(e) = marketplace.add_source(&action.url, &action.name) {
            warn!("Failed to add source '{}': {e}", action.url);
        }
    }

    refresh_skills_data(workspace, cx);
    cx.notify();
}

pub(super) fn handle_skills_remove_source(
    workspace: &mut HiveWorkspace,
    action: &SkillsRemoveSource,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    info!("ClawdHub: remove source '{}'", action.url);

    if cx.has_global::<AppMarketplace>() {
        let marketplace = &mut cx.global_mut::<AppMarketplace>().0;
        if let Err(e) = marketplace.remove_source(&action.url) {
            warn!("Failed to remove source '{}': {e}", action.url);
        }
    }

    refresh_skills_data(workspace, cx);
    cx.notify();
}

pub(super) fn handle_skills_set_tab(
    workspace: &mut HiveWorkspace,
    action: &SkillsSetTab,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_ui_panels::panels::skills::SkillsTab;

    info!("ClawdHub: switch tab to '{}'", action.tab);
    workspace.skills_data.active_tab = match action.tab.as_str() {
        "Installed" => SkillsTab::Installed,
        "Directory" => SkillsTab::Directory,
        "Create" => SkillsTab::Create,
        "Add Source" => SkillsTab::AddSource,
        _ => SkillsTab::Installed,
    };
    cx.notify();
}

pub(super) fn handle_skills_set_search(
    workspace: &mut HiveWorkspace,
    action: &SkillsSetSearch,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.skills_data.search_query = action.query.clone();
    cx.notify();
}

pub(super) fn handle_skills_clear_search(
    workspace: &mut HiveWorkspace,
    _action: &SkillsClearSearch,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    workspace.skills_data.search_query.clear();
    cx.notify();
}

pub(super) fn handle_skills_set_category(
    workspace: &mut HiveWorkspace,
    action: &SkillsSetCategory,
    _window: &mut Window,
    cx: &mut Context<HiveWorkspace>,
) {
    use hive_ui_panels::panels::skills::SkillCategory;

    info!("ClawdHub: set category filter to '{}'", action.category);
    workspace.skills_data.selected_category = match action.category.as_str() {
        "All" => None,
        "Code Quality" => Some(SkillCategory::CodeQuality),
        "Testing" => Some(SkillCategory::Testing),
        "DevOps" => Some(SkillCategory::DevOps),
        "Security" => Some(SkillCategory::Security),
        "Documentation" => Some(SkillCategory::Documentation),
        "Database" => Some(SkillCategory::Database),
        "Productivity" => Some(SkillCategory::Productivity),
        "Other" => Some(SkillCategory::Other),
        _ => None,
    };
    cx.notify();
}
