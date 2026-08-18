use super::*;

fn roundtrip_scope(scope: SkillScope) {
    assert_eq!(SkillScope::parse(scope.as_str()), Some(scope));
    assert_eq!(scope.to_string().parse::<SkillScope>().ok(), Some(scope));
}

fn roundtrip_tier(tier: TrustTier) {
    assert_eq!(TrustTier::parse(tier.as_str()), Some(tier));
    assert_eq!(tier.to_string().parse::<TrustTier>().ok(), Some(tier));
}

fn roundtrip_status(status: OkfStatus) {
    assert_eq!(OkfStatus::parse(status.as_str()), Some(status));
    assert_eq!(status.to_string().parse::<OkfStatus>().ok(), Some(status));
}

#[test]
fn skill_scope_storage_strings_roundtrip() {
    roundtrip_scope(SkillScope::User);
    roundtrip_scope(SkillScope::Team);
    roundtrip_scope(SkillScope::Org);
    roundtrip_scope(SkillScope::Platform);
    assert_eq!(SkillScope::parse("personal"), None);
}

#[test]
fn trust_tier_storage_strings_roundtrip() {
    roundtrip_tier(TrustTier::Builtin);
    roundtrip_tier(TrustTier::Verified);
    roundtrip_tier(TrustTier::Community);
    roundtrip_tier(TrustTier::Untrusted);
    assert_eq!(TrustTier::parse("trusted"), None);
}

#[test]
fn okf_status_storage_strings_roundtrip() {
    roundtrip_status(OkfStatus::Draft);
    roundtrip_status(OkfStatus::Active);
    roundtrip_status(OkfStatus::Deprecated);
    roundtrip_status(OkfStatus::Archived);
    assert_eq!(OkfStatus::parse("live"), None);
}

#[test]
fn okf_frontmatter_carries_typed_status() {
    let fm = OkfFrontmatter {
        kind: "skill".to_string(),
        sources: vec!["doc:abc".to_string()],
        generated: true,
        verified: false,
        status: OkfStatus::Draft,
        stale_after: None,
        content_hash: "deadbeef".to_string(),
    };
    assert_eq!(fm.status.as_str(), "draft");
    assert!(!fm.verified);
}
