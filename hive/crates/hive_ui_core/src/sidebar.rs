use gpui_component::IconName;

/// The 28 navigable panels in the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Panel {
    Chat,
    QuickStart,
    History,
    Files,
    CodeMap,
    PromptLibrary,
    Specs,
    Agents,
    Workflows,
    Channels,
    Kanban,
    Monitor,
    Activity,
    Logs,
    Costs,
    Review,
    Skills,
    Routing,
    RoutingMatrix,
    Models,
    Learning,
    Shield,
    Assistant,
    TokenLaunch,
    Network,
    Terminal,
    Settings,
    Help,
}

impl Panel {
    pub const ALL: [Panel; 28] = [
        Panel::Chat,
        Panel::History,
        Panel::Files,
        Panel::CodeMap,
        Panel::PromptLibrary,
        Panel::Specs,
        Panel::Agents,
        Panel::Workflows,
        Panel::Channels,
        Panel::Kanban,
        Panel::Monitor,
        Panel::Activity,
        Panel::Logs,
        Panel::Costs,
        Panel::Review,
        Panel::Skills,
        Panel::Routing,
        Panel::RoutingMatrix,
        Panel::Models,
        Panel::Learning,
        Panel::Shield,
        Panel::Assistant,
        Panel::TokenLaunch,
        Panel::Network,
        Panel::Terminal,
        Panel::Settings,
        Panel::Help,
        Panel::QuickStart,
    ];

