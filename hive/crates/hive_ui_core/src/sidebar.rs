use gpui_component::IconName;

/// The 27 navigable panels in the application.
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
    pub const ALL: [Panel; 27] = [
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

    pub fn label(self) -> &'static str {
        match self {
            Self::Chat => "Chat",
            Self::QuickStart => "Quick Start",
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
    /// Keyboard shortcuts use this to map `ctrl-1` (index 0) through `ctrl-9`
    /// (index 8) and `ctrl-0` (index 9) to panels.
    pub fn from_index(idx: usize) -> Option<Panel> {
        Self::ALL.get(idx).copied()
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

/// Sidebar component with 26 navigation icon buttons.
pub struct Sidebar {
    pub active_panel: Panel,
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
        }
    }
}
