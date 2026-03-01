use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[allow(dead_code)]
pub struct ApiClient {
    pub server_url: String,
    pub token: String,
    #[allow(dead_code)]
    client: reqwest::Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardStats {
    pub total_users: u64,
    pub free_users: u64,
    pub pro_users: u64,
    pub team_users: u64,
    pub revenue_estimate: f64,
    pub gateway_requests_today: u64,
    pub gateway_requests_month: u64,
    pub active_relay_connections: u64,
    pub sync_storage_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub id: String,
    pub email: String,
    pub tier: String,
    pub created_at: DateTime<Utc>,
    pub last_login: DateTime<Utc>,
    pub usage_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayStats {
    pub total_requests: u64,
    pub total_tokens: u64,
    pub models: Vec<ModelUsage>,
    pub providers: Vec<ProviderCost>,
    pub budget_alerts: Vec<BudgetAlert>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub requests: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCost {
    pub provider: String,
    pub requests: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetAlert {
    pub user_email: String,
    pub used_pct: f64,
    pub tier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayRoom {
    pub room_id: String,
    pub participants: u32,
    pub created_at: DateTime<Utc>,
    pub bytes_transferred: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayStats {
    pub active_rooms: u64,
    pub connected_devices: u64,
    pub rooms: Vec<RelayRoom>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncBlobRecord {
    pub user_email: String,
    pub blob_count: u64,
    pub storage_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncOperation {
    pub timestamp: DateTime<Utc>,
    pub user_email: String,
    pub operation: String,
    pub key: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStats {
    pub total_blobs: u64,
    pub storage_used_bytes: u64,
    pub storage_available_bytes: u64,
    pub per_user: Vec<SyncBlobRecord>,
    pub recent_ops: Vec<SyncOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub email: String,
    pub role: String,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamRecord {
    pub id: String,
    pub name: String,
    pub member_count: u32,
    pub plan: String,
    pub monthly_cost: f64,
    pub members: Vec<TeamMember>,
    pub usage_tokens: u64,
}

impl ApiClient {
    pub fn new(server_url: &str, token: &str) -> Self {
        Self { server_url: server_url.to_string(), token: token.to_string(), client: reqwest::Client::new() }
    }

    pub async fn fetch_dashboard(&self) -> anyhow::Result<DashboardStats> { Ok(mock_dashboard()) }

    pub async fn fetch_users(&self) -> anyhow::Result<Vec<UserRecord>> { Ok(mock_users()) }

    pub async fn fetch_gateway(&self) -> anyhow::Result<GatewayStats> { Ok(mock_gateway()) }

    pub async fn fetch_relay(&self) -> anyhow::Result<RelayStats> { Ok(mock_relay()) }

    pub async fn fetch_sync(&self) -> anyhow::Result<SyncStats> { Ok(mock_sync()) }

    pub async fn fetch_teams(&self) -> anyhow::Result<Vec<TeamRecord>> { Ok(mock_teams()) }

}

fn mock_dashboard() -> DashboardStats {
    DashboardStats {
        total_users: 1_247, free_users: 892, pro_users: 298, team_users: 57,
        revenue_estimate: 298.0 * 8.0 + 57.0 * 20.0,
        gateway_requests_today: 14_832, gateway_requests_month: 387_291,
        active_relay_connections: 42, sync_storage_bytes: 2_147_483_648,
    }
}

fn mock_users() -> Vec<UserRecord> {
    use chrono::TimeZone;
    vec![
        UserRecord { id: "usr_001".into(), email: "alice@example.com".into(), tier: "Pro".into(), created_at: Utc.with_ymd_and_hms(2025,6,15,10,30,0).unwrap(), last_login: Utc.with_ymd_and_hms(2026,2,27,8,12,0).unwrap(), usage_tokens: 1_250_000 },
        UserRecord { id: "usr_002".into(), email: "bob@devshop.io".into(), tier: "Team".into(), created_at: Utc.with_ymd_and_hms(2025,8,3,14,0,0).unwrap(), last_login: Utc.with_ymd_and_hms(2026,2,26,22,45,0).unwrap(), usage_tokens: 3_800_000 },
        UserRecord { id: "usr_003".into(), email: "carol@startup.co".into(), tier: "Free".into(), created_at: Utc.with_ymd_and_hms(2026,1,10,9,0,0).unwrap(), last_login: Utc.with_ymd_and_hms(2026,2,25,17,30,0).unwrap(), usage_tokens: 48_000 },
        UserRecord { id: "usr_004".into(), email: "dave@bigcorp.com".into(), tier: "Pro".into(), created_at: Utc.with_ymd_and_hms(2025,11,20,11,15,0).unwrap(), last_login: Utc.with_ymd_and_hms(2026,2,27,6,0,0).unwrap(), usage_tokens: 2_100_000 },
        UserRecord { id: "usr_005".into(), email: "eve@freelance.dev".into(), tier: "Free".into(), created_at: Utc.with_ymd_and_hms(2026,2,1,16,45,0).unwrap(), last_login: Utc.with_ymd_and_hms(2026,2,27,9,30,0).unwrap(), usage_tokens: 12_500 },
        UserRecord { id: "usr_006".into(), email: "frank@agency.io".into(), tier: "Team".into(), created_at: Utc.with_ymd_and_hms(2025,9,12,8,0,0).unwrap(), last_login: Utc.with_ymd_and_hms(2026,2,26,20,10,0).unwrap(), usage_tokens: 5_400_000 },
        UserRecord { id: "usr_007".into(), email: "grace@uni.edu".into(), tier: "Free".into(), created_at: Utc.with_ymd_and_hms(2026,1,25,13,20,0).unwrap(), last_login: Utc.with_ymd_and_hms(2026,2,24,11,0,0).unwrap(), usage_tokens: 5_200 },
        UserRecord { id: "usr_008".into(), email: "heidi@consulting.biz".into(), tier: "Pro".into(), created_at: Utc.with_ymd_and_hms(2025,7,1,9,0,0).unwrap(), last_login: Utc.with_ymd_and_hms(2026,2,27,7,45,0).unwrap(), usage_tokens: 980_000 },
    ]
}

fn mock_gateway() -> GatewayStats {
    GatewayStats {
        total_requests: 387_291, total_tokens: 94_500_000,
        models: vec![
            ModelUsage { model: "claude-sonnet-4-20250514".into(), requests: 142_000, tokens_in: 28_400_000, tokens_out: 14_200_000 },
            ModelUsage { model: "gpt-4o".into(), requests: 98_000, tokens_in: 19_600_000, tokens_out: 9_800_000 },
            ModelUsage { model: "claude-3-haiku-20240307".into(), requests: 85_000, tokens_in: 8_500_000, tokens_out: 4_250_000 },
            ModelUsage { model: "gemini-2.0-flash".into(), requests: 62_291, tokens_in: 6_229_100, tokens_out: 3_114_550 },
        ],
        providers: vec![
            ProviderCost { provider: "Anthropic".into(), requests: 227_000, cost_usd: 1_842.50 },
            ProviderCost { provider: "OpenAI".into(), requests: 98_000, cost_usd: 1_274.00 },
            ProviderCost { provider: "Google".into(), requests: 62_291, cost_usd: 312.45 },
        ],
        budget_alerts: vec![
            BudgetAlert { user_email: "bob@devshop.io".into(), used_pct: 92.0, tier: "Team".into() },
            BudgetAlert { user_email: "dave@bigcorp.com".into(), used_pct: 87.5, tier: "Pro".into() },
            BudgetAlert { user_email: "frank@agency.io".into(), used_pct: 78.0, tier: "Team".into() },
        ],
    }
}

fn mock_relay() -> RelayStats {
    use chrono::TimeZone;
    RelayStats {
        active_rooms: 12, connected_devices: 42,
        rooms: vec![
            RelayRoom { room_id: "room_a1b2c3".into(), participants: 3, created_at: Utc.with_ymd_and_hms(2026,2,27,7,0,0).unwrap(), bytes_transferred: 4_194_304 },
            RelayRoom { room_id: "room_d4e5f6".into(), participants: 2, created_at: Utc.with_ymd_and_hms(2026,2,27,8,30,0).unwrap(), bytes_transferred: 1_048_576 },
            RelayRoom { room_id: "room_g7h8i9".into(), participants: 5, created_at: Utc.with_ymd_and_hms(2026,2,27,6,15,0).unwrap(), bytes_transferred: 12_582_912 },
            RelayRoom { room_id: "room_j0k1l2".into(), participants: 2, created_at: Utc.with_ymd_and_hms(2026,2,27,9,0,0).unwrap(), bytes_transferred: 524_288 },
            RelayRoom { room_id: "room_m3n4o5".into(), participants: 4, created_at: Utc.with_ymd_and_hms(2026,2,26,22,0,0).unwrap(), bytes_transferred: 8_388_608 },
        ],
    }
}

fn mock_sync() -> SyncStats {
    use chrono::TimeZone;
    SyncStats {
        total_blobs: 3_842, storage_used_bytes: 2_147_483_648, storage_available_bytes: 10_737_418_240,
        per_user: vec![
            SyncBlobRecord { user_email: "alice@example.com".into(), blob_count: 342, storage_bytes: 268_435_456 },
            SyncBlobRecord { user_email: "bob@devshop.io".into(), blob_count: 891, storage_bytes: 536_870_912 },
            SyncBlobRecord { user_email: "dave@bigcorp.com".into(), blob_count: 1205, storage_bytes: 805_306_368 },
            SyncBlobRecord { user_email: "frank@agency.io".into(), blob_count: 624, storage_bytes: 402_653_184 },
            SyncBlobRecord { user_email: "heidi@consulting.biz".into(), blob_count: 180, storage_bytes: 134_217_728 },
        ],
        recent_ops: vec![
            SyncOperation { timestamp: Utc.with_ymd_and_hms(2026,2,27,9,30,12).unwrap(), user_email: "alice@example.com".into(), operation: "PUT".into(), key: "settings/editor.json".into(), bytes: 2_048 },
            SyncOperation { timestamp: Utc.with_ymd_and_hms(2026,2,27,9,28,45).unwrap(), user_email: "bob@devshop.io".into(), operation: "PUT".into(), key: "snippets/rust.json".into(), bytes: 15_360 },
            SyncOperation { timestamp: Utc.with_ymd_and_hms(2026,2,27,9,25,0).unwrap(), user_email: "dave@bigcorp.com".into(), operation: "GET".into(), key: "workspaces/project-x.json".into(), bytes: 8_192 },
            SyncOperation { timestamp: Utc.with_ymd_and_hms(2026,2,27,9,22,33).unwrap(), user_email: "frank@agency.io".into(), operation: "DELETE".into(), key: "cache/old-models.bin".into(), bytes: 0 },
            SyncOperation { timestamp: Utc.with_ymd_and_hms(2026,2,27,9,20,10).unwrap(), user_email: "heidi@consulting.biz".into(), operation: "PUT".into(), key: "themes/custom-dark.json".into(), bytes: 4_096 },
        ],
    }
}

fn mock_teams() -> Vec<TeamRecord> {
    use chrono::TimeZone;
    vec![
        TeamRecord { id: "team_001".into(), name: "DevShop Engineering".into(), member_count: 8, plan: "Team".into(), monthly_cost: 160.0, members: vec![
            TeamMember { email: "bob@devshop.io".into(), role: "Owner".into(), joined_at: Utc.with_ymd_and_hms(2025,8,3,14,0,0).unwrap() },
            TeamMember { email: "anna@devshop.io".into(), role: "Admin".into(), joined_at: Utc.with_ymd_and_hms(2025,8,5,10,0,0).unwrap() },
            TeamMember { email: "carlos@devshop.io".into(), role: "Member".into(), joined_at: Utc.with_ymd_and_hms(2025,9,1,9,0,0).unwrap() },
        ], usage_tokens: 15_200_000 },
        TeamRecord { id: "team_002".into(), name: "Agency Creative".into(), member_count: 5, plan: "Team".into(), monthly_cost: 100.0, members: vec![
            TeamMember { email: "frank@agency.io".into(), role: "Owner".into(), joined_at: Utc.with_ymd_and_hms(2025,9,12,8,0,0).unwrap() },
            TeamMember { email: "gina@agency.io".into(), role: "Member".into(), joined_at: Utc.with_ymd_and_hms(2025,10,1,10,0,0).unwrap() },
        ], usage_tokens: 8_700_000 },
        TeamRecord { id: "team_003".into(), name: "StartupCo".into(), member_count: 3, plan: "Team".into(), monthly_cost: 60.0, members: vec![
            TeamMember { email: "carol@startup.co".into(), role: "Owner".into(), joined_at: Utc.with_ymd_and_hms(2026,1,10,9,0,0).unwrap() },
            TeamMember { email: "dan@startup.co".into(), role: "Member".into(), joined_at: Utc.with_ymd_and_hms(2026,1,12,14,0,0).unwrap() },
        ], usage_tokens: 2_300_000 },
    ]
}
