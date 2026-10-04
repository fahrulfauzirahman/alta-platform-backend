use alta_kernel::AppError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Claims {
    #[serde(rename = "tid")]
    tenant_id: uuid::Uuid,
    #[serde(rename = "aid", skip_serializing_if = "Option::is_none", default)]
    actor_id: Option<uuid::Uuid>,
    exp: i64,
}

pub fn verify_bearer(token: &str, secret: &str) -> Result<(uuid::Uuid, Option<uuid::Uuid>), AppError> {
    use jsonwebtoken::{decode, DecodingKey, Validation};
    let mut validation = Validation::new(jsonwebtoken::Algorithm::HS256);
    validation.validate_exp = true;
    validation.leeway = 0;
    validation.required_spec_claims.remove("aud");
    let data = decode::<Claims>(token, &DecodingKey::from_secret(secret.as_bytes()), &validation)
        .map_err(|_| AppError::unauthorized("invalid token"))?;
    Ok((data.claims.tenant_id, data.claims.actor_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mint(tenant: uuid::Uuid, actor: Option<uuid::Uuid>, secret: &str, exp_offset_secs: i64) -> String {
        use jsonwebtoken::{encode, EncodingKey, Header};
        let claims = Claims {
            tenant_id: tenant,
            actor_id: actor,
            exp: chrono::Utc::now().timestamp() + exp_offset_secs,
        };
        encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes())).unwrap()
    }

    #[test]
    fn verifies_valid_token() {
        let t = uuid::Uuid::new_v4();
        let a = uuid::Uuid::new_v4();
        let s = "test-secret-for-unit-only";
        let tok = mint(t, Some(a), s, 3600);
        let (bt, ba) = verify_bearer(&tok, s).expect("verify");
        assert_eq!(bt, t);
        assert_eq!(ba, Some(a));
    }

    #[test]
    fn rejects_wrong_secret_and_expired() {
        let t = uuid::Uuid::new_v4();
        let s = "test-secret-for-unit-only";
        let tok = mint(t, None, s, 3600);
        assert!(verify_bearer(&tok, "wrong").is_err());
        let expired = mint(t, None, s, -10);
        assert!(verify_bearer(&expired, s).is_err());
    }
}
