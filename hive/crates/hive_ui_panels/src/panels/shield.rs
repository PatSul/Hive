use gpui::*;
use gpui_component::switch::Switch;
use gpui_component::{Icon, IconName};

use hive_shield::UserRule;
use hive_ui_core::HiveTheme;

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

actions!(
    hive_shield_panel,
    [
        ShieldToggleEnabled,
        ShieldToggleSecretScan,
        ShieldToggleVulnCheck,
        ShieldTogglePii,
        ShieldAddRule,
    ]
);


// ---------------------------------------------------------------------------
// Event
// ---------------------------------------------------------------------------

/// Emitted when any shield config setting changes. The workspace subscribes
/// to this, persists the values to `AppConfig`, and rebuilds `AppShield`.
#[derive(Debug, Clone)]
pub struct ShieldConfigChanged;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single security event detected by the privacy shield.
#[derive(Debug, Clone)]
pub struct ShieldEvent {
    pub timestamp: String,
    pub event_type: String,
    pub severity: String,
    pub detail: String,
}

impl ShieldEvent {
    /// Map severity string to a color from the theme.
    pub fn severity_color(&self, theme: &HiveTheme) -> Hsla {
        match self.severity.as_str() {
            "critical" | "high" => theme.accent_red,
            "medium" | "warning" => theme.accent_yellow,
            "low" | "info" => theme.accent_cyan,
            _ => theme.text_muted,
        }
    }
}

/// Access policy for a specific AI provider.
#[derive(Debug, Clone)]
pub struct PolicyDisplay {
    pub provider: String,
    pub trust_level: String,
    pub max_classification: String,
    pub pii_cloaking: bool,
}

/// All data needed to render the privacy shield panel.
#[derive(Debug, Clone)]
pub struct ShieldPanelData {
    pub enabled: bool,
    pub pii_detections: usize,
    pub secrets_blocked: usize,
    pub threats_caught: usize,
    pub recent_events: Vec<ShieldEvent>,
    pub policies: Vec<PolicyDisplay>,
    // Interactive shield settings
    pub shield_enabled: bool,
    pub secret_scan_enabled: bool,
    pub vulnerability_check_enabled: bool,
    pub pii_detection_enabled: bool,
    pub user_rules: Vec<UserRule>,
}

impl ShieldPanelData {
    /// Create a default state with shield enabled but no events.
    pub fn empty() -> Self {
        Self {
            enabled: true,
            pii_detections: 0,
            secrets_blocked: 0,
            threats_caught: 0,
            recent_events: Vec::new(),
            policies: Vec::new(),
            shield_enabled: true,
            secret_scan_enabled: true,
            vulnerability_check_enabled: true,
            pii_detection_enabled: true,
            user_rules: Vec::new(),
        }
    }