    /// Number-key shortcuts should follow the visible shell, not the raw
    /// historical `Panel::ALL` order.
    pub const SHORTCUT_ORDER: [Panel; 10] = [
        Panel::QuickStart,
        Panel::Chat,
        Panel::Files,
        Panel::History,
        Panel::Specs,
        Panel::Agents,
        Panel::Workflows,
        Panel::Kanban,
        Panel::Activity,
        Panel::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Chat => "Chat",
            Self::QuickStart => "Home",
            Self::History => "History",
            Self::Files => "Files",
            Self::CodeMap => "Code Map",
            Self::PromptLibrary => "Prompts",
            Self::Specs => "Specs",
            Self::Agents => "Agents",
            Self::Workflows => "Workflows",
            Self::Channels => "Channels",
            Self::Kanban => "Kanban",
            Self::Monitor => "Monitor",
            Self::Activity => "Activity",
            Self::Logs => "Logs",
            Self::Costs => "Costs",
            Self::Review => "Git Ops",
            Self::Skills => "Skills",
            Self::Routing => "Routing",
            Self::RoutingMatrix => "Routing Matrix",
            Self::Models => "Models",
            Self::Learning => "Learning",
            Self::Shield => "Shield",
            Self::Assistant => "Assistant",
            Self::TokenLaunch => "Launch",
            Self::Network => "Network",
            Self::Terminal => "Terminal",
            Self::Settings => "Settings",
            Self::Help => "Help",
        }
    }

    /// Return the panel at the given index in `Panel::ALL`, or `None` if out
    /// of bounds.
    ///
    /// Keyboard shortcuts use `from_shortcut_index` so they match the visible
    /// shell order. This method remains the raw panel inventory lookup.
    pub fn from_index(idx: usize) -> Option<Panel> {
        Self::ALL.get(idx).copied()
    }

    pub fn from_shortcut_index(idx: usize) -> Option<Panel> {
        Self::SHORTCUT_ORDER.get(idx).copied()
    }

    /// Higher-level shell grouping for the panel. Utility panels intentionally
    /// return `None` so the shell can preserve the previous primary
    /// destination while a utility surface is open.
    pub fn shell_destination(self) -> Option<ShellDestination> {
        match self {
            Self::QuickStart => Some(ShellDestination::Home),
            Self::Chat
            | Self::History
            | Self::Files
            | Self::CodeMap
            | Self::PromptLibrary
            | Self::Specs
            | Self::Review
            | Self::Terminal => Some(ShellDestination::Build),
            Self::Agents | Self::Workflows | Self::Channels | Self::Kanban => {
                Some(ShellDestination::Automate)
            }
            Self::Assistant => Some(ShellDestination::Home),
            Self::Monitor
            | Self::Activity
            | Self::Logs
            | Self::Costs
            | Self::Learning
            | Self::Shield => Some(ShellDestination::Observe),
            Self::Skills
            | Self::Routing
            | Self::RoutingMatrix
            | Self::Models
            | Self::Network
            | Self::TokenLaunch
            | Self::Settings
            | Self::Help => Some(ShellDestination::Settings),
        }
    }

    pub fn is_utility(self) -> bool {
        self.shell_destination().is_none()
    }

    pub fn is_labs_panel(self) -> bool {
        matches!(self, Self::TokenLaunch)
    }

    pub fn labs_enabled() -> bool {
        std::env::var("HIVE_ENABLE_LABS")
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    }

    pub fn is_visible(self) -> bool {
        !self.is_labs_panel() || Self::labs_enabled()
    }

    /// SVG icon for each panel via gpui-component IconName.
    pub fn icon(self) -> IconName {
        match self {
            Self::Chat => IconName::Bot,
            Self::QuickStart => IconName::Star,
            Self::History => IconName::Calendar,
            Self::Files => IconName::Folder,
            Self::CodeMap => IconName::Inspector,
            Self::PromptLibrary => IconName::BookOpen,
            Self::Specs => IconName::File,
            Self::Agents => IconName::Bot,
            Self::Workflows => IconName::Map,
            Self::Channels => IconName::Inbox,
            Self::Kanban => IconName::LayoutDashboard,
            Self::Monitor => IconName::Loader,
            Self::Activity => IconName::Inbox,
            Self::Logs => IconName::File,
            Self::Costs => IconName::ChartPie,
            Self::Review => IconName::Eye,
            Self::Skills => IconName::Star,
            Self::Routing => IconName::Map,
            Self::RoutingMatrix => IconName::LayoutDashboard,
            Self::Models => IconName::BookOpen,
            Self::Learning => IconName::Redo2,
            Self::Shield => IconName::EyeOff,
            Self::Assistant => IconName::Bell,
            Self::TokenLaunch => IconName::Globe,
            Self::Network => IconName::Globe,
            Self::Terminal => IconName::Dash,
            Self::Settings => IconName::Settings,
            Self::Help => IconName::Info,
        }
    }
}

impl Panel {
    /// Convert a stored string back to a `Panel`, defaulting to `Chat` for
    /// unknown values. Used by session recovery.
    pub fn from_stored(s: &str) -> Self {
        match s {
            "Chat" => Self::Chat,
            "QuickStart" => Self::QuickStart,
            "History" => Self::History,
            "Files" => Self::Files,
            "CodeMap" => Self::CodeMap,
            "PromptLibrary" => Self::PromptLibrary,
            "Specs" => Self::Specs,
            "Agents" => Self::Agents,
            "Workflows" => Self::Workflows,
            "Channels" => Self::Channels,
            "Kanban" => Self::Kanban,
            "Monitor" => Self::Monitor,
            "Activity" => Self::Activity,
            "Logs" => Self::Logs,
            "Costs" => Self::Costs,
            "Review" | "GitOps" => Self::Review,
            "Skills" => Self::Skills,
            "Routing" => Self::Routing,
            "RoutingMatrix" => Self::RoutingMatrix,
            "Models" => Self::Models,
            "Learning" => Self::Learning,
            "Shield" => Self::Shield,
            "Assistant" => Self::Assistant,
            "TokenLaunch" => Self::TokenLaunch,
            "Network" => Self::Network,
            "Terminal" => Self::Terminal,
            "Settings" => Self::Settings,
            "Help" => Self::Help,
            _ => Self::Chat,
        }
    }

