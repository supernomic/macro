/**
 * Environment configuration for the agents service.
 *
 * Every value is read once at module load. Agent tokens are per-principal
 * (`mat_...`, minted by Macro's agent-identity API) and are the sole
 * capability boundary: a tool registered in this runtime without a matching
 * scope on the token fails at Macro's API boundary by design.
 */

function required(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`missing required environment variable: ${name}`);
  }
  return value;
}

function optional(name: string, fallback: string): string {
  return process.env[name] ?? fallback;
}

/** Deployment environment for Macro API host selection. */
export type MacroEnv = 'local' | 'dev' | 'prod';

export const config = {
  /** Which Macro backend to talk to (`local` | `dev` | `prod`). */
  macroEnv: optional('MACRO_ENV', 'local') as MacroEnv,

  /**
   * Agent bearer token (`mat_...`) for the super agent principal. Scopes on
   * this token define what the super agent can actually do.
   */
  superAgentToken: () => required('MACRO_SUPER_AGENT_TOKEN'),

  /**
   * Base URL of the service hosting the agent-ledger API
   * (document-cognition-service). Defaults to the local DCS port.
   */
  ledgerBaseUrl: optional(
    'MACRO_LEDGER_BASE_URL',
    process.env.MACRO_ENV === 'prod'
      ? 'https://document-cognition.macro.com'
      : process.env.MACRO_ENV === 'dev'
        ? 'https://document-cognition-dev.macro.com'
        : 'http://localhost:8085',
  ),

  /** Default model for the super agent. */
  superAgentModel: optional(
    'MACRO_SUPER_AGENT_MODEL',
    'anthropic/claude-sonnet-4-6',
  ),

  /**
   * Pinned composition id for the super agent. Bump on any change to the
   * base prompt, tool allowlist, model tier, or skill set — evals and
   * training export group by it.
   */
  superAgentCompositionId: optional(
    'MACRO_SUPER_AGENT_COMPOSITION_ID',
    'super-agent/v1',
  ),

  /**
   * Optional agent bearer token for a domain agent principal, read from
   * `MACRO_<SLUG>_AGENT_TOKEN` (slug uppercased, dashes as underscores).
   * A domain agent whose token is absent is not mounted.
   */
  domainAgentToken: (slug: string): string | undefined =>
    process.env[`MACRO_${slug.toUpperCase().replace(/-/g, '_')}_AGENT_TOKEN`],

  /** Slack signing secret for inbound event verification. */
  slackSigningSecret: () => required('SLACK_SIGNING_SECRET'),

  /** Slack bot token (`xoxb-...`) for outbound Web API calls. */
  slackBotToken: () => required('SLACK_BOT_TOKEN'),

  /**
   * Default listening mode for Slack channels without an explicit entry
   * in `SLACK_CHANNEL_MODES`: `mentions_only` | `proactive` | `silent`.
   */
  slackDefaultChannelMode: optional(
    'SLACK_DEFAULT_CHANNEL_MODE',
    'mentions_only',
  ),

  /**
   * JSON map of Slack channel id → mode, e.g.
   * `{"C0123":"proactive","C0456":"silent"}`.
   */
  slackChannelModes: optional('SLACK_CHANNEL_MODES', '{}'),
} as const;
