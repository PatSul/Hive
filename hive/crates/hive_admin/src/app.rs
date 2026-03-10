use crate::api::{
    ApiClient, DashboardStats, GatewayStats, RelayStats, SyncStats, TeamRecord, UserRecord,
};
use ratatui::widgets::TableState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Dashboard,
    Users,
    Gateway,
    Relay,
    Sync,
    Teams,
}

impl Tab {
    pub const ALL: [Tab; 6] = [
        Tab::Dashboard,
        Tab::Users,
        Tab::Gateway,
        Tab::Relay,
        Tab::Sync,
        Tab::Teams,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Tab::Dashboard => " Dashboard ",
            Tab::Users => " Users ",
            Tab::Gateway => " Gateway ",
            Tab::Relay => " Relay ",
            Tab::Sync => " Sync ",
            Tab::Teams => " Teams ",
        }
    }
}

pub struct App {
    pub current_tab: Tab,
    pub search_active: bool,
    pub search_query: String,
    pub table_state: TableState,
    pub dashboard: Option<DashboardStats>,
    pub users: Vec<UserRecord>,
    pub gateway: Option<GatewayStats>,
    pub relay: Option<RelayStats>,
    pub sync_stats: Option<SyncStats>,
    pub teams: Vec<TeamRecord>,
    api: ApiClient,
}

impl App {
    pub fn new(api: ApiClient) -> Self {
        Self {
            current_tab: Tab::Dashboard,
            search_active: false,
            search_query: String::new(),
            table_state: TableState::default(),
            dashboard: None,
            users: Vec::new(),
            gateway: None,
            relay: None,
            sync_stats: None,
            teams: Vec::new(),
            api,
        }
    }

    pub async fn refresh_data(&mut self) {
        if let Ok(d) = self.api.fetch_dashboard().await {
            self.dashboard = Some(d);
        }
        if let Ok(u) = self.api.fetch_users().await {
            self.users = u;
        }
        if let Ok(g) = self.api.fetch_gateway().await {
            self.gateway = Some(g);
        }
        if let Ok(r) = self.api.fetch_relay().await {
            self.relay = Some(r);
        }
        if let Ok(s) = self.api.fetch_sync().await {
            self.sync_stats = Some(s);
        }
        if let Ok(t) = self.api.fetch_teams().await {
            self.teams = t;
        }
    }

    pub fn next_tab(&mut self) {
        let idx = Tab::ALL
            .iter()
            .position(|t| *t == self.current_tab)
            .unwrap_or(0);
        self.current_tab = Tab::ALL[(idx + 1) % Tab::ALL.len()];
        self.table_state = TableState::default();
    }

    pub fn prev_tab(&mut self) {
        let idx = Tab::ALL
            .iter()
            .position(|t| *t == self.current_tab)
            .unwrap_or(0);
        self.current_tab = Tab::ALL[(idx + Tab::ALL.len() - 1) % Tab::ALL.len()];
        self.table_state = TableState::default();
    }

    pub fn select_next(&mut self) {
        let count = self.row_count();
        if count == 0 {
            return;
        }
        let i = self
            .table_state
            .selected()
            .map(|s| (s + 1) % count)
            .unwrap_or(0);
        self.table_state.select(Some(i));
    }

    pub fn select_prev(&mut self) {
        let count = self.row_count();
        if count == 0 {
            return;
        }
        let i = self
            .table_state
            .selected()
            .map(|s| (s + count - 1) % count)
            .unwrap_or(0);
        self.table_state.select(Some(i));
    }

    pub fn toggle_search(&mut self) {
        self.search_active = true;
        self.search_query.clear();
    }

    pub fn cancel_search(&mut self) {
        self.search_active = false;
        self.search_query.clear();
    }

    pub fn search_input(&mut self, c: char) {
        if self.search_active {
            self.search_query.push(c);
        }
    }

    pub fn search_backspace(&mut self) {
        if self.search_active {
            self.search_query.pop();
        }
    }

    pub fn filtered_users(&self) -> Vec<&UserRecord> {
        if self.search_query.is_empty() {
            self.users.iter().collect()
        } else {
            let q = self.search_query.to_lowercase();
            self.users
                .iter()
                .filter(|u| u.email.to_lowercase().contains(&q))
                .collect()
        }
    }

    fn row_count(&self) -> usize {
        match self.current_tab {
            Tab::Users => self.filtered_users().len(),
            Tab::Gateway => self.gateway.as_ref().map(|g| g.models.len()).unwrap_or(0),
            Tab::Relay => self.relay.as_ref().map(|r| r.rooms.len()).unwrap_or(0),
            Tab::Sync => self
                .sync_stats
                .as_ref()
                .map(|s| s.per_user.len())
                .unwrap_or(0),
            Tab::Teams => self.teams.len(),
            _ => 0,
        }
    }
}