    /// Return a sample dataset for preview / testing.
    #[allow(dead_code)]
    pub fn sample() -> Self {
        Self {
            enabled: true,
            pii_detections: 14,
            secrets_blocked: 3,
            threats_caught: 1,
            recent_events: vec![
                ShieldEvent {
                    timestamp: "2 min ago".into(),
                    event_type: "PII Detected".into(),
                    severity: "medium".into(),
                    detail: "Email address cloaked in prompt to Anthropic".into(),
                },
                ShieldEvent {
                    timestamp: "15 min ago".into(),
                    event_type: "Secret Blocked".into(),
                    severity: "high".into(),
                    detail: "AWS access key removed from code context".into(),
                },
                ShieldEvent {
                    timestamp: "1 hour ago".into(),
                    event_type: "Threat Detected".into(),
                    severity: "critical".into(),
                    detail: "Prompt injection attempt blocked in skill instructions".into(),
                },
                ShieldEvent {
                    timestamp: "3 hours ago".into(),
                    event_type: "PII Detected".into(),
                    severity: "low".into(),
                    detail: "Phone number cloaked in chat message".into(),
                },
            ],
            policies: vec![
                PolicyDisplay {
                    provider: "Anthropic".into(),
                    trust_level: "High".into(),
                    max_classification: "Confidential".into(),
                    pii_cloaking: true,
                },
                PolicyDisplay {
                    provider: "OpenAI".into(),
                    trust_level: "Medium".into(),
                    max_classification: "Internal".into(),
                    pii_cloaking: true,
                },
                PolicyDisplay {
                    provider: "OpenRouter".into(),
                    trust_level: "Low".into(),
                    max_classification: "Public".into(),
                    pii_cloaking: true,
                },
                PolicyDisplay {
                    provider: "Ollama (Local)".into(),
                    trust_level: "Full".into(),
                    max_classification: "Secret".into(),
                    pii_cloaking: false,
                },
            ],
            shield_enabled: true,
            secret_scan_enabled: true,
            vulnerability_check_enabled: true,
            pii_detection_enabled: true,
            user_rules: vec![
                UserRule::new("Block internal codenames", r"(?i)project\s+(phoenix|omega)"),
                UserRule::new("Block employee IDs", r"EMP-\d{5,}"),
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// ShieldView — interactive Entity
// ---------------------------------------------------------------------------

/// Interactive privacy shield panel with toggles and rule management.
pub struct ShieldView {
    pub shield_enabled: bool,
    pub secret_scan_enabled: bool,
    pub vulnerability_check_enabled: bool,
    pub pii_detection_enabled: bool,
    pub user_rules: Vec<UserRule>,
    // Read-only stats & events
    pub pii_detections: usize,
    pub secrets_blocked: usize,
    pub threats_caught: usize,
    pub recent_events: Vec<ShieldEvent>,
    pub policies: Vec<PolicyDisplay>,
}

impl EventEmitter<ShieldConfigChanged> for ShieldView {}

impl ShieldView {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            shield_enabled: true,
            secret_scan_enabled: true,
            vulnerability_check_enabled: true,
            pii_detection_enabled: true,
            user_rules: Vec::new(),
            pii_detections: 0,
            secrets_blocked: 0,
            threats_caught: 0,
            recent_events: Vec::new(),
            policies: Vec::new(),
        }
    }

    /// Update from external data (called by workspace's `refresh_shield_data`).
    pub fn update_from_data(&mut self, data: &ShieldPanelData) {
        self.shield_enabled = data.shield_enabled;
        self.secret_scan_enabled = data.secret_scan_enabled;
        self.vulnerability_check_enabled = data.vulnerability_check_enabled;
        self.pii_detection_enabled = data.pii_detection_enabled;
        self.user_rules = data.user_rules.clone();
        self.pii_detections = data.pii_detections;
        self.secrets_blocked = data.secrets_blocked;
        self.threats_caught = data.threats_caught;
        self.recent_events = data.recent_events.clone();
        self.policies = data.policies.clone();
    }

    /// Collect current toggle/rule state for persistence.
    pub fn collect_shield_config(&self) -> ShieldSnapshot {
        ShieldSnapshot {
            shield_enabled: self.shield_enabled,
            secret_scan_enabled: self.secret_scan_enabled,
            vulnerability_check_enabled: self.vulnerability_check_enabled,
            pii_detection_enabled: self.pii_detection_enabled,
            user_rules: self.user_rules.clone(),
        }
    }
}

/// Snapshot of shield settings for persistence to `HiveConfig`.
#[derive(Debug, Clone)]
pub struct ShieldSnapshot {
    pub shield_enabled: bool,
    pub secret_scan_enabled: bool,
    pub vulnerability_check_enabled: bool,
    pub pii_detection_enabled: bool,
    pub user_rules: Vec<UserRule>,
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for ShieldView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<hive_ui_core::AppTheme>().0.clone();

        div()
            .id("shield-panel")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p(theme.space_4)
            .gap(theme.space_4)
            // Actions
            .on_action(cx.listener(|this: &mut Self, _: &ShieldToggleEnabled, _, cx| {
                this.shield_enabled = !this.shield_enabled;
                cx.emit(ShieldConfigChanged);
                cx.notify();
            }))
            .on_action(cx.listener(|this: &mut Self, _: &ShieldToggleSecretScan, _, cx| {
                this.secret_scan_enabled = !this.secret_scan_enabled;
                cx.emit(ShieldConfigChanged);
                cx.notify();
            }))
            .on_action(cx.listener(|this: &mut Self, _: &ShieldToggleVulnCheck, _, cx| {
                this.vulnerability_check_enabled = !this.vulnerability_check_enabled;
                cx.emit(ShieldConfigChanged);
                cx.notify();
            }))
            .on_action(cx.listener(|this: &mut Self, _: &ShieldTogglePii, _, cx| {
                this.pii_detection_enabled = !this.pii_detection_enabled;
                cx.emit(ShieldConfigChanged);
                cx.notify();
            }))
            .on_action(cx.listener(|this: &mut Self, _: &ShieldAddRule, _, cx| {
                this.user_rules.push(UserRule::new("New Rule", r"pattern"));
                cx.emit(ShieldConfigChanged);
                cx.notify();
            }))
            // Children
            .child(render_header(self.shield_enabled, &theme))
            .child(self.render_controls(&theme, cx))
    }
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn render_header(enabled: bool, theme: &HiveTheme) -> AnyElement {
    let status_color = if enabled {
        theme.accent_green
    } else {
        theme.accent_red
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .child(header_icon(theme))
        .child(header_title(theme))
        .child(div().flex_1())
        .child(
            Switch::new("shield-master-toggle")
                .checked(enabled)
                .on_click(move |_checked, window, cx| {
                    window.dispatch_action(Box::new(ShieldToggleEnabled), cx);
                }),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(theme.space_2)
                .child(
                    div()
                        .w(px(8.0))
                        .h(px(8.0))
                        .rounded(theme.radius_full)
                        .bg(status_color),
                )
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(status_color)
                        .font_weight(FontWeight::MEDIUM)
                        .child(if enabled { "Active" } else { "Disabled" }),
                ),
        )
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
        .child(Icon::new(IconName::EyeOff).size_4())
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
                .child("Privacy Shield"),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child("PII detection, secret scanning, and threat prevention"),
        )
}

// ---------------------------------------------------------------------------
// Controls (toggles, rules, stats)
// ---------------------------------------------------------------------------

impl ShieldView {
    fn render_controls(&mut self, theme: &HiveTheme, cx: &mut Context<Self>) -> AnyElement {
        if !self.shield_enabled {
            return render_disabled_state(theme);
        }

        div()
            .flex()
            .flex_col()
            .gap(theme.space_4)
            .child(render_default_rules(self, theme))
            .child(self.render_custom_rules(theme, cx))
            .child(render_stats_bar(self, theme))
            .child(render_recent_activity(&self.recent_events, theme))
            .child(render_policies_section(&self.policies, theme))
            .into_any_element()
    }
}

// ---------------------------------------------------------------------------
// Default rules section
// ---------------------------------------------------------------------------

fn render_default_rules(view: &ShieldView, theme: &HiveTheme) -> AnyElement {
    card(theme)
        .child(section_title("Default Rules", theme))
        .child(section_desc(
            "Built-in protections that run on every outgoing message.",
            theme,
        ))
        .child(separator(theme))
        .child(switch_row(
            "Secret Scanning",
            "shield-secret-scan",
            view.secret_scan_enabled,
            ShieldToggleSecretScan,
            theme,
        ))
        .child(
            div()
                .pl(theme.space_4)
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child("Blocks API keys, tokens, private keys, and credentials from being sent."),
        )
        .child(separator(theme))
        .child(switch_row(
            "Vulnerability Detection",
            "shield-vuln-check",
            view.vulnerability_check_enabled,
            ShieldToggleVulnCheck,
            theme,
        ))
        .child(
            div()
                .pl(theme.space_4)
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child("Detects prompt injection, jailbreak attempts, and data exfiltration."),
        )
        .child(separator(theme))
        .child(switch_row(
            "PII Detection & Cloaking",
            "shield-pii-detect",
            view.pii_detection_enabled,
            ShieldTogglePii,
            theme,
        ))
        .child(
            div()
                .pl(theme.space_4)
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child("Finds emails, SSNs, credit cards, and phone numbers. Cloaks them for cloud providers."),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Custom rules section
// ---------------------------------------------------------------------------

impl ShieldView {
    fn render_custom_rules(&self, theme: &HiveTheme, cx: &mut Context<Self>) -> AnyElement {
        let mut section = card(theme)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .child(section_title("Custom Rules", theme)),
                    )
                    .child(
                        div()
                            .px(theme.space_3)
                            .py(theme.space_1)
                            .rounded(theme.radius_md)
                            .bg(theme.accent_green.opacity(0.1))
                            .border_1()
                            .border_color(theme.accent_green.opacity(0.2))
                            .text_size(theme.font_size_xs)
                            .text_color(theme.accent_green)
                            .font_weight(FontWeight::MEDIUM)
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                window.dispatch_action(Box::new(ShieldAddRule), cx);
                            })
                            .child("+ Add Rule"),
                    ),
            )
            .child(section_desc(
                "Block messages matching custom regex patterns.",
                theme,
            ))
            .child(separator(theme));

