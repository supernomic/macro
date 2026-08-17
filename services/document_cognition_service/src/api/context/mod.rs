use crate::config::Config;
use crate::service::ai_stream_registry::AiStreamRegistry;
use ai_tools::{
    AiToolSet, ToolCallToolContext, ToolDocumentService, ToolDocumentToolContext, ToolEmailService,
    ToolEmailToolContext, ToolEntityAccessService, ToolPropertiesToolContext, ToolServiceContext,
    ToolSoupService,
};
use attachment::provider::AttachmentProvider;
use axum::extract::FromRef;
use channels::inbound::attachment::ChannelAttachmentService;
use channels::outbound::pg_channels_repo::PgChannelsRepo;
use chat::domain::service::MessageServiceImpl;
use chat::inbound::attachment::ChatAttachmentService;
use chat::outbound::postgres::PgChatRepo;
use connection_gateway::service::connection::ConnectionRepo;
use document_storage_service_client::DocumentStorageServiceClient;
use documents::inbound::attachment::DocumentAttachmentService;
use email::inbound::attachment::EmailAttachmentService;
use entity_access::{domain::service::EntityAccessServiceImpl, outbound::PgAccessRepository};
use macro_auth::InternalApiKey;
use macro_authorization::{
    MacroAuthJwtValidator, MacroAuthorizationServiceImpl, MacroAuthorizationState,
};
use notification::domain::service::SqsNotificationIngress;
use notification::outbound::queue::SqsQueue;
use notification::outbound::websocket::ConnectionGatewayClient;
use search_service_client::SearchServiceClient;
use sqlx::PgPool;
use static_file::inbound::attachment::StaticFileAttachmentService;
use static_file::outbound::CdnStaticFileRepo;
use std::sync::{Arc, OnceLock};
use stream::domain::StreamRepo;

/// Type alias for the entity access service.
pub type DcsEntityAccessService = EntityAccessServiceImpl<PgAccessRepository>;

/// Type alias for the authorization service.
pub type DcsAuthorizationService = MacroAuthorizationServiceImpl<MacroAuthJwtValidator>;

/// Type alias for the roles-and-permissions service backed by MacroDB.
pub type DcsUserPermissionsService =
    roles_and_permissions::domain::service::UserRolesAndPermissionsServiceImpl<
        roles_and_permissions::outbound::pgpool::MacroDB,
        roles_and_permissions::outbound::pgpool::MacroDB,
    >;

/// Type alias for the chat model entitlement extractor wired to DCS services.
pub type DcsChatModelAccess = chat::inbound::http::extractors::ChatModelAccess<
    DcsAuthorizationService,
    DcsUserPermissionsService,
>;

/// Type alias for the attachment provider wired to concrete DCS services.
pub type DcsAttachmentProvider = AttachmentProvider<
    DocumentAttachmentService<ToolDocumentService, ToolEntityAccessService>,
    EmailAttachmentService<ToolEmailService, ToolEntityAccessService>,
    ChatAttachmentService<PgChatRepo, ToolEntityAccessService>,
    ChannelAttachmentService<PgChannelsRepo, ToolEntityAccessService>,
    StaticFileAttachmentService<CdnStaticFileRepo>,
>;

/// Kafka-backed event broker with publish tasks tracked for graceful shutdown.
pub type DcsEventBroker = macro_event_broker::MacroEventBrokerService<
    macro_event_broker::KafkaEventPublisher,
    tokio_util::task::TaskTracker,
>;

/// Type alias for the message service wired to concrete DCS services.
pub type DcsMessageService = MessageServiceImpl<PgChatRepo, DcsAttachmentProvider, DcsEventBroker>;

#[cfg(test)]
mod test;
#[cfg(test)]
pub use test::test_api_context;
pub(crate) type NotificationIngressType = SqsNotificationIngress<SqsQueue>;

pub type DcsMemoryService =
    memory::domain::service::MemoryServiceImpl<memory::outbound::pg_memory_repo::PgMemoryRepo>;

/// The agent identity service wired to the Postgres identity repo.
pub type DcsAgentIdentityService = agent_identity::domain::service::AgentIdentityServiceImpl<
    agent_identity::outbound::PgAgentIdentityRepo,
>;

/// The session ledger service wired to the Postgres ledger repo.
pub type DcsAgentLedgerService =
    agent_ledger::domain::service::LedgerServiceImpl<agent_ledger::outbound::PgLedgerRepo>;

/// The agent-facing ledger facade (scope + tenancy policy) over the ledger
/// service and the Postgres session-mapping repo.
pub type DcsAgentLedgerFacade = agent_ledger::domain::facade::AgentLedgerFacade<
    DcsAgentLedgerService,
    agent_ledger::outbound::PgSessionMappingRepo,
