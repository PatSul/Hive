use gpui::*;
use gpui_component::input::{Input, InputState};
use gpui_component::{Icon, IconName};

use hive_ui_core::{
    AgentsDiscoverRemoteAgent, AgentsRefreshRemoteAgents, AgentsReloadWorkflows,
    AgentsRunRemoteAgent, AgentsRunWorkflow, AgentsSelectRemoteAgent,
    AgentsSelectRemoteSkill,
};
use hive_ui_core::HiveTheme;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Display information for an agent persona.
#[derive(Debug, Clone)]
pub struct PersonaDisplay {
    pub name: String,
    pub kind: String,
    pub description: String,
    pub model_tier: String,
    pub active: bool,
}

impl PersonaDisplay {
    /// Icon name mapped from the persona kind.
    pub fn icon(&self) -> IconName {
        match self.kind.as_str() {
            "investigate" => IconName::Search,
            "implement" => IconName::File,
            "verify" => IconName::CircleCheck,
            "critique" => IconName::Info,
            "debug" => IconName::TriangleAlert,
            "code_review" => IconName::Eye,
            "remote_a2a" => IconName::Globe,
            _ => IconName::Bot,
        }
    }
}

/// Display information for an orchestration run.
#[derive(Debug, Clone)]
pub struct RunDisplay {
    pub id: String,
    pub spec_title: String,
    pub status: String,
    pub progress: f32,
    pub tasks_done: usize,
    pub tasks_total: usize,
    pub cost: f64,
    pub elapsed: String,
    pub tasks: Vec<crate::components::task_tree::TaskDisplay>,
}

impl RunDisplay {
    /// Whether this run is still in progress.
    pub fn is_active(&self) -> bool {
        self.status == "Running" || self.status == "Pending"
    }

    /// Whether this run has task-level detail available for drill-down.
    pub fn has_task_detail(&self) -> bool {
        !self.tasks.is_empty()
    }
}

/// Display information for a runnable automation workflow.
#[derive(Debug, Clone)]
pub struct WorkflowDisplay {
    pub id: String,
    pub name: String,
    pub description: String,
    pub commands: Vec<String>,
    pub source: String,
    pub status: String,
    pub trigger: String,
    pub steps: usize,
    pub run_count: usize,
    pub last_run: Option<String>,
}

/// Display information for a configured outbound A2A agent.
#[derive(Debug, Clone)]
pub struct RemoteAgentDisplay {
    pub name: String,
    pub url: String,
    pub description: String,
    pub discovered: bool,
    pub api_key_configured: bool,
    pub version: Option<String>,
    pub skills: Vec<String>,
}

/// Display information for a remote A2A task run.
#[derive(Debug, Clone)]
pub struct RemoteTaskDisplay {
    pub agent_name: String,
    pub task_id: String,
    pub state: String,
    pub skill_id: Option<String>,
    pub output: String,
    pub completed_at: String,
    pub error: Option<String>,
}

/// All data needed to render the agents panel.
#[derive(Debug, Clone)]
pub struct AgentsPanelData {
    pub personas: Vec<PersonaDisplay>,
    pub remote_agents: Vec<RemoteAgentDisplay>,
    pub selected_remote_agent: Option<String>,
    pub selected_remote_skill: Option<String>,
    pub remote_status: Option<String>,
    pub remote_busy: bool,
    pub remote_run_history: Vec<RemoteTaskDisplay>,
    pub workflows: Vec<WorkflowDisplay>,
    pub active_runs: Vec<RunDisplay>,
    pub run_history: Vec<RunDisplay>,
    pub workflow_source_dir: String,
    pub workflow_hint: Option<String>,
    pub remote_hint: Option<String>,
}

impl AgentsPanelData {
    /// Create an empty state.
    pub fn empty() -> Self {
        Self {
            personas: Vec::new(),
            remote_agents: Vec::new(),
            selected_remote_agent: None,
            selected_remote_skill: None,
            remote_status: None,
            remote_busy: false,
            remote_run_history: Vec::new(),
            workflows: Vec::new(),
            active_runs: Vec::new(),
            run_history: Vec::new(),
            workflow_source_dir: ".hive/workflows".into(),
            workflow_hint: None,
            remote_hint: None,
        }
    }

