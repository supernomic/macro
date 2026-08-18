//! WorkOS identifier newtypes.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Invalid WorkOS identifier.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WorkOsIdError {
    /// Value is not a WorkOS organization id (`org_…`).
    #[error("invalid WorkOS organization id")]
    InvalidOrganizationId,
    /// Value is not a WorkOS user id (`user_…`).
    #[error("invalid WorkOS user id")]
    InvalidUserId,
}

/// WorkOS organization id (`org_…`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkOsOrganizationId(String);

impl WorkOsOrganizationId {
    /// Parse a WorkOS organization id.
    pub fn parse(value: impl AsRef<str>) -> Result<Self, WorkOsIdError> {
        let value = value.as_ref();
        if value.starts_with("org_") && value.len() > 4 {
            Ok(Self(value.to_string()))
        } else {
            Err(WorkOsIdError::InvalidOrganizationId)
        }
    }

    /// Underlying id string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for WorkOsOrganizationId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for WorkOsOrganizationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// WorkOS user id (`user_…`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkOsUserId(String);

impl WorkOsUserId {
    /// Parse a WorkOS user id.
    pub fn parse(value: impl AsRef<str>) -> Result<Self, WorkOsIdError> {
        let value = value.as_ref();
        if value.starts_with("user_") && value.len() > 5 {
            Ok(Self(value.to_string()))
        } else {
            Err(WorkOsIdError::InvalidUserId)
        }
    }

    /// Underlying id string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for WorkOsUserId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for WorkOsUserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parses_organization_id() {
        let id = WorkOsOrganizationId::parse("org_01H945H0YD4F97JN9MATX7BYAG").unwrap();
        assert_eq!(id.as_str(), "org_01H945H0YD4F97JN9MATX7BYAG");
    }

    #[test]
    fn rejects_invalid_organization_id() {
        assert_eq!(
            WorkOsOrganizationId::parse("user_01H945H0YD4F97JN9MATX7BYAG"),
            Err(WorkOsIdError::InvalidOrganizationId)
        );
        assert_eq!(
            WorkOsOrganizationId::parse("org_"),
            Err(WorkOsIdError::InvalidOrganizationId)
        );
    }

    #[test]
    fn parses_user_id() {
        let id = WorkOsUserId::parse("user_01E4ZCR3C56J083X43JQXF3JK5").unwrap();
        assert_eq!(id.as_str(), "user_01E4ZCR3C56J083X43JQXF3JK5");
    }

    #[test]
    fn rejects_invalid_user_id() {
        assert_eq!(
            WorkOsUserId::parse("org_01E4ZCR3C56J083X43JQXF3JK5"),
            Err(WorkOsIdError::InvalidUserId)
        );
        assert_eq!(
            WorkOsUserId::parse("user_"),
            Err(WorkOsIdError::InvalidUserId)
        );
    }
}
