// Modulo: src/types.ts
// Descricao: Type aliases and interface definitions extracted from
// `src/App.tsx` in v0.5.9 per `docs/code-split-plan.md` (frontend track).
// Pure data-only extraction — every type definition preserved verbatim
// from App.tsx v0.5.8 (commit cbfc02d). No runtime values, no React
// components, no hooks, no behavior. Only the home of these declarations
// moved.

import type { ComponentType } from 'react';

export type ProtocolSnapshot = {
  name: string;
  size: number;
  lines: number;
  hash: string;
};

export type AgentState = 'ready' | 'blocked' | 'evidence' | 'running';
export type VerbosityMode = 'resumo' | 'detalhado' | 'diagnostico';
export type PhaseState = 'done' | 'active' | 'waiting';
export type ProviderMode = 'cli' | 'api' | 'hybrid';
export type AiCredentialKey = 'openai' | 'anthropic' | 'gemini' | 'deepseek' | 'grok';
export type InitialAgentKey = 'claude' | 'codex' | 'gemini' | 'deepseek' | 'grok';
export type ProviderRateKey = AiCredentialKey;
export type NativeAttachmentProvider = Exclude<AiCredentialKey, 'deepseek' | 'grok'>;
export type CredentialStorageMode = 'local_json' | 'windows_env' | 'cloudflare';
export type CloudflareTokenSource = 'prompt_each_launch' | 'windows_env' | 'local_encrypted';
export type ActiveSection = 'session' | 'protocols' | 'evidence' | 'agents' | 'settings' | 'setup';
export type SettingsTab = 'providers' | 'cloudflare';
export type RunStatus = 'idle' | 'preparing' | 'running' | 'paused' | 'blocked' | 'completed';
export type ActivityLevel = 'summary' | 'detail' | 'diagnostic';
export type NavItem = { section: ActiveSection; label: string; icon: ComponentType<{ size?: number }> };

export type OperationSnapshot = {
  title: string;
  progress: number;
  current: string;
  eta: string;
  status: RunStatus;
};

export type AgentCard = {
  name: string;
  cli: string;
  state: AgentState;
  note: string;
};

export type ActivityItem = {
  level: ActivityLevel;
  time: string;
  title: string;
  detail: string;
};

export type PhaseItem = {
  label: string;
  detail: string;
  state: PhaseState;
};

export type DiscussionRound = {
  round: string;
  status: string;
  note: string;
};

export type EvidenceRow = {
  label: string;
  value: string;
  tone: 'idle' | 'ok' | 'warn' | 'danger' | 'info';
};

export type CloudflarePermissionRow = {
  label: string;
  value: string;
  tone: 'pending' | 'blocked' | 'ok' | 'warn' | 'error';
};

export type BootstrapCheckRow = {
  label: string;
  value: string;
  tone: 'pending' | 'blocked' | 'ok' | 'warn';
};

export type BootstrapConfig = {
  schema_version: number;
  credential_storage_mode: CredentialStorageMode;
  cloudflare_account_id: string | null;
  cloudflare_api_token_source: CloudflareTokenSource;
  cloudflare_api_token_env_var: string;
  cloudflare_persistence_database: string;
  cloudflare_secret_store: string;
  windows_env_prefix: string;
  updated_at: string;
};

export type CloudflareEnvSnapshot = {
  account_id: string | null;
  account_id_env_var: string | null;
  account_id_env_scope: string | null;
  api_token_present: boolean;
  api_token_env_var: string | null;
  api_token_env_scope: string | null;
};

export type DependencyPreflight = {
  checks: BootstrapCheckRow[];
};

export type CloudflareProbeResult = {
  rows: CloudflarePermissionRow[];
};

export type CloudflareProviderStorageRequest = {
  account_id: string;
  api_token: string | null;
  api_token_env_var: string;
  persistence_database: string;
  secret_store: string;
};