    /// Return a sample dataset with the six default personas.
    #[allow(dead_code)]
    pub fn sample() -> Self {
        Self {
            personas: vec![
                PersonaDisplay {
                    name: "Investigator".into(),
                    kind: "investigate".into(),
                    description: "Analyzes codebases, traces bugs, and gathers context for tasks."
                        .into(),
                    model_tier: "Tier 1".into(),
                    active: true,
                },
                PersonaDisplay {
                    name: "Implementer".into(),
                    kind: "implement".into(),
                    description:
                        "Writes production code, applies patches, and implements features.".into(),
                    model_tier: "Tier 1".into(),
                    active: true,
                },
                PersonaDisplay {
                    name: "Verifier".into(),
                    kind: "verify".into(),
                    description: "Runs tests, validates behavior, and checks correctness.".into(),
                    model_tier: "Tier 2".into(),
                    active: true,
                },
                PersonaDisplay {
                    name: "Critic".into(),
                    kind: "critique".into(),
                    description:
                        "Reviews plans and code for flaws, edge cases, and improvements.".into(),
                    model_tier: "Tier 2".into(),
                    active: true,
                },
                PersonaDisplay {
                    name: "Debugger".into(),
                    kind: "debug".into(),
                    description:
                        "Isolates failures, inspects stack traces, and proposes fixes.".into(),
                    model_tier: "Tier 1".into(),
                    active: false,
                },
                PersonaDisplay {
                    name: "Code Reviewer".into(),
                    kind: "code_review".into(),
                    description:
                        "Performs detailed code review with style, correctness, and security checks."
                            .into(),
                    model_tier: "Tier 2".into(),
                    active: false,
                },
            ],
            remote_agents: vec![RemoteAgentDisplay {
                name: "Remote Builder".into(),
                url: "http://localhost:8088".into(),
                description: "Executes focused build and review tasks over A2A.".into(),
                discovered: true,
                api_key_configured: true,
                version: Some("1.2.3".into()),
                skills: vec!["build".into(), "review".into()],
            }],
            selected_remote_agent: Some("Remote Builder".into()),
            selected_remote_skill: Some("review".into()),
            remote_status: Some("1 remote agent ready for manual runs".into()),
            remote_busy: false,
            remote_run_history: vec![RemoteTaskDisplay {
                agent_name: "Remote Builder".into(),
                task_id: "remote-task-1".into(),
                state: "Completed".into(),
                skill_id: Some("review".into()),
                output: "Reviewed the current patch set and returned a short finding summary."
                    .into(),
                completed_at: "2026-02-13 13:50".into(),
                error: None,
            }],
            workflows: vec![
                WorkflowDisplay {
                    id: "builtin:hive-dogfood-v1".into(),
                    name: "Local Build Check".into(),
                    description: "Run a local validation loop: check, test, and inspect state."
                        .into(),
                    commands: vec![
                        "cargo check --quiet".into(),
                        "cargo test --quiet -p hive_app".into(),
                        "git status --short".into(),
                        "git diff --stat".into(),
                    ],
                    source: "Built-in".into(),
                    status: "Active".into(),
                    trigger: "Manual".into(),
                    steps: 4,
                    run_count: 2,
                    last_run: Some("2026-02-13 13:44".into()),
                },
                WorkflowDisplay {
                    id: "file:project-ci".into(),
                    name: "Project CI".into(),
                    description: "Run lint/check/test before merge.".into(),
                    commands: vec![
                        "cargo fmt --check".into(),
                        "cargo test --all".into(),
                        "cargo clippy".into(),
                    ],
                    source: "User file".into(),
                    status: "Draft".into(),
                    trigger: "Manual".into(),
                    steps: 3,
                    run_count: 0,
                    last_run: None,
                },
            ],
            active_runs: vec![RunDisplay {
                id: "run-001".into(),
                spec_title: "Authentication Overhaul".into(),
                status: "Running".into(),
                progress: 0.58,
                tasks_done: 7,
                tasks_total: 12,
                cost: 0.42,
                elapsed: "3m 22s".into(),
                tasks: vec![],
            }],
            run_history: vec![
                RunDisplay {
                    id: "run-000".into(),
                    spec_title: "Database Migration v2".into(),
                    status: "Complete".into(),
                    progress: 1.0,
                    tasks_done: 5,
                    tasks_total: 5,
                    cost: 0.18,
                    elapsed: "1m 47s".into(),
                    tasks: vec![],
                },
            ],
            workflow_source_dir: ".hive/workflows".into(),
            workflow_hint: Some("2 workflows loaded (1 active)".into()),
            remote_hint: Some("Configured in ~/.hive/a2a.toml".into()),
        }
    }
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

/// Multi-agent orchestration panel: active runs, persona grid.
pub struct AgentsPanel;

impl AgentsPanel {
    pub fn render(
        data: &AgentsPanelData,
        remote_prompt_input: &Entity<InputState>,
        theme: &HiveTheme,
    ) -> impl IntoElement {
        div()
            .id("agents-panel")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p(theme.space_4)
            .gap(theme.space_4)
            .child(render_header(data, theme))
            .child(render_remote_agents_section(data, remote_prompt_input, theme))
            .child(render_workflows_section(data, theme))
            .child(render_active_runs_section(&data.active_runs, theme))
            .child(render_run_history_section(&data.run_history, theme))
            .child(render_personas_section(&data.personas, theme))
    }
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn render_header(data: &AgentsPanelData, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(theme.space_3)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_3)
                .child(header_icon(theme))
                .child(header_title(theme))
                .child(div().flex_1())
                .child(refresh_remote_agents_button(theme))
                .child(reload_workflows_button(theme)),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child(
                    data.workflow_hint.clone().unwrap_or_else(|| {
                        format!("User workflows are loaded from {}", data.workflow_source_dir)
                    }),
                ),
        )
        .into_any_element()
}

