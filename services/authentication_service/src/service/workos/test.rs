use super::*;
use workos_client::WorkOsOrganizationId;

#[test]
fn workos_idp_sentinel_is_stable() {
    assert_eq!(WORKOS_IDP_ID, "workos");
}

#[test]
fn organization_id_parse_rejects_non_workos_values() {
    assert!(WorkOsOrganizationId::parse("org_01H945H0YD4F97JN9MATX7BYAG").is_ok());
    assert!(WorkOsOrganizationId::parse("not-an-org").is_err());
}

#[test]
fn company_domains_skip_addresses_and_generic_providers() {
    let matches = [
        "acme.com".to_string(),
        "ada@acme.com".to_string(),
        "gmail.com".to_string(),
        "notadomain".to_string(),
    ];
    assert_eq!(
        company_domains_from_email_matches(&matches),
        vec!["acme.com"]
    );
}
