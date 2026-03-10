use anyhow::{anyhow, Context};
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub struct ApiClient {
    pub server_url: String,
    pub token: String,
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
        Self {
            server_url: server_url.trim_end_matches('/').to_string(),
            token: token.trim().to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn fetch_dashboard(&self) -> anyhow::Result<DashboardStats> {
        self.get_json("dashboard").await
    }

    pub async fn fetch_users(&self) -> anyhow::Result<Vec<UserRecord>> {
        self.get_json("users").await
    }

    pub async fn fetch_gateway(&self) -> anyhow::Result<GatewayStats> {
        self.get_json("gateway").await
    }

    pub async fn fetch_relay(&self) -> anyhow::Result<RelayStats> {
        self.get_json("relay").await
    }

    pub async fn fetch_sync(&self) -> anyhow::Result<SyncStats> {
        self.get_json("sync").await
    }

    pub async fn fetch_teams(&self) -> anyhow::Result<Vec<TeamRecord>> {
        self.get_json("teams").await
    }

    async fn get_json<T>(&self, path: &str) -> anyhow::Result<T>
    where
        T: DeserializeOwned,
    {
        let url = format!("{}/admin/{path}", self.server_url);
        let mut request = self.client.get(&url);
        if !self.token.is_empty() {
            request = request.bearer_auth(&self.token);
        }

        let response = request
            .send()
            .await
            .with_context(|| format!("failed to fetch {url}"))?;
        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("request to {url} failed ({status}): {body}"));
        }

        response
            .json::<T>()
            .await
            .with_context(|| format!("failed to decode JSON from {url}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        http::{header, HeaderMap, HeaderValue, StatusCode},
        routing::get,
        Json, Router,
    };
    use chrono::TimeZone;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn api_client_fetches_all_admin_sections_over_http() {
        let dashboard = sample_dashboard();
        let users = sample_users();
        let gateway = sample_gateway();
        let relay = sample_relay();
        let sync = sample_sync();
        let teams = sample_teams();

        let app = Router::new()
            .route(
                "/admin/dashboard",
                get({
                    let dashboard = dashboard.clone();
                    move || {
                        let dashboard = dashboard.clone();
                        async move { Json(dashboard) }
                    }
                }),
            )
            .route(
                "/admin/users",
                get({
                    let users = users.clone();
                    move || {
                        let users = users.clone();
                        async move { Json(users) }
                    }
                }),
            )
            .route(
                "/admin/gateway",
                get({
                    let gateway = gateway.clone();
                    move || {
                        let gateway = gateway.clone();
                        async move { Json(gateway) }
                    }
                }),
            )
            .route(
                "/admin/relay",
                get({
                    let relay = relay.clone();
                    move || {
                        let relay = relay.clone();
                        async move { Json(relay) }
                    }
                }),
            )
            .route(
                "/admin/sync",
                get({
                    let sync = sync.clone();
                    move || {
                        let sync = sync.clone();
                        async move { Json(sync) }
                    }
                }),
            )
            .route(
                "/admin/teams",
                get({
                    let teams = teams.clone();
                    move || {
                        let teams = teams.clone();
                        async move { Json(teams) }
                    }
                }),
            );

        let base_url = spawn(app).await;
        let api = ApiClient::new(&base_url, "");

        assert_eq!(
            api.fetch_dashboard().await.unwrap().total_users,
            dashboard.total_users
        );
        assert_eq!(api.fetch_users().await.unwrap().len(), users.len());
        assert_eq!(
            api.fetch_gateway().await.unwrap().total_requests,
            gateway.total_requests
        );
        assert_eq!(
            api.fetch_relay().await.unwrap().active_rooms,
            relay.active_rooms
        );
        assert_eq!(
            api.fetch_sync().await.unwrap().total_blobs,
            sync.total_blobs
        );
        assert_eq!(api.fetch_teams().await.unwrap().len(), teams.len());
    }

    #[tokio::test]
    async fn api_client_sends_bearer_token_when_configured() {
        let dashboard = sample_dashboard();
        let app = Router::new().route(
            "/admin/dashboard",
            get({
                let dashboard = dashboard.clone();
                move |headers: HeaderMap| {
                    let dashboard = dashboard.clone();
                    async move {
                        let authorized = headers
                            .get(header::AUTHORIZATION)
                            .and_then(|value: &HeaderValue| value.to_str().ok())
                            == Some("Bearer secret-token");
                        if authorized {
                            Ok::<_, StatusCode>(Json(dashboard))
                        } else {
                            Err(StatusCode::UNAUTHORIZED)
                        }
                    }
                }
            }),
        );

        let base_url = spawn(app).await;
        let api = ApiClient::new(&base_url, "secret-token");
        let fetched = api.fetch_dashboard().await.unwrap();

        assert_eq!(fetched.total_users, dashboard.total_users);
    }

    async fn spawn(router: Router) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        format!("http://{addr}")
    }

    fn sample_dashboard() -> DashboardStats {
        DashboardStats {
            total_users: 8,
            free_users: 3,
            pro_users: 3,
            team_users: 2,
            revenue_estimate: 344.0,
            gateway_requests_today: 14_832,
            gateway_requests_month: 387_291,
            active_relay_connections: 2,
            sync_storage_bytes: 2_147_483_648,
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
        ]
    }

    fn sample_gateway() -> GatewayStats {
        GatewayStats {
            total_requests: 387_291,
            total_tokens: 94_500_000,
            models: vec![ModelUsage {
                model: "gpt-4o".into(),
                requests: 98_000,
                tokens_in: 19_600_000,
                tokens_out: 9_800_000,
            }],
            providers: vec![ProviderCost {
                provider: "OpenAI".into(),
                requests: 98_000,
                cost_usd: 1_274.0,
            }],
            budget_alerts: vec![BudgetAlert {
                user_email: "bob@devshop.io".into(),
                used_pct: 92.0,
                tier: "Team".into(),
            }],
        }
    }

    fn sample_relay() -> RelayStats {
        RelayStats {
            active_rooms: 1,
            connected_devices: 2,
            rooms: vec![RelayRoom {
                room_id: "room-live".into(),
                participants: 2,
                created_at: dt(2026, 2, 27, 9, 0, 0),
                bytes_transferred: 256,
            }],
        }
    }

    fn sample_sync() -> SyncStats {
        SyncStats {
            total_blobs: 3_842,
            storage_used_bytes: 2_147_483_648,
            storage_available_bytes: 10_737_418_240,
            per_user: vec![SyncBlobRecord {
                user_email: "alice@example.com".into(),
                blob_count: 342,
                storage_bytes: 268_435_456,
            }],
            recent_ops: vec![SyncOperation {
                timestamp: dt(2026, 2, 27, 9, 30, 12),
                user_email: "alice@example.com".into(),
                operation: "PUT".into(),
                key: "settings/editor.json".into(),
                bytes: 2_048,
            }],
        }
    }

    fn sample_teams() -> Vec<TeamRecord> {
        vec![TeamRecord {
            id: "team_001".into(),
            name: "DevShop Engineering".into(),
            member_count: 8,
            plan: "Team".into(),
            monthly_cost: 160.0,
            members: vec![TeamMember {
                email: "bob@devshop.io".into(),
                role: "Owner".into(),
                joined_at: dt(2025, 8, 3, 14, 0, 0),
            }],
            usage_tokens: 15_200_000,
        }]
    }

    fn dt(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, minute, second)
            .single()
            .expect("valid timestamp")
    }
}