fn refresh_remote_agents_button(theme: &HiveTheme) -> AnyElement {
    div()
        .id("agents-refresh-remote")
        .flex()
        .items_center()
        .justify_center()
        .px(theme.space_3)
        .py(theme.space_1)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .text_size(theme.font_size_sm)
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.text_primary)
        .hover(|style| style.bg(theme.bg_tertiary))
        .on_mouse_down(MouseButton::Left, |_event, window, cx| {
            window.dispatch_action(Box::new(AgentsRefreshRemoteAgents), cx);
        })
        .child("Refresh Remote Agents")
        .into_any_element()
}

fn header_icon(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .w(px(40.0))
        .h(px(40.0))
        .rounded(theme.radius_lg)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(Icon::new(IconName::Bot).size_4())
}

fn header_title(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .text_size(theme.font_size_xl)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::BOLD)
                .child("Agent Orchestration"),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child("Coordinate multi-agent runs on specifications"),
        )
}

fn reload_workflows_button(theme: &HiveTheme) -> AnyElement {
    div()
        .id("agents-reload-workflows")
        .flex()
        .items_center()
        .justify_center()
        .px(theme.space_3)
        .py(theme.space_1)
        .rounded(theme.radius_md)
        .bg(theme.accent_cyan)
        .text_size(theme.font_size_sm)
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.text_on_accent)
        .hover(|style| style.bg(theme.accent_aqua))
        .on_mouse_down(MouseButton::Left, |_event, window, cx| {
            window.dispatch_action(Box::new(AgentsReloadWorkflows), cx);
        })
        .child("Reload Workflows")
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Remote A2A Agents
// ---------------------------------------------------------------------------

fn render_remote_agents_section(
    data: &AgentsPanelData,
    remote_prompt_input: &Entity<InputState>,
    theme: &HiveTheme,
) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_3)
        .child(section_title("Remote A2A Agents", data.remote_agents.len(), theme));

    if let Some(hint) = &data.remote_hint {
        section = section.child(section_note(hint.clone(), theme));
    }

    if let Some(status) = &data.remote_status {
        section = section.child(remote_status_banner(status, data.remote_busy, theme));
    }

    if data.remote_agents.is_empty() {
        return section
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(theme.space_1)
                    .p(theme.space_4)
                    .rounded(theme.radius_md)
                    .bg(theme.bg_surface)
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_size(theme.font_size_sm)
                            .text_color(theme.text_secondary)
                            .child("No configured remote A2A agents."),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .child(
                                "Add agent entries to ~/.hive/a2a.toml, then click Refresh Remote Agents.",
                            ),
                    ),
            )
            .into_any_element();
    }

    let mut list = div().flex().flex_col().gap(theme.space_2);
    for agent in &data.remote_agents {
        list = list.child(render_remote_agent_card(
            agent,
            data.selected_remote_agent.as_deref() == Some(agent.name.as_str()),
            theme,
        ));
    }

    section
        .child(list)
        .child(render_remote_prompt_card(data, remote_prompt_input, theme))
        .child(render_remote_run_history_section(
            &data.remote_run_history,
            theme,
        ))
        .into_any_element()
}

