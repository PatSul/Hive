use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
#[cfg(test)]
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
    pub tier: String,
}

#[cfg(test)]
pub fn create_jwt(user_id: &str, tier: &str, secret: &str) -> anyhow::Result<String> {
    let expiration = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as usize + 24 * 3600; // 24 hours

    let claims = Claims {
        sub: user_id.to_owned(),
        tier: tier.to_owned(),
        exp: expiration,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;

    Ok(token)
}

pub fn validate_jwt(token: &str, secret: &str) -> anyhow::Result<Claims> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    let claims = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?
    .claims;

    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_validate_round_trip() {
        let token = create_jwt("user-123", "pro", "secret").unwrap();
        let claims = validate_jwt(&token, "secret").unwrap();

        assert_eq!(claims.sub, "user-123");
        assert_eq!(claims.tier, "pro");
        assert!(claims.exp > 0);
    }

    #[test]
    fn validate_rejects_wrong_secret() {
        let token = create_jwt("user-123", "pro", "secret").unwrap();
        let err = validate_jwt(&token, "wrong-secret").unwrap_err();

        assert!(err.to_string().contains("InvalidSignature"));
    }

    #[test]
    fn validate_rejects_expired_token() {
        let token = encode(
            &Header::default(),
            &Claims {
                sub: "user-123".into(),
                tier: "free".into(),
                exp: 1,
            },
            &EncodingKey::from_secret("secret".as_bytes()),
        )
        .unwrap();

        let err = validate_jwt(&token, "secret").unwrap_err();
        assert!(err.to_string().contains("ExpiredSignature"));
    }
}
