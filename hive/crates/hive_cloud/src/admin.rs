use crate::auth::validate_jwt;
use crate::relay::{RelayService, RelaySnapshot};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    routing::get,
};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone)]
pub struct AdminState {
    relay: Arc<RelayService>,
    seed: AdminSeedData,
    jwt_secret: Option<String>,
}

#[derive(Clone)]
struct AdminSeedData {
    dashboard_requests_today: u64,
    users: Vec<UserRecord>,
    gateway: GatewayStats,
    sync: SyncStats,
    teams: Vec<TeamRecord>,
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

impl AdminState {
    pub fn new(relay: Arc<RelayService>) -> Self {
        Self {
            relay,
            seed: AdminSeedData::default(),
            jwt_secret: std::env::var("HIVE_CLOUD_ADMIN_JWT_SECRET")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        }
    }

    #[cfg(test)]
    fn new_for_test(relay: Arc<RelayService>, jwt_secret: Option<&str>) -> Self {
        Self {
            relay,
            seed: AdminSeedData::default(),
            jwt_secret: jwt_secret.map(ToOwned::to_owned),
        }
    }
}

impl Default for AdminSeedData {
    fn default() -> Self {
        Self {
            dashboard_requests_today: 14_832,
            users: sample_users(),
            gateway: sample_gateway(),
            sync: sample_sync(),
            teams: sample_teams(),
        }
    }
}

pub fn router(state: Arc<AdminState>) -> Router {
    Router::new()
        .route("/dashboard", get(get_dashboard))
        .route("/users", get(get_users))
        .route("/gateway", get(get_gateway))
        .route("/relay", get(get_relay))
        .route("/sync", get(get_sync))
        .route("/teams", get(get_teams))
        .with_state(state)
}

async fn get_dashboard(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
) -> Result<Json<DashboardStats>, StatusCode> {
    authorize(&headers, &state)?;
    let relay = state.relay.snapshot().await;
    Ok(Json(build_dashboard(&state.seed, &relay)))
}

async fn get_users(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<UserRecord>>, StatusCode> {
    authorize(&headers, &state)?;
    Ok(Json(state.seed.users.clone()))
}

async fn get_gateway(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
) -> Result<Json<GatewayStats>, StatusCode> {
    authorize(&headers, &state)?;
    Ok(Json(state.seed.gateway.clone()))
}

async fn get_relay(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
) -> Result<Json<RelayStats>, StatusCode> {
    authorize(&headers, &state)?;
    Ok(Json(convert_relay_snapshot(state.relay.snapshot().await)))
}

async fn get_sync(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
) -> Result<Json<SyncStats>, StatusCode> {
    authorize(&headers, &state)?;
    Ok(Json(state.seed.sync.clone()))
}

async fn get_teams(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<TeamRecord>>, StatusCode> {
    authorize(&headers, &state)?;
    Ok(Json(state.seed.teams.clone()))
}

fn authorize(headers: &HeaderMap, state: &AdminState) -> Result<(), StatusCode> {
    let secret = state
        .jwt_secret
        .as_deref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?; // No secret = reject, not allow

    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    validate_jwt(token, secret)
        .map(|_| ())
        .map_err(|_| StatusCode::UNAUTHORIZED)
}

fn build_dashboard(seed: &AdminSeedData, relay: &RelaySnapshot) -> DashboardStats {
    let (free_users, pro_users, team_users) =
        seed.users
            .iter()
            .fold((0_u64, 0_u64, 0_u64), |(free, pro, team), user| match user
                .tier
                .to_ascii_lowercase()
                .as_str()
            {
                "free" => (free + 1, pro, team),
                "team" => (free, pro, team + 1),
                _ => (free, pro + 1, team),
            });

    let revenue_estimate =
        (pro_users as f64 * 8.0) + seed.teams.iter().map(|team| team.monthly_cost).sum::<f64>();

    DashboardStats {
        total_users: seed.users.len() as u64,
        free_users,
        pro_users,
        team_users,
        revenue_estimate,
        gateway_requests_today: seed.dashboard_requests_today,
        gateway_requests_month: seed.gateway.total_requests,
        active_relay_connections: relay.connected_devices,
        sync_storage_bytes: seed.sync.storage_used_bytes,
    }
}

fn convert_relay_snapshot(snapshot: RelaySnapshot) -> RelayStats {
    RelayStats {
        active_rooms: snapshot.active_rooms,
        connected_devices: snapshot.connected_devices,
        rooms: snapshot
            .rooms
            .into_iter()
            .map(|room| RelayRoom {
                room_id: room.room_id,
                participants: room.participants,
                created_at: room.created_at,
                bytes_transferred: room.bytes_transferred,
            })
            .collect(),
    }
}

fn sample_users() -> Vec<UserRecord> {
    vec![
        UserRecord {
            id: "usr_001".into(),
            email: "alice@example.com".into(),
            tier: "Pro".into(),
            created_at: dt(2025, 6, 15, 10, 30, 0),
            last_login: dt(2026, 2, 27, 8, 12, 0),
            usage_tokens: 1_250_000,
        },
        UserRecord {
            id: "usr_002".into(),
            email: "bob@devshop.io".into(),
            tier: "Team".into(),
            created_at: dt(2025, 8, 3, 14, 0, 0),
            last_login: dt(2026, 2, 26, 22, 45, 0),
            usage_tokens: 3_800_000,
        },
        UserRecord {
            id: "usr_003".into(),
            email: "carol@startup.co".into(),
            tier: "Free".into(),
            created_at: dt(2026, 1, 10, 9, 0, 0),
            last_login: dt(2026, 2, 25, 17, 30, 0),
            usage_tokens: 48_000,
        },
        UserRecord {
            id: "usr_004".into(),
            email: "dave@bigcorp.com".into(),
            tier: "Pro".into(),
            created_at: dt(2025, 11, 20, 11, 15, 0),
            last_login: dt(2026, 2, 27, 6, 0, 0),
            usage_tokens: 2_100_000,
        },
        UserRecord {
            id: "usr_005".into(),
            email: "eve@freelance.dev".into(),
            tier: "Free".into(),
            created_at: dt(2026, 2, 1, 16, 45, 0),
            last_login: dt(2026, 2, 27, 9, 30, 0),
            usage_tokens: 12_500,
        },
        UserRecord {
            id: "usr_006".into(),
            email: "frank@agency.io".into(),
            tier: "Team".into(),
            created_at: dt(2025, 9, 12, 8, 0, 0),
            last_login: dt(2026, 2, 26, 20, 10, 0),
            usage_tokens: 5_400_000,
        },
        UserRecord {
            id: "usr_007".into(),
            email: "grace@uni.edu".into(),
            tier: "Free".into(),
            created_at: dt(2026, 1, 25, 13, 20, 0),
            last_login: dt(2026, 2, 24, 11, 0, 0),
            usage_tokens: 5_200,
        },
        UserRecord {
            id: "usr_008".into(),
            email: "heidi@consulting.biz".into(),
            tier: "Pro".into(),
            created_at: dt(2025, 7, 1, 9, 0, 0),
            last_login: dt(2026, 2, 27, 7, 45, 0),
            usage_tokens: 980_000,
        },
    ]
}

fn sample_gateway() -> GatewayStats {
    GatewayStats {
        total_requests: 387_291,
        total_tokens: 94_500_000,
        models: vec![
            ModelUsage {
                model: "claude-sonnet-4-20250514".into(),
                requests: 142_000,
                tokens_in: 28_400_000,
                tokens_out: 14_200_000,
            },
            ModelUsage {
                model: "gpt-4o".into(),
                requests: 98_000,
                tokens_in: 19_600_000,
                tokens_out: 9_800_000,
            },
            ModelUsage {
                model: "claude-3-haiku-20240307".into(),
                requests: 85_000,
                tokens_in: 8_500_000,
                tokens_out: 4_250_000,
            },
            ModelUsage {
                model: "gemini-2.0-flash".into(),
                requests: 62_291,
                tokens_in: 6_229_100,
                tokens_out: 3_114_550,
            },
        ],
        providers: vec![
            ProviderCost {
                provider: "Anthropic".into(),
                requests: 227_000,
                cost_usd: 1_842.50,
            },
            ProviderCost {
                provider: "OpenAI".into(),
                requests: 98_000,
                cost_usd: 1_274.00,
            },
            ProviderCost {
                provider: "Google".into(),
                requests: 62_291,
                cost_usd: 312.45,
            },
        ],
        budget_alerts: vec![
            BudgetAlert {
                user_email: "bob@devshop.io".into(),
                used_pct: 92.0,
                tier: "Team".into(),
            },
            BudgetAlert {
                user_email: "dave@bigcorp.com".into(),
                used_pct: 87.5,
                tier: "Pro".into(),
            },
            BudgetAlert {
                user_email: "frank@agency.io".into(),
                used_pct: 78.0,
                tier: "Team".into(),
            },
        ],
    }
}

fn sample_sync() -> SyncStats {
    SyncStats {
        total_blobs: 3_842,
        storage_used_bytes: 2_147_483_648,
        storage_available_bytes: 10_737_418_240,
        per_user: vec![
            SyncBlobRecord {
                user_email: "alice@example.com".into(),
                blob_count: 342,
                storage_bytes: 268_435_456,
            },
            SyncBlobRecord {
                user_email: "bob@devshop.io".into(),
                blob_count: 891,
                storage_bytes: 536_870_912,
            },
            SyncBlobRecord {
                user_email: "dave@bigcorp.com".into(),
                blob_count: 1_205,
                storage_bytes: 805_306_368,
            },
            SyncBlobRecord {
                user_email: "frank@agency.io".into(),
                blob_count: 624,
                storage_bytes: 402_653_184,
            },
            SyncBlobRecord {
                user_email: "heidi@consulting.biz".into(),
                blob_count: 180,
                storage_bytes: 134_217_728,
            },
        ],
        recent_ops: vec![
            SyncOperation {
                timestamp: dt(2026, 2, 27, 9, 30, 12),
                user_email: "alice@example.com".into(),
                operation: "PUT".into(),
                key: "settings/editor.json".into(),
                bytes: 2_048,
            },
            SyncOperation {
                timestamp: dt(2026, 2, 27, 9, 28, 45),
                user_email: "bob@devshop.io".into(),
                operation: "PUT".into(),
                key: "snippets/rust.json".into(),
                bytes: 15_360,
            },
            SyncOperation {
                timestamp: dt(2026, 2, 27, 9, 25, 0),
                user_email: "dave@bigcorp.com".into(),
                operation: "GET".into(),
                key: "workspaces/project-x.json".into(),
                bytes: 8_192,
            },
            SyncOperation {
                timestamp: dt(2026, 2, 27, 9, 22, 33),
                user_email: "frank@agency.io".into(),
                operation: "DELETE".into(),
                key: "cache/old-models.bin".into(),
                bytes: 0,
            },
            SyncOperation {
                timestamp: dt(2026, 2, 27, 9, 20, 10),
                user_email: "heidi@consulting.biz".into(),
                operation: "PUT".into(),
                key: "themes/custom-dark.json".into(),
                bytes: 4_096,
            },
        ],
    }
}

fn sample_teams() -> Vec<TeamRecord> {
    vec![
        TeamRecord {
            id: "team_001".into(),
            name: "DevShop Engineering".into(),
            member_count: 8,
            plan: "Team".into(),
            monthly_cost: 160.0,
            members: vec![
                TeamMember {
                    email: "bob@devshop.io".into(),
                    role: "Owner".into(),
                    joined_at: dt(2025, 8, 3, 14, 0, 0),
                },
                TeamMember {
                    email: "anna@devshop.io".into(),
                    role: "Admin".into(),
                    joined_at: dt(2025, 8, 5, 10, 0, 0),
                },
                TeamMember {
                    email: "carlos@devshop.io".into(),
                    role: "Member".into(),
                    joined_at: dt(2025, 9, 1, 9, 0, 0),
                },
            ],
            usage_tokens: 15_200_000,
        },
        TeamRecord {
            id: "team_002".into(),
            name: "Agency Creative".into(),
            member_count: 5,
            plan: "Team".into(),
            monthly_cost: 100.0,
            members: vec![
                TeamMember {
                    email: "frank@agency.io".into(),
                    role: "Owner".into(),
                    joined_at: dt(2025, 9, 12, 8, 0, 0),
                },
                TeamMember {
                    email: "gina@agency.io".into(),
                    role: "Member".into(),
                    joined_at: dt(2025, 10, 1, 10, 0, 0),
                },
            ],
            usage_tokens: 8_700_000,
        },
        TeamRecord {
            id: "team_003".into(),
            name: "StartupCo".into(),
            member_count: 3,
            plan: "Team".into(),
            monthly_cost: 60.0,
            members: vec![
                TeamMember {
                    email: "carol@startup.co".into(),
                    role: "Owner".into(),
                    joined_at: dt(2026, 1, 10, 9, 0, 0),
                },
                TeamMember {
                    email: "dan@startup.co".into(),
                    role: "Member".into(),
                    joined_at: dt(2026, 1, 12, 14, 0, 0),
                },
            ],
            usage_tokens: 2_300_000,
        },
    ]
}

fn dt(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, second)
        .single()
        .expect("valid timestamp")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::create_jwt;
    use reqwest::StatusCode as ReqwestStatusCode;
    use tokio::net::TcpListener;

    #[test]
    fn dashboard_aggregates_seed_and_relay_state() {
        let seed = AdminSeedData::default();
        let relay = RelaySnapshot {
            active_rooms: 2,
            connected_devices: 5,
            rooms: Vec::new(),
        };

        let dashboard = build_dashboard(&seed, &relay);
        assert_eq!(dashboard.total_users, 8);
        assert_eq!(dashboard.free_users, 3);
        assert_eq!(dashboard.pro_users, 3);
        assert_eq!(dashboard.team_users, 2);
        assert_eq!(dashboard.active_relay_connections, 5);
        assert_eq!(
            dashboard.gateway_requests_month,
            seed.gateway.total_requests
        );
        assert_eq!(dashboard.sync_storage_bytes, seed.sync.storage_used_bytes);
    }

    #[tokio::test]
    async fn admin_routes_reject_when_no_secret_configured() {
        let relay = Arc::new(RelayService::default());
        let router = router(Arc::new(AdminState::new_for_test(relay, None)));
        let base_url = spawn(router).await;

        let response = reqwest::get(format!("{base_url}/dashboard")).await.unwrap();
        assert_eq!(
            response.status(),
            ReqwestStatusCode::SERVICE_UNAVAILABLE,
            "Requests must be rejected when no JWT secret is configured"
        );
    }

    #[tokio::test]
    async fn admin_routes_require_jwt_when_secret_is_configured() {
        let relay = Arc::new(RelayService::default());
        let router = router(Arc::new(AdminState::new_for_test(relay, Some("secret"))));
        let base_url = spawn(router).await;

        let response = reqwest::get(format!("{base_url}/users")).await.unwrap();
        assert_eq!(response.status(), ReqwestStatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn admin_routes_accept_valid_bearer_token() {
        let relay = Arc::new(RelayService::default());
        relay
            .seed_room_for_test("room-live", &["node-a", "node-b"], 256)
            .await;

        let router = router(Arc::new(AdminState::new_for_test(relay, Some("secret"))));
        let base_url = spawn(router).await;
        let token = create_jwt("admin-user", "team", "secret").unwrap();
        let client = reqwest::Client::new();

        let relay_stats = client
            .get(format!("{base_url}/relay"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .json::<RelayStats>()
            .await
            .unwrap();

        assert_eq!(relay_stats.active_rooms, 1);
        assert_eq!(relay_stats.connected_devices, 2);
        assert_eq!(relay_stats.rooms[0].room_id, "room-live");
    }

    async fn spawn(router: Router) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        format!("http://{addr}")
    }
}