fn render_remote_agent_card(
    agent: &RemoteAgentDisplay,
    selected: bool,
    theme: &HiveTheme,
) -> AnyElement {
    let border_color = if selected {
        theme.accent_cyan
    } else {
        theme.border
    };
    let discover_color = if agent.discovered {
        theme.accent_green
    } else {
        theme.accent_yellow
    };
    let select_agent_name = agent.name.clone();
    let discover_agent_name = agent.name.clone();

    let mut card = div()
        .id(ElementId::Name(
            format!("remote-agent-{}", agent.name.replace(' ', "-")).into(),
        ))
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .p(theme.space_3)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(border_color)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .child(
                    Icon::new(IconName::Globe)
                        .size_4()
                        .text_color(theme.accent_cyan),
                )
                .child(
                    div()
                        .text_size(theme.font_size_base)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(agent.name.clone()),
                )
                .child(
                    div()
                        .px(theme.space_2)
                        .py(px(2.0))
                        .rounded(theme.radius_full)
                        .bg(theme.bg_primary)
                        .text_size(theme.font_size_xs)
                        .text_color(discover_color)
                        .child(if agent.discovered {
                            "Discovered"
                        } else {
                            "Needs Discovery"
                        }),
                )
                .child(
                    div()
                        .px(theme.space_2)
                        .py(px(2.0))
                        .rounded(theme.radius_full)
                        .bg(theme.bg_primary)
                        .text_size(theme.font_size_xs)
                        .text_color(if agent.api_key_configured {
                            theme.accent_green
                        } else {
                            theme.text_muted
                        })
                        .child(if agent.api_key_configured {
                            "Auth"
                        } else {
                            "No Key"
                        }),
                )
                .child(div().flex_1())
                .child(
                    control_button(
                        if selected { "Selected" } else { "Select" },
                        if selected {
                            theme.accent_green
                        } else {
                            theme.accent_aqua
                        },
                        theme.text_on_accent,
                        move |_event, window, cx| {
                            window.dispatch_action(
                                Box::new(AgentsSelectRemoteAgent {
                                    agent_name: select_agent_name.clone(),
                                }),
                                cx,
                            );
                        },
                        theme,
                    ),
                )
                .child(
                    control_button(
                        "Discover",
                        theme.bg_primary,
                        theme.text_primary,
                        move |_event, window, cx| {
                            window.dispatch_action(
                                Box::new(AgentsDiscoverRemoteAgent {
                                    agent_name: discover_agent_name.clone(),
                                }),
                                cx,
                            );
                        },
                        theme,
                    ),
                ),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_secondary)
                .child(agent.description.clone()),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(agent.url.clone()),
        );

    if let Some(version) = agent.version.as_ref() {
        card = card.child(
            div()
                .px(theme.space_2)
                .py(px(2.0))
                .rounded(theme.radius_full)
                .bg(theme.bg_primary)
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(format!("v{version}")),
        );
    }

    if agent.skills.is_empty() {
        card = card.child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child("No advertised skills yet. Discover the agent to refresh its card."),
        );
    } else {
        let mut chips = div().flex().flex_row().flex_wrap().gap(theme.space_1);
        for skill in &agent.skills {
            chips = chips.child(
                div()
                    .px(theme.space_2)
                    .py(px(2.0))
                    .rounded(theme.radius_full)
                    .bg(theme.bg_primary)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_secondary)
                    .child(skill.clone()),
            );
        }
        card = card.child(chips);
    }

    card.into_any_element()
}