        if self.user_rules.is_empty() {
            section = section.child(
                div()
                    .py(theme.space_4)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_size(theme.font_size_sm)
                            .text_color(theme.text_muted)
                            .child("No custom rules yet. Add patterns to block specific content."),
                    ),
            );
        } else {
            for rule in &self.user_rules {
                section = section.child(self.render_rule_row(rule, theme, cx));
            }
        }

        section.into_any_element()
    }

    fn render_rule_row(&self, rule: &UserRule, theme: &HiveTheme, cx: &mut Context<Self>) -> AnyElement {
        let rule_id_toggle = rule.id.clone();
        let rule_id_delete = rule.id.clone();
        let rule_id_delete2 = rule.id.clone();
        let entity = cx.entity().clone();
        let entity2 = entity.clone();
        let active = rule.active;
        let name_color = if active {
            theme.text_primary
        } else {
            theme.text_muted
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(theme.space_3)
            .py(theme.space_2)
            .child(
                // Active toggle
                Switch::new(SharedString::from(format!("rule-toggle-{}", &rule.id)))
                    .checked(active)
                    .on_click(move |_checked, _window, cx| {
                        entity.update(cx, |this, cx| {
                            if let Some(r) = this.user_rules.iter_mut().find(|r| r.id == rule_id_toggle) {
                                r.active = !r.active;
                            }
                            cx.emit(ShieldConfigChanged);
                            cx.notify();
                        });
                    }),
            )
            .child(
                // Name + pattern
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(theme.font_size_sm)
                            .text_color(name_color)
                            .font_weight(FontWeight::MEDIUM)
                            .child(rule.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(theme.font_size_xs)
                            .text_color(theme.text_muted)
                            .font_family("monospace")
                            .child(rule.pattern.clone()),
                    ),
            )
            .child(
                // Delete button
                div()
                    .id(SharedString::from(format!("rule-delete-{}", &rule_id_delete2)))
                    .px(theme.space_2)
                    .py(theme.space_1)
                    .rounded(theme.radius_sm)
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.accent_red.opacity(0.1)))
                    .text_size(theme.font_size_xs)
                    .text_color(theme.accent_red)
                    .on_click(move |_, _window, cx| {
                        entity2.update(cx, |this, cx| {
                            this.user_rules.retain(|r| r.id != rule_id_delete);
                            cx.emit(ShieldConfigChanged);
                            cx.notify();
                        });
                    })
                    .child("\u{2715}"),
            )
            .into_any_element()
    }
}

