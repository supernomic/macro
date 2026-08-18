use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{Duration, Utc};

use super::*;
use crate::domain::model::{AgentKind, scope_allows};

#[derive(Default)]
struct FakeRepo {
    principals: Mutex<HashMap<Uuid, AgentPrincipal>>,
    tokens: Mutex<HashMap<Uuid, AgentApiToken>>,
}

impl AgentIdentityRepo for FakeRepo {
    async fn insert_principal(&self, principal: &AgentPrincipal) -> Result<()> {
        let mut principals = self.principals.lock().unwrap();
        if principals
            .values()
            .any(|p| p.org_id == principal.org_id && p.slug == principal.slug)
        {
            return Err(IdentityError::SlugTaken);
        }
        principals.insert(principal.id, principal.clone());
        Ok(())
    }

    async fn get_principal(&self, id: Uuid) -> Result<Option<AgentPrincipal>> {
        Ok(self.principals.lock().unwrap().get(&id).cloned())
    }

    async fn get_principal_by_slug(
        &self,
        org_id: Option<i32>,
        slug: &str,
    ) -> Result<Option<AgentPrincipal>> {
        Ok(self
            .principals
            .lock()
            .unwrap()
            .values()
            .find(|p| p.org_id == org_id && p.slug == slug)
            .cloned())
    }

    async fn list_principals(&self, org_id: Option<i32>) -> Result<Vec<AgentPrincipal>> {
        Ok(self
            .principals
            .lock()
            .unwrap()
            .values()
            .filter(|p| p.org_id == org_id)
            .cloned()
            .collect())
    }

    async fn disable_principal(&self, id: Uuid) -> Result<()> {
        if let Some(p) = self.principals.lock().unwrap().get_mut(&id) {
            p.disabled_at = Some(Utc::now());
        }
        Ok(())
    }

    async fn insert_token(&self, token: &AgentApiToken) -> Result<()> {
        self.tokens.lock().unwrap().insert(token.id, token.clone());
        Ok(())
    }

    async fn get_token(&self, id: Uuid) -> Result<Option<AgentApiToken>> {
        Ok(self.tokens.lock().unwrap().get(&id).cloned())
    }

    async fn revoke_token(&self, id: Uuid) -> Result<()> {
        if let Some(t) = self.tokens.lock().unwrap().get_mut(&id) {
            t.revoked_at = Some(Utc::now());
        }
        Ok(())
    }

    async fn touch_token(&self, _id: Uuid) -> Result<()> {
        Ok(())
    }
}

async fn setup() -> (AgentIdentityServiceImpl<FakeRepo>, AgentPrincipal) {
    let service = AgentIdentityServiceImpl::new(FakeRepo::default());
    let principal = service
        .create_principal(CreatePrincipal {
            org_id: Some(1),
            slug: "techops".to_string(),
            display_name: "TechOps Agent".to_string(),
            kind: AgentKind::DomainAgent,
        })
        .await
        .unwrap();
    (service, principal)
}

#[tokio::test]
async fn mint_and_verify_roundtrip() {
    let (service, principal) = setup().await;
    let minted = service
        .mint_token(MintToken {
            principal_id: principal.id,
            name: "runtime".to_string(),
            scopes: vec!["tool:search".to_string(), "ledger:append".to_string()],
            expires_at: None,
        })
        .await
        .unwrap();

    let verified = service.verify_bearer(&minted.bearer).await.unwrap();
    assert_eq!(verified.principal.id, principal.id);
    assert!(verified.require_scope("tool:search").is_ok());
    assert!(verified.require_scope("ledger:append").is_ok());
    assert!(matches!(
        verified.require_scope("tool:delete_document"),
        Err(IdentityError::MissingScope { .. })
    ));
}

#[tokio::test]
async fn wrong_secret_is_rejected() {
    let (service, principal) = setup().await;
    let minted = service
        .mint_token(MintToken {
            principal_id: principal.id,
            name: "runtime".to_string(),
            scopes: vec!["tool:*".to_string()],
            expires_at: None,
        })
        .await
        .unwrap();

    let forged = format!("{TOKEN_PREFIX}{}.{}", minted.id, "not-the-secret");
    assert!(matches!(
        service.verify_bearer(&forged).await,
        Err(IdentityError::TokenRejected)
    ));
}

#[tokio::test]
async fn revoked_expired_and_disabled_are_rejected() {
    let (service, principal) = setup().await;

    let revoked = service
        .mint_token(MintToken {
            principal_id: principal.id,
            name: "a".to_string(),
            scopes: vec!["tool:*".to_string()],
            expires_at: None,
        })
        .await
        .unwrap();
    service.revoke_token(revoked.id).await.unwrap();
    assert!(service.verify_bearer(&revoked.bearer).await.is_err());

    let expired = service
        .mint_token(MintToken {
            principal_id: principal.id,
            name: "b".to_string(),
            scopes: vec!["tool:*".to_string()],
            expires_at: Some(Utc::now() - Duration::hours(1)),
        })
        .await
        .unwrap();
    assert!(service.verify_bearer(&expired.bearer).await.is_err());

    let live = service
        .mint_token(MintToken {
            principal_id: principal.id,
            name: "c".to_string(),
            scopes: vec!["tool:*".to_string()],
            expires_at: None,
        })
        .await
        .unwrap();
    service.disable_principal(principal.id).await.unwrap();
    assert!(service.verify_bearer(&live.bearer).await.is_err());
}

#[tokio::test]
async fn malformed_bearers_are_rejected() {
    let (service, _) = setup().await;
    for bearer in [
        "",
        "mat_",
        "mat_nope",
        "mat_00000000-0000-0000-0000-000000000000",
        "xyz_a.b",
    ] {
        assert!(matches!(
            service.verify_bearer(bearer).await,
            Err(IdentityError::MalformedToken) | Err(IdentityError::TokenRejected)
        ));
    }
}

#[tokio::test]
async fn empty_scopes_and_bad_slug_are_rejected() {
    let (service, principal) = setup().await;
    assert!(
        service
            .mint_token(MintToken {
                principal_id: principal.id,
                name: "x".to_string(),
                scopes: vec![],
                expires_at: None,
            })
            .await
            .is_err()
    );

    assert!(
        service
            .create_principal(CreatePrincipal {
                org_id: Some(1),
                slug: "Bad Slug!".to_string(),
                display_name: "x".to_string(),
                kind: AgentKind::SuperAgent,
            })
            .await
            .is_err()
    );
}

#[test]
fn scope_grammar() {
    assert!(scope_allows("tool:search", "tool:search"));
    assert!(scope_allows("tool:*", "tool:search"));
    assert!(scope_allows("tool:*", "tool:email:read"));
    assert!(scope_allows("api:documents:*", "api:documents:read"));
    assert!(scope_allows("*", "anything:at:all"));
    assert!(!scope_allows("tool:search", "tool:searchmore"));
    assert!(!scope_allows("tool:search", "tool:search:sub"));
    assert!(!scope_allows("api:documents:read", "api:documents:write"));
    assert!(!scope_allows("tool", "tool:search"));
}