fn render_remote_prompt_card(
    data: &AgentsPanelData,
    remote_prompt_input: &Entity<InputState>,
    theme: &HiveTheme,
) -> AnyElement {
    let Some(selected_agent_name) = data.selected_remote_agent.as_ref() else {
        return div()
            .flex()
            .flex_col()
            .gap(theme.space_2)
            .p(theme.space_4)
            .rounded(theme.radius_md)
            .bg(theme.bg_surface)
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .text_size(theme.font_size_sm)
                    .text_color(theme.text_secondary)
                    .child("Select a remote agent to run a task."),
            )
            .into_any_element();
    };

    let Some(selected_agent) = data
        .remote_agents
        .iter()
        .find(|agent| agent.name == *selected_agent_name)
    else {
        return div().into_any_element();
    };

    let run_agent_name = selected_agent.name.clone();
    let run_skill_id = data.selected_remote_skill.clone();
    let run_busy = data.remote_busy;
    let prompt_input_for_run = remote_prompt_input.clone();

    let mut skill_row = div().flex().flex_row().flex_wrap().gap(theme.space_1);
    skill_row = skill_row.child(render_skill_chip(
        selected_agent.name.clone(),
        None,
        data.selected_remote_skill.is_none(),
        theme,
    ));
    for skill in &selected_agent.skills {
        skill_row = skill_row.child(render_skill_chip(
            selected_agent.name.clone(),
            Some(skill.clone()),
            data.selected_remote_skill.as_deref() == Some(skill.as_str()),
            theme,
        ));
    }

    div()
        .flex()
        .flex_col()
        .gap(theme.space_3)
        .p(theme.space_4)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .child(
                    div()
                        .text_size(theme.font_size_base)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("Run {}", selected_agent.name)),
                )
                .child(
                    div()
                        .px(theme.space_2)
                        .py(px(2.0))
                        .rounded(theme.radius_full)
                        .bg(theme.bg_primary)
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(if data.selected_remote_skill.is_some() {
                            "Pinned skill"
                        } else {
                            "Auto skill"
                        }),
                ),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child("Prompt"),
        )
        .child(
            Input::new(remote_prompt_input)
                .text_size(theme.font_size_sm)
                .cleanable(true),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child("Skill"),
        )
        .child(skill_row)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(theme.space_3)
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(if selected_agent.discovered {
                            "The task will use the cached agent card and selected skill."
                        } else {
                            "This agent has not been discovered yet. Run will discover it first."
                        }),
                )
                .child(
                    control_button(
                        if run_busy { "Running..." } else { "Run Remote Task" },
                        if run_busy {
                            theme.bg_primary
                        } else {
                            theme.accent_cyan
                        },
                        if run_busy {
                            theme.text_muted
                        } else {
                            theme.text_on_accent
                        },
                        move |_event, window, cx| {
                            if run_busy {
                                return;
                            }
                            let prompt = prompt_input_for_run.read(cx).value().to_string();
                            window.dispatch_action(
                                Box::new(AgentsRunRemoteAgent {
                                    agent_name: run_agent_name.clone(),
                                    prompt,
                                    skill_id: run_skill_id.clone(),
                                }),
                                cx,
                            );
                        },
                        theme,
                    ),
                ),
        )
        .into_any_element()
}

fn render_skill_chip(
    agent_name: String,
    skill_id: Option<String>,
    selected: bool,
    theme: &HiveTheme,
) -> AnyElement {
    let chip_label = skill_id.clone().unwrap_or_else(|| "Auto".into());
    div()
        .id(ElementId::Name(
            format!(
                "remote-skill-{}-{}",
                agent_name.replace(' ', "-"),
                chip_label.replace(' ', "-")
            )
            .into(),
        ))
        .px(theme.space_2)
        .py(px(3.0))
        .rounded(theme.radius_full)
        .bg(if selected {
            theme.accent_aqua
        } else {
            theme.bg_primary
        })
        .text_size(theme.font_size_xs)
        .font_weight(FontWeight::MEDIUM)
        .text_color(if selected {
            theme.text_on_accent
        } else {
            theme.text_secondary
        })
        .hover(|style| {
            if selected {
                style.bg(theme.accent_cyan)
            } else {
                style.bg(theme.bg_tertiary)
            }
        })
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            window.dispatch_action(
                Box::new(AgentsSelectRemoteSkill {
                    agent_name: agent_name.clone(),
                    skill_id: skill_id.clone(),
                }),
                cx,
            );
        })
        .child(chip_label)
        .into_any_element()
}

fn render_remote_run_history_section(
    runs: &[RemoteTaskDisplay],
    theme: &HiveTheme,
) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_3)
        .child(section_title("Recent Remote Tasks", runs.len(), theme));

    if runs.is_empty() {
        section = section.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .py(theme.space_4)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("No remote A2A tasks yet."),
                ),
        );
    } else {
        let mut list = div().flex().flex_col().gap(theme.space_2);
        for run in runs {
            list = list.child(render_remote_run_card(run, theme));
        }
        section = section.child(list);
    }

    section.into_any_element()
}