// ---------------------------------------------------------------------------
// Stats bar
// ---------------------------------------------------------------------------

fn render_stats_bar(view: &ShieldView, theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .gap(theme.space_3)
        .child(stat_card(
            "PII Detections",
            view.pii_detections,
            theme.accent_yellow,
            theme,
        ))
        .child(stat_card(
            "Secrets Blocked",
            view.secrets_blocked,
            theme.accent_red,
            theme,
        ))
        .child(stat_card(
            "Threats Caught",
            view.threats_caught,
            theme.accent_cyan,
            theme,
        ))
        .into_any_element()
}

fn stat_card(label: &str, count: usize, accent: Hsla, theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .p(theme.space_3)
        .gap(theme.space_1)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_size(theme.font_size_2xl)
                .text_color(accent)
                .font_weight(FontWeight::BOLD)
                .child(format!("{count}")),
        )
        .child(
            div()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(label.to_string()),
        )
}

// ---------------------------------------------------------------------------
// Recent activity
// ---------------------------------------------------------------------------

fn render_recent_activity(events: &[ShieldEvent], theme: &HiveTheme) -> AnyElement {
    let mut section = card(theme).child(
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
                    .child("Recent Activity"),
            )
            .child(
                div()
                    .px(theme.space_2)
                    .py(px(2.0))
                    .rounded(theme.radius_full)
                    .bg(theme.bg_tertiary)
                    .text_size(theme.font_size_xs)
                    .text_color(theme.text_secondary)
                    .child(format!("{}", events.len())),
            ),
    );

    if events.is_empty() {
        section = section.child(
            div()
                .py(theme.space_4)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_muted)
                        .child("No recent security events."),
                ),
        );
    } else {
        section = section.child(separator(theme));
        for event in events {
            section = section.child(render_event_row(event, theme));
        }
    }

    section.into_any_element()
}

