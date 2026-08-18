use crate::api::{
    attachments::get_chats_for_attachment,
    chats::{chat_history, chat_history_batch_messages},
    citations, health,
    preview::get_batch_preview,
    stream::chat_message::{
        self, ChatMessageError, HttpSendChatMessageRequest, SendChatMessageResponse,
    },
    stream::stop::{
        self as stream_stop, StopChatStreamError, StopChatStreamRequest, StopChatStreamResponse,
    },
    structured_completion::{
        self, StructuredCompletionError, StructuredCompletionRequest, StructuredCompletionResponse,
    },
};
use crate::model::{
    response::attachments::GetChatsForAttachmentResponse,
    stream::{ChatStream, SendChatMessagePayload, StreamError, ToolSet},
};
use agent_feedback::inbound::axum_router::{
    self as feedback_api, ConsentRequest, FeedbackErrorBody, RateRequest,
};
use agent_identity::inbound::axum_router::{
    self as agent_identity_api, CreatePrincipalRequest, IdentityErrorBody, MintTokenRequest,
    MintTokenResponse, PrincipalResponse,
};
use agent_inbox::inbound::axum_router::{self as inbox_api, InboxErrorBody, UserInboxResponse};
use agent_ledger::inbound::axum_router::{
    self as agent_ledger_api, AppendEventsRequest, EventResponse, LedgerErrorBody, NewEventBody,
    OpenSessionRequest, RecordOutcomeRequest, SessionMappingResponse, VerifyChainResponse,
};
use ai_projections::domain::model::{Expiry, ProjectionStatus, RefreshCadence, TargetType};
use ai_projections::inbound::axum_router::upsert_projection::{
    ProjectionStateResponse, UpsertProjectionRequest,
};
use ai_usage::inbound::axum_router::{self as ai_usage_api};
use approvals::inbound::axum_router::{
    self as approvals_api, ApprovalErrorBody, DecideRequest, GateToolCallRequest,
    ReassignApprovalRequest, UserApprovalsResponse,
};
use entity_graph::inbound::axum_router::{
    self as entity_graph_api, GraphErrorBody, Neighbor, UpsertEdgeRequest, UpsertKnowledgeRequest,
    UpsertNodeRequest,
};
use escalations::inbound::axum_router::{
    self as escalations_api, CreateEscalationRequest, EscalationErrorBody, ReassignRequest,
    ResolveRequest, UserEscalationsResponse,
};
use import::inbound::axum_router::{self as import_api, RunImportRequest};
use lifecycle_connectors::inbound::axum_router::{
    self as lifecycle_api, ConnectorErrorBody, IngestRequest, RegisterAccountRequest,
};
use mcp_client::inbound::axum_router::{
    self as mcp_api, AddServerRequest, ServerResponse, StartAuthRequest, StartAuthResponse,
    UpdateServerRequest,
};
use memory::inbound::axum_router::{self as memory_api, MemoryErrorBody, MemoryResponse};
use onboarding::inbound::axum_router::{self as onboarding_api, CompleteOnboardingRequest};
use skill_governance::inbound::axum_router::{
    self as skill_governance_api, DecideProposalRequest, GovernanceErrorBody, ProposeSkillRequest,
    RecordEvalRequest, RefineRequest, UserProposalsResponse,
};
use tenant_extensions::inbound::axum_router::{
    self as extensions_api, ExtensionErrorBody, RegisterExtensionRequest,
};
use ticket_mirrors::inbound::axum_router::{
    self as mirrors_api, MirrorErrorBody, UpsertMirrorRequest,
};
use training_export::inbound::axum_router::{
    self as export_api, ExportErrorBody, RunExportRequest, RunExportResponse,
};

use crate::api::preview::get_batch_preview::{GetBatchPreviewRequest, GetBatchPreviewResponse};

use chat::domain::models::{ChatResponse, GetChatResponse, WebCitation};
use chat::inbound::http::router::{
    self as chat_router, CallToolRequest, CallToolResponse, CreateChatRequest,
    GetChatPermissionsResponse, PatchChatRequest, RejectToolCallRequest, UpdateToolCallRequest,
    UpdateToolResponseRequest,
};

use model::{
    chat::{
        AttachmentMetadata, AttachmentType, Chat, ChatAttachment, ChatHistory, ChatMessage,
        ChatMessageWithAttachments, ConversationRecord, MessageWithAttachments, NewAttachment,
        NewChatMessage, NewMessageAttachment,
    },
    response::{GenericErrorResponse, StringIDResponse},
    version::DocumentCognitionServiceApiVersion,
};