fn render_remote_run_card(run: &RemoteTaskDisplay, theme: &HiveTheme) -> AnyElement {
    let success = run.error.is_none()
        && matches!(run.state.to_ascii_lowercase().as_str(), "completed" | "success");
    let status_color = if success {
        theme.accent_green
    } else if run.error.is_some() {
        theme.accent_red
    } else {
        theme.accent_yellow
    };
    let mut meta = div()
        .flex()
        .flex_row()
        .gap(theme.space_3)
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(format!("Task: {}", run.task_id)),
        );
    if let Some(skill) = run.skill_id.as_ref() {
        meta = meta.child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(format!("Skill: {skill}")),
        );
    }

    div()
        .flex()
        .flex_col()
        .gap(theme.space_2)
        .p(theme.space_3)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .child(
                    div()
                        .text_size(theme.font_size_base)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(run.agent_name.clone()),
                )
                .child(
                    div()
                        .px(theme.space_2)
                        .py(px(2.0))
                        .rounded(theme.radius_full)
                        .bg(theme.bg_primary)
                        .text_size(theme.font_size_xs)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(status_color)
                        .child(if let Some(error) = &run.error {
                            if error.is_empty() {
                                run.state.clone()
                            } else {
                                "Failed".into()
                            }
                        } else {
                            run.state.clone()
                        }),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(run.completed_at.clone()),
                ),
        )
        .child(meta)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(if run.error.is_some() {
                    theme.accent_red
                } else {
                    theme.text_secondary
                })
                .child(
                    run.error
                        .clone()
                        .unwrap_or_else(|| remote_run_excerpt(&run.output)),
                ),
        )
        .into_any_element()
}

fn remote_status_banner(status: &str, busy: bool, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(theme.space_2)
        .px(theme.space_3)
        .py(theme.space_2)
        .rounded(theme.radius_sm)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(if busy {
            theme.accent_cyan
        } else {
            theme.border
        })
        .child(
            div()
                .w(px(8.0))
                .h(px(8.0))
                .rounded(theme.radius_full)
                .bg(if busy {
                    theme.accent_cyan
                } else {
                    theme.accent_green
                }),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(status.to_string()),
        )
        .into_any_element()
}

fn remote_run_excerpt(output: &str) -> String {
    const MAX_LEN: usize = 220;
    let output = output.trim();
    if output.chars().count() <= MAX_LEN {
        output.to_string()
    } else {
        let truncated = output.chars().take(MAX_LEN).collect::<String>();
        format!("{truncated}...")
    }
}

fn section_note(text: String, theme: &HiveTheme) -> AnyElement {
    div()
        .text_size(theme.font_size_xs)
        .text_color(theme.text_muted)
        .child(text)
        .into_any_element()
}

fn control_button<F>(
    label: &str,
    bg: Hsla,
    text_color: Hsla,
    on_click: F,
    theme: &HiveTheme,
) -> AnyElement
where
    F: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
{
    let label = label.to_string();
    div()
        .px(theme.space_2)
        .py(px(3.0))
        .rounded(theme.radius_sm)
        .bg(bg)
        .text_size(theme.font_size_xs)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(text_color)
        .hover(|style| style.opacity(0.9))
        .on_mouse_down(MouseButton::Left, on_click)
        .child(label)
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Workflows
// ---------------------------------------------------------------------------

fn render_workflows_section(data: &AgentsPanelData, theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_3)
        .child(section_title("Automation Workflows", data.workflows.len(), theme));

    if data.workflows.is_empty() {
        section = section.child(
            div()
                .flex()
                .flex_col()
                .gap(theme.space_1)
                .p(theme.space_4)
                .rounded(theme.radius_md)
                .bg(theme.bg_surface)
                .border_1()
                .border_color(theme.border)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_secondary)
                        .child("No workflows loaded."),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(format!(
                            "Add JSON files to {} and click Reload Workflows.",
                            data.workflow_source_dir
                        )),
                ),
        );
    } else {
        let mut list = div().flex().flex_col().gap(theme.space_2);
        for workflow in &data.workflows {
            list = list.child(render_workflow_card(workflow, theme));
        }
        section = section.child(list);
    }

    section.into_any_element()
}