fn render_event_row(event: &ShieldEvent, theme: &HiveTheme) -> AnyElement {
    let severity_color = event.severity_color(theme);

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_3)
        .py(theme.space_1)
        .child(
            div()
                .w(px(6.0))
                .h(px(6.0))
                .rounded(theme.radius_full)
                .bg(severity_color),
        )
        .child(
            div()
                .w(px(80.0))
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .child(event.timestamp.clone()),
        )
        .child(
            div()
                .px(theme.space_1)
                .py(px(1.0))
                .rounded(theme.radius_sm)
                .bg(theme.bg_tertiary)
                .text_size(theme.font_size_xs)
                .text_color(severity_color)
                .font_weight(FontWeight::MEDIUM)
                .child(event.event_type.clone()),
        )
        .child(
            div()
                .flex_1()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_secondary)
                .overflow_hidden()
                .child(event.detail.clone()),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Access policies
// ---------------------------------------------------------------------------

fn render_policies_section(policies: &[PolicyDisplay], theme: &HiveTheme) -> AnyElement {
    let mut section = card(theme).child(
        div()
            .text_size(theme.font_size_lg)
            .text_color(theme.text_primary)
            .font_weight(FontWeight::SEMIBOLD)
            .child("Access Policies"),
    );

    if policies.is_empty() {
        section = section.child(
            div()
                .py(theme.space_3)
                .flex_col()
                .gap(theme.space_2)
                .child(
                    div()
                        .text_size(theme.font_size_sm)
                        .text_color(theme.text_secondary)
                        .child("Default policies are active. All outbound requests pass through HiveShield's 4-layer protection:"),
                )
                .child(
                    div()
                        .pl(theme.space_3)
                        .flex_col()
                        .gap(theme.space_1)
                        .child(div().text_size(theme.font_size_xs).text_color(theme.text_muted).child("\u{2022} PII detection \u{2014} 11+ sensitive data types"))
                        .child(div().text_size(theme.font_size_xs).text_color(theme.text_muted).child("\u{2022} Secrets scanning \u{2014} API keys, tokens, credentials"))
                        .child(div().text_size(theme.font_size_xs).text_color(theme.text_muted).child("\u{2022} Vulnerability assessment \u{2014} injection & jailbreak detection"))
                        .child(div().text_size(theme.font_size_xs).text_color(theme.text_muted).child("\u{2022} Access control \u{2014} PII cloaking required for cloud providers")),
                )
                .child(
                    div()
                        .text_size(theme.font_size_xs)
                        .text_color(theme.text_muted)
                        .pt(theme.space_1)
                        .child("Local providers (Ollama, LM Studio) are fully trusted. Cloud providers require PII cloaking and are limited to Internal classification."),
                ),
        );
    } else {
        section = section.child(policy_table_header(theme));
        section = section.child(separator(theme));
        for policy in policies {
            section = section.child(render_policy_row(policy, theme));
        }
    }

    section.into_any_element()
}

fn policy_table_header(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .child(
            div()
                .w(px(120.0))
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .font_weight(FontWeight::SEMIBOLD)
                .child("Provider"),
        )
        .child(
            div()
                .w(px(80.0))
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .font_weight(FontWeight::SEMIBOLD)
                .child("Trust"),
        )
        .child(
            div()
                .flex_1()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .font_weight(FontWeight::SEMIBOLD)
                .child("Max Classification"),
        )
        .child(
            div()
                .w(px(80.0))
                .text_size(theme.font_size_xs)
                .text_color(theme.text_muted)
                .font_weight(FontWeight::SEMIBOLD)
                .child("PII Cloak"),
        )
}