>;

/// The escalation service wired to the Postgres repos, MacroDB team
/// membership, the HTTP resume-callback client, and the no-op notifier.
pub type DcsEscalationService = escalations::domain::service::EscalationServiceImpl<
    escalations::outbound::PgEscalationRepo,
    escalations::outbound::PgRoutingRepo,
    escalations::outbound::PgTeamMembership,
    escalations::outbound::HttpCallbackClient,
    escalations::outbound::NoopNotifier,
>;

/// The agent-facing escalation facade (scope + tenancy policy).
pub type DcsEscalationFacade =
    escalations::domain::facade::AgentEscalationFacade<DcsEscalationService>;

/// The approval-gate service wired to the Postgres repos, MacroDB team
/// membership, the HTTP resume-callback client, and the no-op notifier.
pub type DcsApprovalService = approvals::domain::service::ApprovalServiceImpl<
    approvals::outbound::PgApprovalRepo,
    approvals::outbound::PgPolicyRepo,
    approvals::outbound::PgTeamMembership,
    approvals::outbound::HttpCallbackClient,
    approvals::outbound::NoopNotifier,
>;

/// The agent-facing approval facade (scope + tenancy policy).
pub type DcsApprovalFacade = approvals::domain::facade::AgentApprovalFacade<DcsApprovalService>;

/// Governed skills service (catalog, proposals, evals, refinements).
pub type DcsSkillGovernanceService = skill_governance::domain::service::SkillGovernanceServiceImpl<
    skill_governance::outbound::PgSkillRepo,
    skill_governance::outbound::PgProposalRepo,
    skill_governance::outbound::PgTeamMembership,
>;

/// Agent-facing skills facade (scope + tenancy policy).
pub type DcsSkillGovernanceFacade =
    skill_governance::domain::facade::AgentSkillFacade<DcsSkillGovernanceService>;

/// Entity-graph service wired to Postgres.
pub type DcsGraphService =
    entity_graph::domain::service::GraphServiceImpl<entity_graph::outbound::PgGraphRepo>;

/// Agent-facing graph facade (scope + tenancy policy).
pub type DcsGraphFacade = entity_graph::domain::facade::AgentGraphFacade<DcsGraphService>;

/// Lifecycle connector service (Okta / Iru / Meraki → entity graph).
pub type DcsConnectorService = lifecycle_connectors::domain::service::ConnectorServiceImpl<
    lifecycle_connectors::outbound::PgConnectorRepo,
    lifecycle_connectors::outbound::EntityGraphIngest<DcsGraphService>,
>;

/// Ticket-mirror service (Zendesk / Jira) with a no-op external client until
/// HTTP adapters land.
pub type DcsMirrorService = ticket_mirrors::domain::service::MirrorServiceImpl<
    ticket_mirrors::outbound::PgMirrorRepo,
    ticket_mirrors::outbound::NoopTicketClient,
>;

/// Per-tenant extension registry (activate / rollback / disable).
pub type DcsExtensionService = tenant_extensions::domain::service::ExtensionServiceImpl<
    tenant_extensions::outbound::PgExtensionRepo,
>;

/// Training-export projections over the session ledger.
pub type DcsExportService = training_export::domain::service::ExportServiceImpl<
    training_export::outbound::LedgerServiceReader<DcsAgentLedgerService>,
    training_export::outbound::PgExportJobRepo,
>;

/// Feedback sidecar (editable ratings + consent).
pub type DcsFeedbackService = agent_feedback::domain::service::FeedbackServiceImpl<
    agent_feedback::outbound::PgRatingRepo,
    agent_feedback::outbound::PgConsentRepo,
>;

/// Agent-facing feedback facade (scope checks).
pub type DcsFeedbackFacade =
    agent_feedback::domain::facade::AgentFeedbackFacade<DcsFeedbackService>;

/// The AI cost service wired to the Postgres usage repo.
pub type DcsUsageService =
    ai_usage::domain::service::UsageServiceImpl<ai_usage::outbound::PgUsageRepo>;

/// The AI projections service wired to the Postgres projection repo, the SQS
/// materialization queue, and the connection-gateway update notifier.
pub type DcsAiProjectionService =
    ai_projections::domain::ai_projection_service::AiProjectionServiceImpl<
        ai_projections::outbound::ai_projection_repo::AiProjectionRepositoryImpl,
        sqs_client::SQS,
        ai_projections::outbound::agent_generator::AgentProjectionGenerator,
        ai_projections::outbound::gateway_notifier::GatewayProjectionNotifier,
    >;

