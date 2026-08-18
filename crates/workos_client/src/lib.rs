#![deny(missing_docs)]
//! WorkOS AuthKit client used to sign companies into Macro and match them
//! to a WorkOS organization in our environment.

mod client;
mod error;
mod ids;
mod models;

pub use client::WorkOsClient;
pub use error::WorkOsClientError;
pub use ids::{WorkOsIdError, WorkOsOrganizationId, WorkOsUserId};
pub use models::{
    AuthenticateWithCodeResponse, AuthorizationUrlParams, PortalIntent, PortalLink, ScreenHint,
    WorkOsOrganization, WorkOsUser,
};

#[cfg(test)]
mod test;