use model::citations::DocumentTextPart;
use models_dcs::api::ChatHistoryBatchMessagesRequest;
use models_permissions::share_permission::channel_share_permission::UpdateOperation;
use utoipa::OpenApi;

// TODO: update to a real license - I added this bc it's required by orval
#[derive(OpenApi)]
#[openapi(
        info(
            title = "Document Cognition Service",
            version = "1.0.0",
            terms_of_service = "https://macro.com/terms",
            license(name = "Proprietary", identifier = "Proprietary"),
        ),
        paths(
            health::health_handler,
            chat_router::get_chat_handler,
            chat_router::create_chat_handler,
            chat_router::copy_chat_handler,
            chat_router::get_chat_permissions_handler,
            chat_router::delete_chat_handler,
            chat_router::permanently_delete_chat_handler,
            chat_router::patch_chat_handler,
            chat_router::revert_delete_handler,
            chat_router::update_tool_call_handler,
            chat_router::update_tool_response_handler,
            chat_router::call_tool_handler,
            chat_router::reject_tool_call_handler,
            get_chats_for_attachment::get_chats_for_attachment_handler,
            citations::get_citation_handler,
            get_batch_preview::handler,
            chat_history::get_chat_history_handler,
            chat_history_batch_messages::get_chat_history_batch_messages_handler,
            chat_message::send_chat_message,
            stream_stop::stop_chat_stream,
            structured_completion::structured_completion,
            memory_api::get_memory_handler,
            agent_identity_api::create_principal_handler::<crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            agent_identity_api::list_principals_handler::<crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            agent_identity_api::disable_principal_handler::<crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            agent_identity_api::mint_token_handler::<crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            agent_identity_api::revoke_token_handler::<crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            agent_ledger_api::open_session_handler::<crate::api::context::DcsAgentLedgerService, agent_ledger::outbound::PgSessionMappingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            agent_ledger_api::find_session_by_thread_handler::<crate::api::context::DcsAgentLedgerService, agent_ledger::outbound::PgSessionMappingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            agent_ledger_api::append_events_handler::<crate::api::context::DcsAgentLedgerService, agent_ledger::outbound::PgSessionMappingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            agent_ledger_api::list_session_events_handler::<crate::api::context::DcsAgentLedgerService, agent_ledger::outbound::PgSessionMappingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            agent_ledger_api::record_outcome_handler::<crate::api::context::DcsAgentLedgerService, agent_ledger::outbound::PgSessionMappingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            agent_ledger_api::verify_chain_handler::<crate::api::context::DcsAgentLedgerService, agent_ledger::outbound::PgSessionMappingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            agent_ledger_api::query_events_handler::<crate::api::context::DcsAgentLedgerService, agent_ledger::outbound::PgSessionMappingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            agent_ledger_api::agent_query_events_handler::<crate::api::context::DcsAgentLedgerService, agent_ledger::outbound::PgSessionMappingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            agent_ledger_api::export_events_handler::<crate::api::context::DcsAgentLedgerService, agent_ledger::outbound::PgSessionMappingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            escalations_api::agent_create_escalation_handler::<crate::api::context::DcsEscalationService, escalations::outbound::PgRoutingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            escalations_api::agent_get_escalation_handler::<crate::api::context::DcsEscalationService, escalations::outbound::PgRoutingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            escalations_api::list_my_escalations_handler::<crate::api::context::DcsEscalationService, escalations::outbound::PgRoutingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            escalations_api::list_team_escalations_handler::<crate::api::context::DcsEscalationService, escalations::outbound::PgRoutingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            escalations_api::get_escalation_handler::<crate::api::context::DcsEscalationService, escalations::outbound::PgRoutingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            escalations_api::claim_escalation_handler::<crate::api::context::DcsEscalationService, escalations::outbound::PgRoutingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            escalations_api::reassign_escalation_handler::<crate::api::context::DcsEscalationService, escalations::outbound::PgRoutingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            escalations_api::resolve_escalation_handler::<crate::api::context::DcsEscalationService, escalations::outbound::PgRoutingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            escalations_api::cancel_escalation_handler::<crate::api::context::DcsEscalationService, escalations::outbound::PgRoutingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            escalations_api::list_transitions_handler::<crate::api::context::DcsEscalationService, escalations::outbound::PgRoutingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            escalations_api::list_rules_handler::<crate::api::context::DcsEscalationService, escalations::outbound::PgRoutingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            escalations_api::upsert_rule_handler::<crate::api::context::DcsEscalationService, escalations::outbound::PgRoutingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            escalations_api::delete_rule_handler::<crate::api::context::DcsEscalationService, escalations::outbound::PgRoutingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            escalations_api::list_experts_handler::<crate::api::context::DcsEscalationService, escalations::outbound::PgRoutingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            escalations_api::upsert_expert_handler::<crate::api::context::DcsEscalationService, escalations::outbound::PgRoutingRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            approvals_api::agent_gate_handler::<crate::api::context::DcsApprovalService, approvals::outbound::PgPolicyRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            approvals_api::agent_get_approval_handler::<crate::api::context::DcsApprovalService, approvals::outbound::PgPolicyRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            approvals_api::agent_cancel_approval_handler::<crate::api::context::DcsApprovalService, approvals::outbound::PgPolicyRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            approvals_api::list_my_approvals_handler::<crate::api::context::DcsApprovalService, approvals::outbound::PgPolicyRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            approvals_api::list_team_approvals_handler::<crate::api::context::DcsApprovalService, approvals::outbound::PgPolicyRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            approvals_api::get_approval_handler::<crate::api::context::DcsApprovalService, approvals::outbound::PgPolicyRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            approvals_api::decide_approval_handler::<crate::api::context::DcsApprovalService, approvals::outbound::PgPolicyRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            approvals_api::reassign_approval_handler::<crate::api::context::DcsApprovalService, approvals::outbound::PgPolicyRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            approvals_api::list_approval_transitions_handler::<crate::api::context::DcsApprovalService, approvals::outbound::PgPolicyRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            approvals_api::list_policies_handler::<crate::api::context::DcsApprovalService, approvals::outbound::PgPolicyRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            approvals_api::upsert_policy_handler::<crate::api::context::DcsApprovalService, approvals::outbound::PgPolicyRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            approvals_api::delete_policy_handler::<crate::api::context::DcsApprovalService, approvals::outbound::PgPolicyRepo, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            skill_governance_api::agent_catalog_handler::<crate::api::context::DcsSkillGovernanceService, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            skill_governance_api::agent_get_skill_handler::<crate::api::context::DcsSkillGovernanceService, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            skill_governance_api::agent_propose_handler::<crate::api::context::DcsSkillGovernanceService, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            skill_governance_api::list_my_proposals_handler::<crate::api::context::DcsSkillGovernanceService, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            skill_governance_api::get_proposal_handler::<crate::api::context::DcsSkillGovernanceService, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            skill_governance_api::decide_proposal_handler::<crate::api::context::DcsSkillGovernanceService, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            skill_governance_api::rollback_proposal_handler::<crate::api::context::DcsSkillGovernanceService, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            skill_governance_api::record_eval_handler::<crate::api::context::DcsSkillGovernanceService, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            skill_governance_api::refine_handler::<crate::api::context::DcsSkillGovernanceService, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            inbox_api::list_my_inbox_handler::<crate::api::context::DcsInboxService, crate::api::context::DcsAuthorizationService>,
            entity_graph_api::upsert_node_handler::<crate::api::context::DcsGraphService, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            entity_graph_api::upsert_edge_handler::<crate::api::context::DcsGraphService, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            entity_graph_api::neighbors_handler::<crate::api::context::DcsGraphService, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            entity_graph_api::upsert_knowledge_handler::<crate::api::context::DcsGraphService, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            lifecycle_api::register_account_handler::<crate::api::context::DcsConnectorService, crate::api::context::DcsAuthorizationService>,
            lifecycle_api::ingest_handler::<crate::api::context::DcsConnectorService, crate::api::context::DcsAuthorizationService>,
            mirrors_api::upsert_mirror_handler::<crate::api::context::DcsMirrorService, crate::api::context::DcsAuthorizationService>,
            mirrors_api::disconnect_mirror_handler::<crate::api::context::DcsMirrorService, crate::api::context::DcsAuthorizationService>,
            extensions_api::register_extension_handler::<crate::api::context::DcsExtensionService, crate::api::context::DcsAuthorizationService>,
            extensions_api::activate_extension_handler::<crate::api::context::DcsExtensionService, crate::api::context::DcsAuthorizationService>,
            extensions_api::rollback_extension_handler::<crate::api::context::DcsExtensionService, crate::api::context::DcsAuthorizationService>,
            extensions_api::disable_extension_handler::<crate::api::context::DcsExtensionService, crate::api::context::DcsAuthorizationService>,
            export_api::run_export_handler::<crate::api::context::DcsExportService, crate::api::context::DcsAuthorizationService>,
            feedback_api::agent_rate_handler::<crate::api::context::DcsFeedbackService, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            feedback_api::agent_list_ratings_handler::<crate::api::context::DcsFeedbackService, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            feedback_api::agent_get_consent_handler::<crate::api::context::DcsFeedbackService, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            feedback_api::user_set_consent_handler::<crate::api::context::DcsFeedbackService, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            feedback_api::user_rate_handler::<crate::api::context::DcsFeedbackService, crate::api::context::DcsAgentIdentityService, crate::api::context::DcsAuthorizationService>,
            import_api::get_state_handler,
            import_api::run_import_handler,
            import_api::retry_gather_handler,
            import_api::dismiss_run_handler,
            onboarding_api::get_state_handler,
            onboarding_api::complete_handler,
            ai_usage_api::get_usage_handler,
            ai_usage_api::set_pricing_handler,
            ai_projections::inbound::axum_router::upsert_projection::handler::<crate::api::context::DcsAiProjectionService>,
            mcp_api::list_servers,
            mcp_api::add_server,
            mcp_api::update_server,
            mcp_api::delete_server,
            mcp_api::start_auth,
            mcp_api::client_metadata,
            mcp_api::auth_callback
        ),
        components(
            schemas(
                DocumentCognitionServiceApiVersion,
                // Generic
                StringIDResponse,
                GenericErrorResponse,
                // Permissions V2
                models_permissions::share_permission::LinkShare, models_permissions::share_permission::access_level::AccessLevel, models_permissions::share_permission::SharePermissionV2, models_permissions::share_permission::UpdateSharePermissionRequestV2, // Share permission
                models_permissions::share_permission::channel_share_permission::ChannelSharePermission, models_permissions::share_permission::channel_share_permission::UpdateChannelSharePermission, // Channel share permissions

                // Chat
                Chat,
                ChatAttachment,
                AttachmentType,
                ChatHistory,
                ConversationRecord,
                MessageWithAttachments,
                ChatMessage,
                ChatMessageWithAttachments,
                ChatResponse,
                NewChatMessage,
                NewMessageAttachment,
                WebCitation,

                // Chat History
                ChatHistoryBatchMessagesRequest,

                // Citation
                DocumentTextPart,

                // Chat Request
                CreateChatRequest,
                PatchChatRequest,
                AttachmentMetadata,
                // Chat Response
                GetChatPermissionsResponse,
                GetChatResponse,

                // Tool Operations
                UpdateToolCallRequest,
                UpdateToolResponseRequest,
                CallToolRequest,
                CallToolResponse,
                RejectToolCallRequest,

                // Share Permission
                UpdateOperation,

                //stream
                ChatStream,
                SendChatMessagePayload,
                StreamError,

                // Attachments
                GetChatsForAttachmentResponse,
                NewAttachment,

                // Preview
                GetBatchPreviewRequest,
                GetBatchPreviewResponse,

                // Stream HTTP API
                HttpSendChatMessageRequest,
                SendChatMessageResponse,
                ChatMessageError,
                StopChatStreamRequest,
                StopChatStreamResponse,
                StopChatStreamError,
                StreamError,
                ToolSet,
                StructuredCompletionRequest,
                StructuredCompletionResponse,
                StructuredCompletionError,
                agent::structured_output::DynamicSchema,

                // Memory
                MemoryResponse,
                MemoryErrorBody,

                // Agent identity
                CreatePrincipalRequest,
                PrincipalResponse,
                MintTokenRequest,
                MintTokenResponse,
                IdentityErrorBody,

                // Agent ledger
                OpenSessionRequest,
                SessionMappingResponse,
                AppendEventsRequest,
                NewEventBody,
                EventResponse,
                RecordOutcomeRequest,
                VerifyChainResponse,
                LedgerErrorBody,

                // Escalations
                CreateEscalationRequest,
                UserEscalationsResponse,
                ReassignRequest,
                ResolveRequest,
                EscalationErrorBody,
                escalations::domain::model::Escalation,
                escalations::domain::model::EscalationStatus,
                escalations::domain::model::Priority,
                escalations::domain::model::EscalationTransition,
                escalations::domain::model::RoutingRule,
                escalations::domain::model::RouteTarget,
                escalations::domain::model::ExpertProfile,

                // Approval gates
                GateToolCallRequest,
                UserApprovalsResponse,
                DecideRequest,
                ReassignApprovalRequest,
                ApprovalErrorBody,
                approvals::domain::model::ApprovalRequest,
                approvals::domain::model::ApprovalStatus,
                approvals::domain::model::ApprovalTransition,
                approvals::domain::model::GateOutcome,
                approvals::domain::model::PolicyDecision,
                approvals::domain::model::ToolPolicy,

                // Skills governance
                ProposeSkillRequest,
                DecideProposalRequest,
                RecordEvalRequest,
                RefineRequest,
                UserProposalsResponse,
                GovernanceErrorBody,
                skill_governance::domain::model::SkillCatalogEntry,
                skill_governance::domain::model::SkillRecord,
                skill_governance::domain::model::SkillProposal,
                skill_governance::domain::model::SkillEvalRun,
                skill_governance::domain::model::TraceRefinement,
                skill_governance::domain::model::SkillScope,
                skill_governance::domain::model::TrustTier,

                // Unified inbox
                UserInboxResponse,
                InboxErrorBody,
                agent_inbox::domain::model::InboxItem,
                agent_inbox::domain::model::InboxKind,
                agent_inbox::domain::model::InboxView,

                // Entity graph
                UpsertNodeRequest,
                UpsertEdgeRequest,
                UpsertKnowledgeRequest,
                Neighbor,
                GraphErrorBody,
                entity_graph::domain::model::GraphNode,
                entity_graph::domain::model::GraphEdge,
                entity_graph::domain::model::KnowledgeDocument,

                // Lifecycle connectors
                RegisterAccountRequest,
                IngestRequest,
                ConnectorErrorBody,
                lifecycle_connectors::domain::model::ConnectorAccount,
                lifecycle_connectors::domain::model::ConnectorRecord,
                lifecycle_connectors::domain::model::Provider,
                lifecycle_connectors::domain::model::IngestRecord,

                // Ticket mirrors
                UpsertMirrorRequest,
                MirrorErrorBody,
                ticket_mirrors::domain::model::TicketMirror,
                ticket_mirrors::domain::model::MirrorProvider,

                // Tenant extensions
                RegisterExtensionRequest,
                ExtensionErrorBody,
                tenant_extensions::domain::model::TenantExtension,

                // Training export
                RunExportRequest,
                RunExportResponse,
                ExportErrorBody,
                training_export::domain::model::ExportJob,
                training_export::domain::model::ProjectedEvent,
                training_export::domain::model::Projection,
                training_export::domain::model::SharingMode,

                // Feedback sidecar
                RateRequest,
                ConsentRequest,
                FeedbackErrorBody,
                agent_feedback::domain::model::MessageRating,
                agent_feedback::domain::model::ConsentRecord,
                agent_feedback::domain::model::RatingValue,
                agent_feedback::domain::model::FeedbackSharingMode,

                // Import pipeline
                import::domain::models::ImportState,
                import::domain::models::ImportEntity,
                import::domain::models::ImportRun,
                import::domain::models::ImportSource,
                import::domain::models::ImportStatus,
                import::domain::models::Initiator,
                import::domain::models::RunStatus,
                import::domain::models::LinearIssueMeta,
                import::domain::models::NotionDocMeta,
                import::domain::models::SlackChannelMeta,
                import::domain::models::SlackParticipant,
                import::domain::service::RunImportOutcome,
                RunImportRequest,

                // Onboarding
                onboarding::domain::models::OnboardingState,
                onboarding::domain::models::OnboardingRow,
                onboarding::domain::models::OnboardingStatus,
                onboarding::domain::models::ConnectedServer,
                CompleteOnboardingRequest,

                // AI cost
                ai_usage_api::UsageRequest,
                ai_usage_api::SetPricingRequest,
                ai_usage_api::ErrorBody,
                ai_usage::UsageSummary,
                ai_usage::FeatureUsage,
                ai_usage::CompletionUsage,
                ai_usage::Usage,
                ai_usage::Price,
                ai_usage::AiFeature,

                // AI projections
                UpsertProjectionRequest,
                ProjectionStateResponse,
                TargetType,
                RefreshCadence,
                Expiry,
                ProjectionStatus,

                // MCP
                ServerResponse,
                AddServerRequest,
                UpdateServerRequest,
                StartAuthRequest,
                StartAuthResponse,
                model_error_response::ErrorResponse,
            ),
        ),
        tags(
            (name = "macro document cognition service", description = "Document Cognition Service")
        )
    )]
pub struct ApiDoc;