/// Concrete MCP router state for DCS.
pub type DcsMcpRouterState = mcp_client::inbound::McpRouterState<
    mcp_client::outbound::pg_server_repo::PgServerRepo,
    mcp_client::outbound::oauth::OAuthService<
        mcp_client::outbound::pg_server_repo::PgServerRepo,
        mcp_client::outbound::redis_state_store::RedisOAuthStateStore,
    >,
    DcsAuthorizationService,
>;

/// The import pipeline service, shared between the import router, the chat
/// toolset, and the onboarding flow.
pub type DcsImportService = ai_tools::ToolImportService;

/// The onboarding orchestrator wired to the Postgres repo, the MCP server
/// store, and the import service.
pub type DcsOnboardingService = onboarding::domain::service::OnboardingServiceImpl<
    onboarding::outbound::pg_onboarding_repo::PgOnboardingRepo,
    mcp_client::outbound::pg_server_repo::PgServerRepo,
    DcsImportService,
>;

#[derive(Clone, FromRef)]
pub struct ApiContext {
    pub db: PgPool,
    pub sqs_client: Arc<sqs_client::SQS>,
    pub document_storage_client: Arc<DocumentStorageServiceClient>,
    pub search_service_client: Arc<SearchServiceClient>,
    pub email_service_client_external: Arc<email_service_client::EmailServiceClientExternal>,
    pub authorization_state: MacroAuthorizationState<DcsAuthorizationService>,
    pub user_permissions_service: Arc<DcsUserPermissionsService>,
    pub config: Arc<Config>,
    pub internal_api_key: InternalApiKey,
    pub notification_ingress_service: Arc<NotificationIngressType>,
    pub connection_repo: Arc<dyn ConnectionRepo>,
    pub connection_gateway_client: Arc<ConnectionGatewayClient>,
    pub soup_service: Arc<ToolSoupService>,
    pub email_service: Arc<ToolEmailService>,
    pub stream_repo: Arc<dyn StreamRepo>,
    pub document_tool_context: ToolDocumentToolContext,
    pub memory_service: Arc<DcsMemoryService>,
    pub agent_identity_service: Arc<DcsAgentIdentityService>,
    pub agent_ledger_service: Arc<DcsAgentLedgerService>,
    pub agent_ledger_facade: Arc<DcsAgentLedgerFacade>,
    pub escalation_service: Arc<DcsEscalationService>,
    pub escalation_facade: Arc<DcsEscalationFacade>,
    pub escalation_routing: Arc<escalations::outbound::PgRoutingRepo>,
    pub approval_service: Arc<DcsApprovalService>,
    pub approval_facade: Arc<DcsApprovalFacade>,
    pub approval_policies: Arc<approvals::outbound::PgPolicyRepo>,
    pub skill_governance_service: Arc<DcsSkillGovernanceService>,
    pub skill_governance_facade: Arc<DcsSkillGovernanceFacade>,
    pub graph_service: Arc<DcsGraphService>,
    pub graph_facade: Arc<DcsGraphFacade>,
    pub connector_service: Arc<DcsConnectorService>,
    pub mirror_service: Arc<DcsMirrorService>,
    pub extension_service: Arc<DcsExtensionService>,
    pub export_service: Arc<DcsExportService>,
    pub feedback_facade: Arc<DcsFeedbackFacade>,
    pub usage_service: Arc<DcsUsageService>,
    pub ai_projections_service: Arc<DcsAiProjectionService>,
    pub properties_tool_context: ToolPropertiesToolContext,
    pub email_tool_context: ToolEmailToolContext,
    pub call_tool_context: ToolCallToolContext,
    pub tool_service_context: ToolServiceContext,
    pub all_tools: Arc<AiToolSet>,
    pub all_tools_prompt: Arc<dyn std::fmt::Display + Send + Sync>,
    pub entity_access_service: Arc<DcsEntityAccessService>,
    pub message_service: Arc<DcsMessageService>,
    pub ai_stream_registry: AiStreamRegistry,
    pub mcp_state: DcsMcpRouterState,
    pub import_service: Arc<DcsImportService>,
    pub onboarding_service: Arc<DcsOnboardingService>,
    /// Kafka-backed macro event broker for publishing domain events.
    pub macro_event_broker: DcsEventBroker,
}

impl FromRef<ApiContext>
    for chat::inbound::http::extractors::UserPermissionsState<DcsUserPermissionsService>
{
    fn from_ref(state: &ApiContext) -> Self {
        chat::inbound::http::extractors::UserPermissionsState(
            state.user_permissions_service.clone(),
        )
    }
}

pub static GLOBAL_CONTEXT: OnceLock<ApiContext> = OnceLock::new();