export type AiProviderConfig = {
  schema_version: number;
  provider_mode: ProviderMode;
  credential_storage_mode: CredentialStorageMode;
  openai_api_key: string | null;
  anthropic_api_key: string | null;
  gemini_api_key: string | null;
  deepseek_api_key: string | null;
  grok_api_key: string | null;
  openai_api_key_remote: boolean;
  anthropic_api_key_remote: boolean;
  gemini_api_key_remote: boolean;
  deepseek_api_key_remote: boolean;
  grok_api_key_remote: boolean;
  openai_input_usd_per_million: number | null;
  openai_output_usd_per_million: number | null;
  anthropic_input_usd_per_million: number | null;
  anthropic_output_usd_per_million: number | null;
  gemini_input_usd_per_million: number | null;
  gemini_output_usd_per_million: number | null;
  deepseek_input_usd_per_million: number | null;
  deepseek_output_usd_per_million: number | null;
  grok_input_usd_per_million: number | null;
  grok_output_usd_per_million: number | null;
  cloudflare_secret_store_id: string | null;
  cloudflare_secret_store_name: string | null;
  updated_at: string;
};

export type AiProviderProbeRow = {
  label: string;
  value: string;
  tone: 'pending' | 'blocked' | 'ok' | 'warn' | 'error';
};

export type AiProviderProbeResult = {
  rows: AiProviderProbeRow[];
  checked_at: string;
};

export type LinkAuditRow = {
  url: string;
  status: string;
  invalidity: string;
  tone: 'ok' | 'warn' | 'blocked' | 'error';
};

export type LinkAuditResult = {
  urls_found: number;
  checked: number;
  ok: number;
  failed: number;
  rows: LinkAuditRow[];
};

export type EditorialAgentResult = {
  name: string;
  cli: string;
  tone: 'ok' | 'warn' | 'blocked' | 'error';
  status: string;
  duration_ms: number;
  exit_code: number | null;
  role: string;
  output_path: string;
  usage_input_tokens?: number | null;
  usage_output_tokens?: number | null;
  cost_usd?: number | null;
  cost_estimated?: boolean | null;
};

export type EditorialSessionResult = {
  run_id: string;
  session_dir: string;
  final_markdown_path: string | null;
  session_minutes_path: string;
  prompt_path: string;
  protocol_path: string;
  draft_path: string | null;
  agents: EditorialAgentResult[];
  consensus_ready: boolean;
  status: string;
  active_agents: InitialAgentKey[];
  max_session_cost_usd: number | null;
  max_session_minutes: number | null;
  observed_cost_usd: number | null;
  links_path: string | null;
  attachments_manifest_path: string | null;
  human_log_path: string | null;
};

export type PromptAttachmentPayload = {
  name: string;
  media_type: string | null;
  size_bytes: number;
  data_base64: string;
};

export type AttachmentDeliveryPlan = {
  attachment: PromptAttachmentPayload;
  nativeProviders: NativeAttachmentProvider[];
  manifestProviders: AiCredentialKey[];
  fallbackReason: string | null;
};

export type SessionRunOptions = {
  activeAgents: InitialAgentKey[];
  maxSessionCostUsd: number | null;
  maxSessionMinutes: number | null;
  attachments: PromptAttachmentPayload[];
  links: string[];
};

export type ResumableSessionInfo = {
  run_id: string;
  session_name: string;
  session_dir: string;
  prompt_path: string;
  protocol_path: string;
  draft_path: string | null;
  final_markdown_path: string | null;
  next_round: number;
  last_activity_unix: number;
  artifact_count: number;
  protocol_lines: number;
  status: string;
  saved_active_agents: InitialAgentKey[];
  saved_initial_agent: string | null;
  saved_max_session_cost_usd: number | null;
  saved_max_session_minutes: number | null;
};

export type ProtocolReadingGate = {
  agent: string;
  progress: number;
  status: string;
};
