use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

fn client(server: &MockServer) -> WorkOsClient {
    WorkOsClient::new_with_base_url(
        "sk_test".into(),
        "client_01TEST".into(),
        "https://auth-service.macro.com/login/workos/callback".into(),
        server.uri(),
    )
}

#[test]
fn noop_is_not_configured() {
    assert!(!WorkOsClient::noop().is_configured());
}

#[test]
fn authorization_url_includes_authkit_and_optional_params() {
    let client = WorkOsClient::new(
        "sk_test".into(),
        "client_01TEST".into(),
        "https://auth-service.macro.com/login/workos/callback".into(),
    );
    let organization_id = WorkOsOrganizationId::parse("org_01H945H0YD4F97JN9MATX7BYAG").unwrap();
    let url = client
        .authorization_url(AuthorizationUrlParams {
            state: Some(r#"{"is_mobile":false}"#),
            login_hint: Some("ada@acme.com"),
            organization_id: Some(&organization_id),
            screen_hint: Some(ScreenHint::SignIn),
        })
        .unwrap();

    assert!(url.starts_with("https://api.workos.com/user_management/authorize?"));
    assert!(url.contains("provider=authkit"));
    assert!(url.contains("client_id=client_01TEST"));
    assert!(url.contains("login_hint=ada%40acme.com"));
    assert!(url.contains("organization_id=org_01H945H0YD4F97JN9MATX7BYAG"));
    assert!(url.contains("screen_hint=sign-in"));
    assert!(
        url.contains(
            "redirect_uri=https%3A%2F%2Fauth-service.macro.com%2Flogin%2Fworkos%2Fcallback"
        )
    );
}

#[test]
fn authorization_url_errors_when_not_configured() {
    let err = WorkOsClient::noop()
        .authorization_url(AuthorizationUrlParams::default())
        .unwrap_err();
    assert!(matches!(err, WorkOsClientError::NotConfigured));
}

#[tokio::test]
async fn authenticate_with_code_parses_user_and_organization() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/user_management/authenticate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "user": {
                "id": "user_01E4ZCR3C56J083X43JQXF3JK5",
                "email": "ada@acme.com",
                "email_verified": true,
                "first_name": "Ada",
                "last_name": "Lovelace"
            },
            "organization_id": "org_01H945H0YD4F97JN9MATX7BYAG",
            "access_token": "access"
        })))
        .mount(&server)
        .await;

    let authenticated = client(&server)
        .authenticate_with_code("code_123", None, Some("macro-test"))
        .await
        .unwrap();

    assert_eq!(
        authenticated.user.id.as_str(),
        "user_01E4ZCR3C56J083X43JQXF3JK5"
    );
    assert_eq!(authenticated.user.email, "ada@acme.com");
    assert_eq!(
        authenticated.organization_id.unwrap().as_str(),
        "org_01H945H0YD4F97JN9MATX7BYAG"
    );
}

#[tokio::test]
async fn create_organization_posts_pending_domains() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/organizations"))
        .and(header("authorization", "Bearer sk_test"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "org_01H945H0YD4F97JN9MATX7BYAG",
            "name": "Acme",
            "domains": [{ "domain": "acme.com" }]
        })))
        .mount(&server)
        .await;

    let organization = client(&server)
        .create_organization("Acme", &["acme.com"])
        .await
        .unwrap();

    assert_eq!(organization.name, "Acme");
    assert_eq!(organization.domains[0].domain, "acme.com");
}

#[tokio::test]
async fn list_organizations_by_domain_filters_query() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/organizations"))
        .and(query_param("domains", "acme.com"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [{
                "id": "org_01H945H0YD4F97JN9MATX7BYAG",
                "name": "Acme",
                "domains": [{ "domain": "acme.com" }]
            }],
            "list_metadata": {}
        })))
        .mount(&server)
        .await;

    let organizations = client(&server)
        .list_organizations_by_domain("acme.com")
        .await
        .unwrap();
    assert_eq!(organizations.len(), 1);
}

#[tokio::test]
async fn generate_portal_link_returns_url() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/portal/generate_link"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "link": "https://id.workos.com/portal/launch?secret=abc"
        })))
        .mount(&server)
        .await;

    let organization_id = WorkOsOrganizationId::parse("org_01H945H0YD4F97JN9MATX7BYAG").unwrap();
    let portal = client(&server)
        .generate_portal_link(&organization_id, PortalIntent::Sso)
        .await
        .unwrap();
    assert!(portal.link.contains("id.workos.com"));
}

#[tokio::test]
async fn api_errors_surface_status_and_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/user_management/authenticate"))
        .respond_with(ResponseTemplate::new(400).set_body_string("invalid_grant"))
        .mount(&server)
        .await;

    let err = client(&server)
        .authenticate_with_code("bad", None, None)
        .await
        .unwrap_err();
    match err {
        WorkOsClientError::Api { status, message } => {
            assert_eq!(status.as_u16(), 400);
            assert_eq!(message, "invalid_grant");
        }
        other => panic!("unexpected error: {other}"),
    }
}