    /// Serialize to a stable string for session persistence.
    pub fn to_stored(self) -> &'static str {
        match self {
            Self::Chat => "Chat",
            Self::QuickStart => "QuickStart",
            Self::History => "History",
            Self::Files => "Files",
            Self::CodeMap => "CodeMap",
            Self::PromptLibrary => "PromptLibrary",
            Self::Specs => "Specs",
            Self::Agents => "Agents",
            Self::Workflows => "Workflows",
            Self::Channels => "Channels",
            Self::Kanban => "Kanban",
            Self::Monitor => "Monitor",
            Self::Activity => "Activity",
            Self::Logs => "Logs",
            Self::Costs => "Costs",
            Self::Review => "Review",
            Self::Skills => "Skills",
            Self::Routing => "Routing",
            Self::RoutingMatrix => "RoutingMatrix",
            Self::Models => "Models",
            Self::Learning => "Learning",
            Self::Shield => "Shield",
            Self::Assistant => "Assistant",
            Self::TokenLaunch => "TokenLaunch",
            Self::Network => "Network",
            Self::Terminal => "Terminal",
            Self::Settings => "Settings",
            Self::Help => "Help",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShellDestination {
    Home,
    Build,
    Automate,
    Observe,
    Settings,
}

impl ShellDestination {
    pub const ALL: [ShellDestination; 5] = [
        ShellDestination::Home,
        ShellDestination::Build,
        ShellDestination::Automate,
        ShellDestination::Observe,
        ShellDestination::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Build => "Work",
            Self::Automate => "Runs",
            Self::Observe => "Observe",
            Self::Settings => "Settings",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Home => "Start work, clear setup blockers, and launch the next mission.",
            Self::Build => "Code, plan, review, and execute in the active workspace.",
            Self::Automate => "Run workflows, channels, and distributed execution paths.",
            Self::Observe => "Review approvals, activity, costs, and safety signals.",
            Self::Settings => "Configure models, routing, skills, network, and advanced tools.",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Self::Home => IconName::Star,
            Self::Build => IconName::Bot,
            Self::Automate => IconName::Map,
            Self::Observe => IconName::Inbox,
            Self::Settings => IconName::Settings,
        }
    }

    pub fn default_panel(self) -> Panel {
        match self {
            Self::Home => Panel::QuickStart,
            Self::Build => Panel::Chat,
            Self::Automate => Panel::Workflows,
            Self::Observe => Panel::Activity,
            Self::Settings => Panel::Settings,
        }
    }

    pub fn panels(self) -> &'static [Panel] {
        match self {
            Self::Home => &[Panel::QuickStart, Panel::Assistant],
            Self::Build => &[
                Panel::Chat,
                Panel::Files,
                Panel::History,
                Panel::Specs,
                Panel::CodeMap,
                Panel::PromptLibrary,
                Panel::Review,
                Panel::Terminal,
            ],
            Self::Automate => &[
                Panel::Agents,
                Panel::Workflows,
                Panel::Kanban,
                Panel::Channels,
            ],
            Self::Observe => &[
                Panel::Activity,
                Panel::Monitor,
                Panel::Logs,
                Panel::Costs,
                Panel::Learning,
                Panel::Shield,
            ],
            Self::Settings => &[
                Panel::Settings,
                Panel::Models,
                Panel::Routing,
                Panel::RoutingMatrix,
                Panel::Skills,
                Panel::Network,
                Panel::TokenLaunch,
                Panel::Help,
            ],
        }
    }

    pub fn from_panel(panel: Panel) -> Option<Self> {
        panel.shell_destination()
    }
}

/// Sidebar state for shell-level destinations and panel routing.
pub struct Sidebar {
    pub active_panel: Panel,
    pub active_destination: ShellDestination,
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new()
    }
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            active_panel: Panel::Chat,
            active_destination: ShellDestination::Build,
        }
    }
}