fn render_workflow_card(workflow: &WorkflowDisplay, theme: &HiveTheme) -> AnyElement {
    let status_color = match workflow.status.as_str() {
        "Active" => theme.accent_green,
        "Draft" => theme.accent_yellow,
        "Paused" => theme.accent_cyan,
        "Failed" => theme.accent_red,
        _ => theme.text_muted,
    };

    let run_id = workflow.id.clone();
    let source_id = workflow.id.clone();
    let safe_id = workflow.id.replace(':', "-");

    div()
        .id(ElementId::Name(format!("workflow-{safe_id}").into()))
        .flex()
        .flex_col()
        .p(theme.space_3)
        .gap(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .child(
                    div()
                        .text_size(theme.font_size_base)
                        .text_color(theme.text_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(workflow.name.clone()),
                )
                .child(
                    div()
                        .px(theme.space_2)
                        .py(px(2.0))
                        .rounded(theme.radius_full)
                        .bg(theme.bg_tertiary)
                        .text_size(theme.font_size_xs)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(status_color)
                        .child(workflow.status.clone()),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .px(theme.space_2)
                        .py(px(2.0))
                        .rounded(theme.radius_full)
                        .bg(theme.bg_primary)
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(workflow.source.clone()),
                )
                        .child(
                            div()
                                .id(ElementId::Name(format!("run-workflow-{safe_id}").into()))
                                .px(theme.space_2)
                                .py(px(3.0))
                        .rounded(theme.radius_sm)
                        .bg(theme.accent_aqua)
                        .text_size(theme.font_size_xs)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_on_accent)
                        .hover(|style| style.bg(theme.accent_cyan))
                                .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                                    window.dispatch_action(
                                        Box::new(AgentsRunWorkflow {
                                            workflow_id: run_id.clone(),
                                            instruction: String::new(),
                                            source: "workflow".into(),
                                            source_id: source_id.clone(),
                                        }),
                                        cx,
                                    );
                                })
                                .child("Run"),
                ),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_secondary)
                .child(workflow.description.clone()),
        )
        .child(render_workflow_commands(workflow, theme))
        .child(workflow_meta_row(workflow, theme))
        .into_any_element()
}

fn render_workflow_commands(workflow: &WorkflowDisplay, theme: &HiveTheme) -> Div {
    if workflow.commands.is_empty() {
        return div()
            .text_size(theme.font_size_xs)
            .text_color(theme.text_muted)
            .child("No command steps defined.");
    }

    let mut command_rows = div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .text_size(theme.font_size_xs)
        .text_color(theme.text_muted)
        .child("Commands:");

    for command in &workflow.commands {
        command_rows = command_rows.child(
            div()
                .flex()
                .flex_row()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child("•"),
                )
                .child(div().text_color(theme.text_secondary).child(command.clone())),
        );
    }

    command_rows
}

fn workflow_meta_row(workflow: &WorkflowDisplay, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(format!("Trigger: {}", workflow.trigger)),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(format!("Steps: {}", workflow.steps)),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(format!("Runs: {}", workflow.run_count)),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(format!(
                    "Last Run: {}",
                    workflow.last_run.clone().unwrap_or_else(|| "Never".into())
                )),
        )
}

// ---------------------------------------------------------------------------
// Active runs
// ---------------------------------------------------------------------------

fn render_active_runs_section(runs: &[RunDisplay], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_3)
        .child(section_title("Active Runs", runs.len(), theme));

    if runs.is_empty() {
        section = section.child(render_empty_runs(theme));
    } else {
        for run in runs {
            section = section.child(render_run_card(run, theme));
        }
    }

    section.into_any_element()
}

fn render_run_card(run: &RunDisplay, theme: &HiveTheme) -> AnyElement {
    let status_color = match run.status.as_str() {
        "Running" => theme.accent_aqua,
        "Complete" => theme.accent_green,
        "Failed" => theme.accent_red,
        "Pending" => theme.accent_yellow,
        _ => theme.text_muted,
    };

    div()
        .flex()
        .flex_col()
        .p(theme.space_4)
        .gap(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(run_card_top_row(run, status_color, theme))
        .child(run_progress_bar(run, status_color, theme))
        .child(run_card_stats(run, theme))
        .into_any_element()
}

fn run_card_top_row(run: &RunDisplay, status_color: Hsla, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .text_size(theme.font_size_base)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::SEMIBOLD)
                .child(run.spec_title.clone()),
        )
        .child(div().flex_1())
        .child(
            div()
                .px(theme.space_2)
                .py(px(2.0))
                .rounded(theme.radius_full)
                .bg(theme.bg_tertiary)
                .text_size(theme.font_size_xs)
                .font_weight(FontWeight::MEDIUM)
                .text_color(status_color)
                .child(run.status.clone()),
        )
}

fn run_progress_bar(run: &RunDisplay, bar_color: Hsla, theme: &HiveTheme) -> Div {
    let progress = run.progress.clamp(0.0, 1.0);

    div()
        .flex()
        .flex_col()
        .gap(theme.space_1)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child(format!("{}/{} tasks", run.tasks_done, run.tasks_total)),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(bar_color)
                        .child(format!("{}%", (progress * 100.0) as u32)),
                ),
        )
        .child(
            div()
                .w_full()
                .h(px(6.0))
                .rounded(theme.radius_full)
                .bg(theme.bg_tertiary)
                .child(
                    div()
                        .h(px(6.0))
                        .rounded(theme.radius_full)
                        .bg(bar_color)
                        .w(relative(progress)),
                ),
        )
}

