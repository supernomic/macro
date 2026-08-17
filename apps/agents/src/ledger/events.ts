/**
 * TypeScript mirror of the ledger event vocabulary defined in
 * `crates/agent_ledger/src/domain/model.rs`.
 *
 * Wire shape: serde adjacently-tagged — `{ "type": "<snake_case variant>",
 * "data": { ... } }` (`request_header` uses the struct directly as `data`).
 * Keep this file in lockstep with the Rust enum; it is the shared contract
 * every lane codes against.
 */

/** Why a turn ended. Mirrors `TurnEndReason` (internally tagged on `kind`). */
export type TurnEndReason =
  | { kind: 'completed' }
  | { kind: 'error'; message: string }
  | { kind: 'max_tokens' }
  | { kind: 'aborted'; cause: string }
  | { kind: 'escalated'; escalation_id: string }
  | { kind: 'interrupted' };

/** Where a user-role message came from. Mirrors `UserMessageSource`. */
export type UserMessageSource =
  | 'human'
  | 'injected_context'
  | 'escalation_reply'
  | 'schedule'
  | 'agent';

/** Token accounting for one model request. Mirrors `TokenUsage`. */
export interface TokenUsage {
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens?: number;
}

/**
 * Snapshot of everything that shapes a model request. Logged on session init
 * and on every change ("model-visible means logged"). Mirrors
 * `RequestHeader`.
 */
export interface RequestHeader {
  rendered_system_prompt: string;
  tool_schemas: unknown;
  provider: string;
  model: string;
  sampling: unknown;
  skill_versions: unknown;
  composition_id: string;
}

/** Structured tool error carried on a `tool_result` event. */
export interface ToolErrorBody {
  code: string;
  message: string;
}

/** The typed event payloads, adjacently tagged. Mirrors `AgentEventPayload`. */
export type AgentEventPayload =
  | { type: 'turn_start'; data: { turn: number } }
  | { type: 'turn_end'; data: { turn: number; reason: TurnEndReason } }
  | { type: 'step_start'; data: { turn: number; step: number } }
  | { type: 'step_end'; data: { turn: number; step: number } }
  | {
      type: 'user_message';
      data: { content: string; source: UserMessageSource };
    }
  | {
      type: 'assistant_message';
      data: {
        content: string;
        provider: string;
        model: string;
        usage?: TokenUsage;
      };
    }
  | {
      type: 'tool_call';
      data: { call_id: string; name: string; arguments_raw: string };
    }
  | {
      type: 'tool_result';
      data: {
        call_id: string;
        name: string;
        content: unknown;
        error?: ToolErrorBody;
      };
    }
  | { type: 'request_header'; data: RequestHeader }
  | {
      type: 'approval_requested';
      data: {
        approval_id: string;
        tool_name: string;
        arguments_digest: string;
      };
    }
  | {
      type: 'approval_decided';
      data: {
        approval_id: string;
        approved: boolean;
        decided_by: string;
        note?: string;
      };
    }
  | {
      type: 'escalation_created';
      data: { escalation_id: string; domain: string };
    }
  | {
      type: 'escalation_resolved';
      data: { escalation_id: string; resolved_by: string };
    }
  | { type: 'skill_injected'; data: { skill_id: string; version: string } }
  | {
      type: 'feedback_record';
      data: { rating?: boolean; note?: string; target_seq?: number };
    }
  | {
      type: 'compaction';
      data: {
        replaced_from_seq: number;
        replaced_to_seq: number;
        summary: string;
      };
    }
  | {
      type: 'session_seed';
      data: { parent_session_id: string; seed_length: number };
    };

/** Who produced an event. Mirrors `ActorKind`. */
export type ActorKind = 'user' | 'agent' | 'system';

/** A new event submitted for appending (`NewEventBody` on the wire). */
export interface NewLedgerEvent {
  payload: AgentEventPayload;
  actor_kind: ActorKind;
  actor_id: string;
  occurred_at?: string;
  source_event_seqs?: number[];
}

/** A stored ledger event as returned by the API (`EventResponse`). */
export interface LedgerEvent {
  session_id: string;
  seq: number;
  event_type: string;
  payload: AgentEventPayload;
  actor_kind: ActorKind;
  actor_id: string;
  org_id: number | null;
  occurred_at: string;
  source_event_seqs: number[];
  hash: string;
}

/** External thread kinds a session can be anchored to. */
export type ExternalThreadKind =
  | 'slack_thread'
  | 'email_thread'
  | 'channel_thread'
  | 'native_chat';

/** Session mapping as returned by the API (`SessionMappingResponse`). */
export interface SessionMapping {
  session_id: string;
  runtime_conversation_id: string;
  external_thread_kind: ExternalThreadKind | null;
  external_thread_key: string | null;
  org_id: number | null;
  agent_principal_id: string;
  created_at: string;
}

/** Terminal session outcomes. Mirrors `SessionOutcome`. */
export type SessionOutcome = 'resolved' | 'unresolved' | 'escalated';
