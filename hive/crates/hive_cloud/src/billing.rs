use anyhow::Context;
use stripe::{
    Client, CustomerId, ListSubscriptions, Subscription, SubscriptionStatus,
    SubscriptionStatusFilter,
};

pub struct BillingService {
    client: Client,
}

impl BillingService {
    pub fn new(secret_key: &str) -> Self {
        Self {
            client: Client::new(secret_key),
        }
    }

    pub async fn check_subscription(&self, customer_id: &str) -> anyhow::Result<String> {
        let customer_id = customer_id
            .parse::<CustomerId>()
            .with_context(|| format!("invalid Stripe customer id `{customer_id}`"))?;

        let mut params = ListSubscriptions::new();
        params.customer = Some(customer_id);
        params.status = Some(SubscriptionStatusFilter::All);
        params.limit = Some(10);

        let subscriptions = Subscription::list(&self.client, &params)
            .await
            .context("failed to fetch subscriptions from Stripe")?;

        let tier = subscriptions
            .data
            .iter()
            .map(|subscription| tier_for_status(&subscription.status))
            .max_by_key(|tier| tier_priority(tier))
            .unwrap_or("free");

        Ok(tier.to_string())
    }
}

fn tier_for_status(status: &SubscriptionStatus) -> &'static str {
    match status {
        SubscriptionStatus::Active | SubscriptionStatus::Trialing | SubscriptionStatus::PastDue => {
            "pro"
        }
        _ => "free",
    }
}

fn tier_priority(tier: &str) -> u8 {
    match tier {
        "pro" => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_subscription_statuses_map_to_pro() {
        assert_eq!(tier_for_status(&SubscriptionStatus::Active), "pro");
        assert_eq!(tier_for_status(&SubscriptionStatus::Trialing), "pro");
        assert_eq!(tier_for_status(&SubscriptionStatus::PastDue), "pro");
    }

    #[test]
    fn inactive_subscription_statuses_map_to_free() {
        assert_eq!(tier_for_status(&SubscriptionStatus::Canceled), "free");
        assert_eq!(tier_for_status(&SubscriptionStatus::Incomplete), "free");
        assert_eq!(tier_for_status(&SubscriptionStatus::Unpaid), "free");
    }

    #[tokio::test]
    async fn invalid_customer_ids_are_rejected_before_network_calls() {
        let billing = BillingService::new("sk_test_123");
        let error = billing
            .check_subscription("not-a-customer-id")
            .await
            .unwrap_err();

        assert!(error.to_string().contains("invalid Stripe customer id"));
    }
}
