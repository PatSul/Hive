use stripe::Client;
// In the future: use stripe::resources::Subscription;

pub struct BillingService {
    client: Client,
}

impl BillingService {
    pub fn new(secret_key: &str) -> Self {
        Self {
            client: Client::new(secret_key),
        }
    }

    pub async fn check_subscription(&self, _customer_id: &str) -> anyhow::Result<String> {
        // Stub implementation for now
        Ok("pro".to_string())
    }
}