fn run_card_stats(run: &RunDisplay, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_4)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_1)
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child("Cost:"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.accent_yellow)
                        .child(format!("${:.2}", run.cost)),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_1)
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .child("Elapsed:"),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_secondary)
                        .child(run.elapsed.clone()),
                ),
        )
}

fn render_empty_runs(theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .py(theme.space_6)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child("No active runs. Click \"Run Spec\" to start one."),
        )
        .into_any_element()
}

fn render_run_history_section(runs: &[RunDisplay], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_3)
        .child(section_title("Recent Workflow Runs", runs.len(), theme));

    if runs.is_empty() {
        section = section.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .py(theme.space_4)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("No workflow runs yet."),
                ),
        );
    } else {
        let mut list = div().flex().flex_col().gap(theme.space_2);
        for run in runs {
            list = list.child(render_run_card(run, theme));
        }
        section = section.child(list);
    }

    section.into_any_element()
}

// ---------------------------------------------------------------------------
// Personas
// ---------------------------------------------------------------------------

fn render_personas_section(personas: &[PersonaDisplay], theme: &HiveTheme) -> AnyElement {
    let mut section = div()
        .flex()
        .flex_col()
        .gap(theme.space_3)
        .child(section_title("Agent Personas", personas.len(), theme));

    if personas.is_empty() {
        section = section.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .py(theme.space_6)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("No personas configured."),
                ),
        );
    } else {
        let mut grid = div().flex().flex_row().flex_wrap().gap(theme.space_3);

        for persona in personas {
            grid = grid.child(render_persona_card(persona, theme));
        }

        section = section.child(grid);
    }

    // Custom personas section
    section = section.child(custom_personas_header(theme));

    section.into_any_element()
}

fn render_persona_card(persona: &PersonaDisplay, theme: &HiveTheme) -> AnyElement {
    let border_color = if persona.active {
        theme.accent_aqua
    } else {
        theme.border
    };

    let name_color = if persona.active {
        theme.text_primary
    } else {
        theme.text_secondary
    };

    div()
        .flex()
        .flex_col()
        .w(px(200.0))
        .p(theme.space_3)
        .gap(theme.space_2)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(border_color)
        .child(persona_card_header(persona, name_color, theme))
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .overflow_hidden()
                .max_h(px(32.0))
                .child(persona.description.clone()),
        )
        .child(persona_card_footer(persona, theme))
        .into_any_element()
}

fn persona_card_header(persona: &PersonaDisplay, name_color: Hsla, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            Icon::new(persona.icon())
                .size_4()
                .text_color(theme.accent_cyan),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(name_color)
                .font_weight(FontWeight::SEMIBOLD)
                .child(persona.name.clone()),
        )
}

fn persona_card_footer(persona: &PersonaDisplay, theme: &HiveTheme) -> Div {
    let active_color = if persona.active {
        theme.accent_aqua
    } else {
        theme.text_muted
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .px(theme.space_1)
                .py(px(1.0))
                .rounded(theme.radius_sm)
                .bg(theme.bg_tertiary)
                .text_size(theme.font_size_xs)
                .text_color(theme.text_secondary)
                .child(persona.model_tier.clone()),
        )
        .child(div().flex_1())
        .child(
            div()
                .w(px(6.0))
                .h(px(6.0))
                .rounded(theme.radius_full)
                .bg(active_color),
        )
}

fn custom_personas_header(theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .p(theme.space_3)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_secondary)
                .font_weight(FontWeight::MEDIUM)
                .child("Custom Personas"),
        )
        .child(div().flex_1())
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(24.0))
                .h(px(24.0))
                .rounded(theme.radius_full)
                .bg(theme.bg_tertiary)
                .text_size(theme.font_size_sm)
                .text_color(theme.accent_cyan)
                .child("+"),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn section_title(title: &str, count: usize, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .text_size(theme.font_size_lg)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
        .child(
            div()
                .px(theme.space_2)
                .py(px(2.0))
                .rounded(theme.radius_full)
                .bg(theme.bg_tertiary)
                .text_size(theme.font_size_xs)
                .text_color(theme.text_secondary)
                .child(format!("{count}")),
        )
}

// ---------------------------------------------------------------------------