fn render_policy_row(policy: &PolicyDisplay, theme: &HiveTheme) -> AnyElement {
    let trust_color = match policy.trust_level.as_str() {
        "Full" => theme.accent_green,
        "High" => theme.accent_aqua,
        "Medium" => theme.accent_yellow,
        "Low" => theme.accent_red,
        _ => theme.text_muted,
    };

    let pii_color = if policy.pii_cloaking {
        theme.accent_green
    } else {
        theme.text_muted
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(theme.space_2)
        .py(theme.space_1)
        .child(
            div()
                .w(px(120.0))
                .text_size(theme.font_size_sm)
                .text_color(theme.text_primary)
                .child(policy.provider.clone()),
        )
        .child(
            div()
                .w(px(80.0))
                .text_size(theme.font_size_xs)
                .text_color(trust_color)
                .font_weight(FontWeight::MEDIUM)
                .child(policy.trust_level.clone()),
        )
        .child(
            div()
                .flex_1()
                .text_size(theme.font_size_xs)
                .text_color(theme.text_secondary)
                .child(policy.max_classification.clone()),
        )
        .child(
            div()
                .w(px(80.0))
                .text_size(theme.font_size_xs)
                .text_color(pii_color)
                .child(if policy.pii_cloaking {
                    "\u{2713} On"
                } else {
                    "\u{2717} Off"
                }),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Disabled state
// ---------------------------------------------------------------------------

fn render_disabled_state(theme: &HiveTheme) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .flex_1()
        .gap(theme.space_2)
        .p(theme.space_8)
        .child(
            div()
                .text_size(px(32.0))
                .text_color(theme.text_muted)
                .child("\u{1F6E1}"),
        )
        .child(
            div()
                .text_size(theme.font_size_base)
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_secondary)
                .child("Privacy Shield Disabled"),
        )
        .child(
            div()
                .text_size(theme.font_size_sm)
                .text_color(theme.text_muted)
                .child("Enable the shield to protect sensitive data in AI interactions."),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn card(theme: &HiveTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .p(theme.space_4)
        .gap(theme.space_3)
        .rounded(theme.radius_md)
        .bg(theme.bg_surface)
        .border_1()
        .border_color(theme.border)
}

fn section_title(label: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .text_size(theme.font_size_lg)
        .text_color(theme.text_primary)
        .font_weight(FontWeight::SEMIBOLD)
        .child(label.to_string())
        .into_any_element()
}

fn section_desc(text: &str, theme: &HiveTheme) -> AnyElement {
    div()
        .text_size(theme.font_size_sm)
        .text_color(theme.text_muted)
        .child(text.to_string())
        .into_any_element()
}

fn separator(theme: &HiveTheme) -> AnyElement {
    div()
        .w_full()
        .h(px(1.0))
        .bg(theme.border)
        .into_any_element()
}

fn switch_row<A: Action + Clone>(
    label: &str,
    id: impl Into<ElementId>,
    checked: bool,
    action: A,
    theme: &HiveTheme,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(theme.space_4)
        .py(theme.space_2)
        .child(
            div()
                .flex_1()
                .text_size(theme.font_size_base)
                .text_color(theme.text_secondary)
                .child(label.to_string()),
        )
        .child(
            Switch::new(id)
                .checked(checked)
                .on_click(move |_new_checked, window, cx| {
                    window.dispatch_action(Box::new(action.clone()), cx);
                }),
        )
        .into_any_element()
}

// ---------------------------------------------------------------------------
// Backward-compat: ShieldPanel (pure-render, kept for tests)
// ---------------------------------------------------------------------------

/// Legacy pure-render entry point. Prefer using `ShieldView` as an Entity.
pub struct ShieldPanel;

impl ShieldPanel {
    pub fn render(data: &ShieldPanelData, theme: &HiveTheme) -> impl IntoElement {
        let enabled = data.shield_enabled;

        let controls = if enabled {
            let view = ShieldView {
                shield_enabled: data.shield_enabled,
                secret_scan_enabled: data.secret_scan_enabled,
                vulnerability_check_enabled: data.vulnerability_check_enabled,
                pii_detection_enabled: data.pii_detection_enabled,
                user_rules: data.user_rules.clone(),
                pii_detections: data.pii_detections,
                secrets_blocked: data.secrets_blocked,
                threats_caught: data.threats_caught,
                recent_events: data.recent_events.clone(),
                policies: data.policies.clone(),
            };

            div()
                .flex()
                .flex_col()
                .gap(theme.space_4)
                .child(render_default_rules(&view, theme))
                .child(render_stats_bar(&view, theme))
                .child(render_recent_activity(&view.recent_events, theme))
                .child(render_policies_section(&view.policies, theme))
                .into_any_element()
        } else {
            render_disabled_state(theme)
        };

        div()
            .id("shield-panel")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p(theme.space_4)
            .gap(theme.space_4)
            .child(render_header(enabled, theme))
            .child(controls)
    }
}
