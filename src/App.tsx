import {
  Activity,
  AlertTriangle,
  Bot,
  CheckCircle2,
  Clock3,
  Database,
  Eye,
  EyeOff,
  FileText,
  FilePlus2,
  GitBranch,
  HardDriveDownload,
  KeyRound,
  ListChecks,
  Link2,
  Play,
  RefreshCw,
  Search,
  Settings,
  ShieldCheck,
  Square,
  Upload,
  Globe2,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import type { ChangeEvent, ComponentType } from 'react';
import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import packageJson from '../package.json';
import { logEvent } from './diagnostics';
import { useEscapeKey } from './hooks/useEscapeKey';

const PostEditor = lazy(() => import('./editor/posteditor/PostEditor'));

const APP_VERSION = `v${packageJson.version}`;

type ProtocolSnapshot = {
  name: string;
  size: number;
  lines: number;
  hash: string;
};

type AgentState = 'ready' | 'blocked' | 'evidence' | 'running';
type VerbosityMode = 'resumo' | 'detalhado' | 'diagnostico';
type PhaseState = 'done' | 'active' | 'waiting';
type ProviderMode = 'cli' | 'api' | 'hybrid';
type AiCredentialKey = 'openai' | 'anthropic' | 'gemini' | 'deepseek';
type InitialAgentKey = 'claude' | 'codex' | 'gemini' | 'deepseek';
type ProviderRateKey = AiCredentialKey;
type NativeAttachmentProvider = Exclude<AiCredentialKey, 'deepseek'>;
type CredentialStorageMode = 'local_json' | 'windows_env' | 'cloudflare';
type CloudflareTokenSource = 'prompt_each_launch' | 'windows_env' | 'local_encrypted';
type ActiveSection = 'session' | 'protocols' | 'evidence' | 'agents' | 'settings' | 'setup';
type SettingsTab = 'providers' | 'cloudflare';
type RunStatus = 'idle' | 'preparing' | 'running' | 'paused' | 'blocked' | 'completed';
type ActivityLevel = 'summary' | 'detail' | 'diagnostic';
type NavItem = { section: ActiveSection; label: string; icon: ComponentType<{ size?: number }> };

type OperationSnapshot = {
  title: string;
  progress: number;
  current: string;
  eta: string;
  status: RunStatus;
};

type AgentCard = {
  name: string;
  cli: string;
  state: AgentState;
  note: string;
};

type ActivityItem = {
  level: ActivityLevel;
  time: string;
  title: string;
  detail: string;
};

type PhaseItem = {
  label: string;
  detail: string;
  state: PhaseState;
};

type DiscussionRound = {
  round: string;
  status: string;
  note: string;
};

type EvidenceRow = {
  label: string;
  value: string;
  tone: 'idle' | 'ok' | 'warn' | 'danger' | 'info';
};

type CloudflarePermissionRow = {
  label: string;
  value: string;
  tone: 'pending' | 'blocked' | 'ok' | 'warn' | 'error';
};

type BootstrapCheckRow = {
  label: string;
  value: string;
  tone: 'pending' | 'blocked' | 'ok' | 'warn';
};

type BootstrapConfig = {
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

type CloudflareEnvSnapshot = {
  account_id: string | null;
  account_id_env_var: string | null;
  account_id_env_scope: string | null;
  api_token_present: boolean;
  api_token_env_var: string | null;
  api_token_env_scope: string | null;
};

type DependencyPreflight = {
  checks: BootstrapCheckRow[];
};

type CloudflareProbeResult = {
  rows: CloudflarePermissionRow[];
};

type CloudflareProviderStorageRequest = {
  account_id: string;
  api_token: string | null;
  api_token_env_var: string;
  persistence_database: string;
  secret_store: string;
};

type AiProviderConfig = {
  schema_version: number;
  provider_mode: ProviderMode;
  credential_storage_mode: CredentialStorageMode;
  openai_api_key: string | null;
  anthropic_api_key: string | null;
  gemini_api_key: string | null;
  deepseek_api_key: string | null;
  openai_api_key_remote: boolean;
  anthropic_api_key_remote: boolean;
  gemini_api_key_remote: boolean;
  deepseek_api_key_remote: boolean;
  openai_input_usd_per_million: number | null;
  openai_output_usd_per_million: number | null;
  anthropic_input_usd_per_million: number | null;
  anthropic_output_usd_per_million: number | null;
  gemini_input_usd_per_million: number | null;
  gemini_output_usd_per_million: number | null;
  deepseek_input_usd_per_million: number | null;
  deepseek_output_usd_per_million: number | null;
  cloudflare_secret_store_id: string | null;
  cloudflare_secret_store_name: string | null;
  updated_at: string;
};

type AiProviderProbeRow = {
  label: string;
  value: string;
  tone: 'pending' | 'blocked' | 'ok' | 'warn' | 'error';
};

type AiProviderProbeResult = {
  rows: AiProviderProbeRow[];
  checked_at: string;
};

type LinkAuditRow = {
  url: string;
  status: string;
  tone: 'ok' | 'warn' | 'blocked' | 'error';
};

type LinkAuditResult = {
  urls_found: number;
  checked: number;
  ok: number;
  failed: number;
  rows: LinkAuditRow[];
};

type EditorialAgentResult = {
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

type EditorialSessionResult = {
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

type PromptAttachmentPayload = {
  name: string;
  media_type: string | null;
  size_bytes: number;
  data_base64: string;
};

type AttachmentDeliveryPlan = {
  attachment: PromptAttachmentPayload;
  nativeProviders: NativeAttachmentProvider[];
  manifestProviders: AiCredentialKey[];
  fallbackReason: string | null;
};

type SessionRunOptions = {
  activeAgents: InitialAgentKey[];
  maxSessionCostUsd: number | null;
  maxSessionMinutes: number | null;
  attachments: PromptAttachmentPayload[];
  links: string[];
};

type ResumableSessionInfo = {
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

type ProtocolReadingGate = {
  agent: string;
  progress: number;
  status: string;
};

const initialAgents: AgentCard[] = [
  { name: 'Claude', cli: 'claude', state: 'blocked', note: 'aguardando sessao editorial' },
  { name: 'Codex', cli: 'codex', state: 'blocked', note: 'aguardando sessao editorial' },
  { name: 'Gemini', cli: 'gemini', state: 'blocked', note: 'aguardando sessao editorial' },
  { name: 'DeepSeek', cli: 'deepseek-api', state: 'blocked', note: 'aguardando chave de API' },
  { name: 'Maestro', cli: 'motor local', state: 'blocked', note: 'aguardando verificacoes iniciais' },
];

const initialEvidenceRows: EvidenceRow[] = [
  { label: 'DOI', value: 'nao iniciado', tone: 'idle' },
  { label: 'Links', value: 'nao iniciado', tone: 'idle' },
  { label: 'ABNT', value: 'nao iniciado', tone: 'idle' },
  { label: 'Quarentena', value: 'nao iniciado', tone: 'idle' },
];

const initialProtocolReadingGates: ProtocolReadingGate[] = [
  { agent: 'Claude', progress: 0, status: 'Aguardando' },
  { agent: 'Codex', progress: 0, status: 'Aguardando' },
  { agent: 'Gemini', progress: 0, status: 'Aguardando' },
  { agent: 'DeepSeek', progress: 0, status: 'Aguardando' },
];

const initialDiscussionRounds: DiscussionRound[] = [
  { round: '--', status: 'Sem rodada', note: 'Submeta um prompt para criar a primeira ata operacional.' },
];

const finalArtifacts = [
  { name: 'texto-final.md', detail: 'somente entregue com unanimidade dos agentes' },
  { name: 'ata-da-sessao.md', detail: 'prompt, protocolo, rounds, divergencias e decisoes' },
];

const importChannels = [
  { provider: 'ChatGPT', pattern: 'chatgpt.com/share/<id>', status: 'snapshot publico' },
  { provider: 'Claude', pattern: 'claude.ai/share/...', status: 'snapshot com artifacts' },
  { provider: 'Gemini', pattern: 'g.co/gemini/share/...', status: 'link publico normalizado' },
];

const contentPipelines = [
  { label: 'Editor PostEditor', value: 'mesma funcionalidade e HTML' },
  { label: 'Markdown puro', value: 'ler + gerar' },
  { label: 'Markdown + HTML', value: 'preservar tabelas e midia' },
  { label: 'PDF', value: 'importar, extrair e exportar' },
  { label: 'D1 mainsite_posts', value: 'sincronizar com BigData' },
];

const webEvidenceTools = [
  { label: 'fetch', value: 'HEAD/GET, redirects, hash' },
  { label: 'curl', value: 'replay com segredos ocultos' },
  { label: 'web search', value: 'provedores configuraveis' },
  { label: 'navegador assistido', value: 'CAPTCHA/login com humano' },
];

const initialBootstrapChecks: BootstrapCheckRow[] = [
  { label: 'WebView2', value: 'verificacao pendente', tone: 'pending' },
  { label: 'Claude CLI', value: 'verificacao pendente', tone: 'pending' },
  { label: 'Codex CLI', value: 'verificacao pendente', tone: 'pending' },
  { label: 'Gemini CLI', value: 'verificacao pendente', tone: 'pending' },
  { label: 'Cloudflare env', value: 'verificacao pendente', tone: 'pending' },
  { label: 'Wrangler', value: 'aguardando autorizacao', tone: 'pending' },
];

const initialCloudflarePermissionChecks: CloudflarePermissionRow[] = [
  { label: 'Token ativo', value: 'pendente de verificacao', tone: 'pending' },
  { label: 'Conta acessivel', value: 'pendente de verificacao', tone: 'pending' },
  { label: 'D1 Read/Edit', value: 'pendente de verificacao', tone: 'pending' },
  { label: 'Secrets Store', value: 'pendente de verificacao', tone: 'pending' },
];

const initialAiProviderChecks: AiProviderProbeRow[] = [
  { label: 'OpenAI / Codex', value: 'pendente de verificacao', tone: 'pending' },
  { label: 'Anthropic / Claude', value: 'pendente de verificacao', tone: 'pending' },
  { label: 'Google / Gemini', value: 'pendente de verificacao', tone: 'pending' },
  { label: 'DeepSeek', value: 'pendente de verificacao', tone: 'pending' },
];

const credentialStorageModes = [
  { mode: 'local_json', label: 'JSON local', detail: 'configuracoes e segredos em JSON ignorado' },
  { mode: 'windows_env', label: 'Env var Windows', detail: 'segredos em env var; configs em JSON' },
  { mode: 'cloudflare', label: 'Cloudflare', detail: 'maestro_db + Secrets Store remoto (execucao local exige MAESTRO_*_API_KEY em env)' },
] satisfies Array<{ mode: CredentialStorageMode; label: string; detail: string }>;

const storageModeSummaries: Record<CredentialStorageMode, { title: string; detail: string }> = {
  local_json: {
    title: 'JSON local',
    detail: 'Tudo fica em arquivos JSON locais ignorados pelo Git.',
  },
  windows_env: {
    title: 'Env var hibrido',
    detail: 'Tokens e API keys ficam em env vars do Windows; demais configuracoes ficam em JSON local.',
  },
  cloudflare: {
    title: 'Cloudflare remoto',
    detail:
      'Configuracoes em D1 maestro_db; segredos centralizados no Cloudflare Secrets Store. Importante: o app nao busca segredos remotos em runtime; para executar peers via API localmente, mantenha MAESTRO_OPENAI_API_KEY / MAESTRO_ANTHROPIC_API_KEY / MAESTRO_GEMINI_API_KEY / MAESTRO_DEEPSEEK_API_KEY em env vars (ou na config local). Esse modo e escolha de armazenamento canonico, nao alimenta execucao local sozinho.',
  },
};

const aiProviderRows = [
  {
    key: 'openai',
    name: 'OpenAI / Codex',
    cli: 'codex',
    secretLabel: 'OpenAI API key',
    meta: 'project id, organization id e model pin opcionais',
  },
  {
    key: 'anthropic',
    name: 'Anthropic / Claude',
    cli: 'claude',
    secretLabel: 'Anthropic API key',
    meta: 'workspace, anthropic-version e model pin',
  },
  {
    key: 'gemini',
    name: 'Google / Gemini',
    cli: 'gemini',
    secretLabel: 'Gemini API key',
    meta: 'Developer API ou Vertex AI, projeto e regiao',
  },
  {
    key: 'deepseek',
    name: 'DeepSeek',
    cli: 'deepseek-api',
    secretLabel: 'DeepSeek API key',
    meta: 'API oficial DeepSeek; melhor modelo disponivel via /models',
  },
] satisfies Array<{
  key: AiCredentialKey;
  name: string;
  cli: string;
  secretLabel: string;
  meta: string;
}>;

const providerRateRows = [
  {
    key: 'openai',
    name: 'OpenAI / Codex',
    hint: 'Usado quando o peer operar por API com uso observado.',
  },
  {
    key: 'anthropic',
    name: 'Anthropic / Claude',
    hint: 'Usado quando o peer operar por API com uso observado.',
  },
  {
    key: 'gemini',
    name: 'Google / Gemini',
    hint: 'Usado quando o peer operar por API com uso observado.',
  },
  {
    key: 'deepseek',
    name: 'DeepSeek',
    hint: 'Obrigatorio para sessoes com DeepSeek via API.',
  },
] satisfies Array<{ key: ProviderRateKey; name: string; hint: string }>;

const initialAgentOptions = [
  { key: 'claude', label: 'Claude', detail: 'primeira versao e revisoes' },
  { key: 'codex', label: 'Codex', detail: 'primeira versao e revisoes' },
  { key: 'gemini', label: 'Gemini', detail: 'primeira versao e revisoes' },
  { key: 'deepseek', label: 'DeepSeek', detail: 'primeira versao e revisoes via API' },
] satisfies Array<{ key: InitialAgentKey; label: string; detail: string }>;

const defaultActiveAgents = initialAgentOptions.map((option) => option.key);
const attachmentLimits = {
  maxFiles: 8,
  maxFileBytes: 25 * 1024 * 1024,
  maxTotalBytes: 75 * 1024 * 1024,
  maxNativeApiBytes: 20 * 1024 * 1024,
};

const verbosityOptions = [
  { mode: 'resumo', label: 'Resumo', icon: EyeOff },
  { mode: 'detalhado', label: 'Detalhado', icon: Eye },
  { mode: 'diagnostico', label: 'Diagnostico', icon: ListChecks },
] satisfies Array<{ mode: VerbosityMode; label: string; icon: ComponentType<{ size?: number }> }>;

const navGroups: Array<{ label: string; items: NavItem[] }> = [
  {
    label: 'Fluxo editorial',
    items: [
      { section: 'session', label: 'Sessao', icon: GitBranch },
      { section: 'protocols', label: 'Protocolos', icon: FileText },
      { section: 'evidence', label: 'Evidencias', icon: Globe2 },
    ],
  },
  {
    label: 'Operacao',
    items: [
      { section: 'agents', label: 'Agentes', icon: Bot },
      { section: 'settings', label: 'Ajustes', icon: Settings },
      { section: 'setup', label: 'Setup', icon: HardDriveDownload },
    ],
  },
];

const navItems: NavItem[] = navGroups.flatMap((group) => group.items);

const settingsTabs = [
  {
    tab: 'providers',
    label: 'Agentes via API',
    detail: 'Chaves, modo e tabela de tarifas',
    icon: KeyRound,
  },
  {
    tab: 'cloudflare',
    label: 'Cloudflare',
    detail: 'Bootstrap, D1 e Secrets Store',
    icon: Database,
  },
] satisfies Array<{ tab: SettingsTab; label: string; detail: string; icon: ComponentType<{ size?: number }> }>;

const idleOperation: OperationSnapshot = {
  title: 'Aguardando sessao editorial',
  progress: 0,
  current: 'Nenhum prompt foi submetido nesta execucao.',
  eta: 'ocioso',
  status: 'idle',
};

const idlePhases: PhaseItem[] = [
  { label: 'Protocolo', detail: 'aguardando prompt', state: 'waiting' },
  { label: 'Verificacoes', detail: 'nao iniciadas', state: 'waiting' },
  { label: 'Agentes', detail: 'nao iniciados', state: 'waiting' },
  { label: 'Entrega', detail: 'bloqueada ate unanimidade', state: 'waiting' },
];

const idleActivityFeed: ActivityItem[] = [
  {
    level: 'summary',
    time: 'pronto',
    title: 'Runtime carregado',
    detail: 'Logs estruturados ativos. Ao submeter um prompt, o monitor deve registrar cada etapa visivel.',
  },
  {
    level: 'diagnostic',
    time: '--:--:--',
    title: 'Diagnostico',
    detail: 'Ao relatar falhas, anexe o arquivo mais recente da pasta data/logs.',
  },
];

function stateLabel(state: AgentState) {
  if (state === 'ready') return 'Aprovado';
  if (state === 'running') return 'Em andamento';
  if (state === 'evidence') return 'Precisa de revisao';
  return 'Aguardando';
}

function stateIcon(state: AgentState) {
  if (state === 'ready') return <CheckCircle2 size={16} />;
  if (state === 'running') return <RefreshCw size={16} />;
  if (state === 'evidence') return <Clock3 size={16} />;
  return <AlertTriangle size={16} />;
}

async function sha256(text: string) {
  const bytes = new TextEncoder().encode(text);
  const buffer = await crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(buffer)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

function formatElapsedTime(totalSeconds: number) {
  const safeSeconds = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(safeSeconds / 3600);
  const minutes = Math.floor((safeSeconds % 3600) / 60);
  const seconds = safeSeconds % 60;
  return [hours, minutes, seconds].map((value) => value.toString().padStart(2, '0')).join(':');
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes.toLocaleString('pt-BR')} B`;
  const kib = bytes / 1024;
  if (kib < 1024) return `${kib.toLocaleString('pt-BR', { maximumFractionDigits: 1 })} KiB`;
  const mib = kib / 1024;
  return `${mib.toLocaleString('pt-BR', { maximumFractionDigits: 1 })} MiB`;
}

function normalizedAttachmentMediaType(attachment: PromptAttachmentPayload) {
  const media = attachment.media_type?.trim().toLowerCase();
  if (!media) return 'application/octet-stream';
  if (media === 'image/jpg') return 'image/jpeg';
  return media;
}

function attachmentExtension(name: string) {
  const lastSegment = name.split(/[\\/]/).pop() ?? name;
  const index = lastSegment.lastIndexOf('.');
  if (index < 0 || index === lastSegment.length - 1) return '';
  return lastSegment.slice(index + 1).toLowerCase();
}

function isTextLikeAttachment(attachment: PromptAttachmentPayload) {
  const media = normalizedAttachmentMediaType(attachment);
  if (
    media.startsWith('text/') ||
    media.includes('json') ||
    media.includes('xml') ||
    media.includes('markdown') ||
    media.includes('csv') ||
    media.includes('yaml')
  ) {
    return true;
  }
  return ['txt', 'md', 'markdown', 'json', 'csv', 'tsv', 'html', 'htm', 'xml', 'yaml', 'yml', 'log'].includes(
    attachmentExtension(attachment.name),
  );
}

function isImageAttachment(attachment: PromptAttachmentPayload) {
  return ['image/png', 'image/jpeg', 'image/webp', 'image/gif'].includes(normalizedAttachmentMediaType(attachment));
}

function isPdfAttachment(attachment: PromptAttachmentPayload) {
  return normalizedAttachmentMediaType(attachment) === 'application/pdf' || attachmentExtension(attachment.name) === 'pdf';
}

function isKnownDocumentAttachment(attachment: PromptAttachmentPayload) {
  if (isTextLikeAttachment(attachment) || isPdfAttachment(attachment)) return true;
  const media = normalizedAttachmentMediaType(attachment);
  if (
    [
      'application/msword',
      'application/rtf',
      'application/vnd.ms-excel',
      'application/vnd.ms-powerpoint',
      'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
      'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
      'application/vnd.openxmlformats-officedocument.presentationml.presentation',
      'application/vnd.oasis.opendocument.text',
      'application/vnd.oasis.opendocument.spreadsheet',
      'application/vnd.oasis.opendocument.presentation',
    ].includes(media)
  ) {
    return true;
  }
  return ['doc', 'docx', 'rtf', 'xls', 'xlsx', 'ppt', 'pptx', 'odt', 'ods', 'odp'].includes(
    attachmentExtension(attachment.name),
  );
}

// Keep this predictor aligned with src-tauri/src/lib.rs provider_supports_native_attachment.
function providerSupportsNativeAttachment(provider: NativeAttachmentProvider, attachment: PromptAttachmentPayload) {
  if (attachment.size_bytes > attachmentLimits.maxNativeApiBytes) return false;
  if (provider === 'openai') return isImageAttachment(attachment) || isKnownDocumentAttachment(attachment);
  if (provider === 'anthropic') return isImageAttachment(attachment) || isPdfAttachment(attachment);
  return (
    isImageAttachment(attachment) ||
    normalizedAttachmentMediaType(attachment).startsWith('audio/') ||
    normalizedAttachmentMediaType(attachment).startsWith('video/') ||
    isPdfAttachment(attachment) ||
    isTextLikeAttachment(attachment) ||
    isKnownDocumentAttachment(attachment)
  );
}

function attachmentDeliveryPlan(
  attachment: PromptAttachmentPayload,
  activeApiProviders: AiCredentialKey[],
): AttachmentDeliveryPlan {
  const nativeProviders = activeApiProviders.filter(
    (provider): provider is NativeAttachmentProvider =>
      provider !== 'deepseek' && providerSupportsNativeAttachment(provider, attachment),
  );
  const manifestProviders = activeApiProviders.filter(
    (provider) => provider === 'deepseek' || !nativeProviders.includes(provider as NativeAttachmentProvider),
  );
  let fallbackReason: string | null = null;
  if (manifestProviders.length > 0 || nativeProviders.length === 0) {
    fallbackReason =
      attachment.size_bytes > attachmentLimits.maxNativeApiBytes
        ? `excede envio nativo (${formatBytes(attachmentLimits.maxNativeApiBytes)})`
        : activeApiProviders.length === 0
          ? 'peers ativos usam CLI'
          : manifestProviders.length > 0 && manifestProviders.every((provider) => provider === 'deepseek')
            ? 'API text-only'
            : nativeProviders.length > 0
              ? 'sem suporte nativo nesses peers'
              : 'tipo sem suporte nativo nos peers API ativos';
  }
  return { attachment, nativeProviders, manifestProviders, fallbackReason };
}

function providerShortLabel(provider: AiCredentialKey) {
  if (provider === 'openai') return 'OpenAI';
  if (provider === 'anthropic') return 'Anthropic';
  if (provider === 'gemini') return 'Gemini';
  return 'DeepSeek';
}

function attachmentDeliveryHint(plan: AttachmentDeliveryPlan) {
  const parts: string[] = [];
  if (plan.nativeProviders.length > 0) {
    parts.push(`Nativo previsto: ${plan.nativeProviders.map(providerShortLabel).join(', ')}`);
  }
  if (plan.manifestProviders.length > 0) {
    const reason = plan.fallbackReason ? ` (${plan.fallbackReason})` : '';
    parts.push(`Manifesto/previews: ${plan.manifestProviders.map(providerShortLabel).join(', ')}${reason}`);
  }
  if (parts.length === 0 && plan.fallbackReason) {
    parts.push(`Manifesto/previews: ${plan.fallbackReason}`);
  }
  return parts.join(' · ');
}

function formatBrazilDateTime(value: Date | number) {
  return new Intl.DateTimeFormat('pt-BR', {
    timeZone: 'America/Sao_Paulo',
    day: '2-digit',
    month: '2-digit',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  }).format(value);
}

function humanizeRunStatus(status: RunStatus) {
  if (status === 'idle') return 'Aguardando';
  if (status === 'preparing') return 'Preparando';
  if (status === 'running') return 'Em andamento';
  if (status === 'paused') return 'Aguardando ajustes';
  if (status === 'completed') return 'Concluido';
  return 'Bloqueado';
}

function operationMeterLabel(status: RunStatus) {
  if (status === 'running') return 'Em andamento';
  if (status === 'completed') return 'Concluido';
  if (status === 'paused') return 'Aguardando ajustes';
  if (status === 'blocked') return 'Bloqueado';
  if (status === 'preparing') return 'Preparando';
  return 'Aguardando';
}

function humanizeAgentStatus(status: string) {
  const normalized = status.trim().toUpperCase();
  if (normalized === 'READY') return 'Aprovado';
  if (normalized === 'NOT_READY') return 'Precisa de ajustes';
  if (normalized === 'NEEDS_EVIDENCE') return 'Precisa de verificacao';
  if (normalized === 'DRAFT_CREATED') return 'Rascunho gerado';
  if (normalized === 'CLI_NOT_FOUND') return 'CLI nao encontrada';
  if (normalized === 'API_KEY_NOT_AVAILABLE') return 'Chave de API ausente';
  if (normalized === 'REMOTE_SECRET_NOT_READABLE') return 'Segredo remoto nao legivel localmente';
  if (normalized === 'READY_UNANIMOUS') return 'Texto liberado';
  if (normalized === 'PAUSED_DRAFT_UNAVAILABLE') return 'Rascunho indisponivel';
  if (normalized === 'TIME_LIMIT_REACHED') return 'Limite de tempo atingido';
  if (normalized === 'COST_LIMIT_REACHED') return 'Limite de custo atingido';
  if (normalized === 'PAUSED_COST_RATES_MISSING') return 'Tarifas de custo ausentes';
  if (normalized === 'ALL_PEERS_FAILING') return 'Todos os peers em erro';
  if (normalized === 'PAUSED_WITH_REAL_AGENT_OUTPUTS') return 'Aguardando ajustes';
  return status
    .replace(/_/g, ' ')
    .toLowerCase()
    .replace(/(^|\s)\S/g, (value) => value.toUpperCase());
}

function humanizeRole(role: string) {
  if (role === 'draft') return 'Rascunho';
  if (role === 'revision') return 'Ajuste';
  if (role === 'review') return 'Revisao';
  return 'Atividade';
}

function agentStateFromTone(tone: EditorialAgentResult['tone']): AgentState {
  if (tone === 'ok') return 'ready';
  if (tone === 'warn') return 'evidence';
  return 'blocked';
}

function agentResultRank(agent: EditorialAgentResult) {
  const match = agent.output_path.match(/round-(\d{3})-/i);
  const round = match ? Number.parseInt(match[1], 10) : 0;
  const roleRank = agent.role === 'review' ? 3 : agent.role === 'revision' ? 2 : agent.role === 'draft' ? 1 : 0;
  return round * 10 + roleRank;
}

function latestAgentResults(agents: EditorialAgentResult[]) {
  const byName = new Map<string, EditorialAgentResult>();
  for (const agent of agents) {
    const current = byName.get(agent.name);
    if (!current || agentResultRank(agent) >= agentResultRank(current)) {
      byName.set(agent.name, agent);
    }
  }
  return ['Claude', 'Codex', 'Gemini', 'DeepSeek']
    .map((name) => byName.get(name))
    .filter((agent): agent is EditorialAgentResult => Boolean(agent));
}

function latestAgentCards(agents: EditorialAgentResult[]): AgentCard[] {
  return latestAgentResults(agents).map((agent) => ({
    name: agent.name,
    cli: agent.cli,
    state: agentStateFromTone(agent.tone),
    note: `${humanizeRole(agent.role)}: ${humanizeAgentStatus(agent.status)}; ${formatElapsedTime(
      Math.round(agent.duration_ms / 1000),
    )}`,
  }));
}

function latestProtocolGateItems(agents: EditorialAgentResult[]): ProtocolReadingGate[] {
  return latestAgentResults(agents).map((agent) => ({
    agent: agent.name,
    progress: agent.tone === 'ok' ? 100 : agent.tone === 'warn' ? 70 : 35,
    status: agent.tone === 'ok' ? 'Protocolo lido na ultima rodada' : humanizeAgentStatus(agent.status),
  }));
}

function countAgentRounds(agents: EditorialAgentResult[]) {
  return new Set(
    agents
      .map((agent) => agent.output_path.match(/round-(\d{3})-/i)?.[1])
      .filter((round): round is string => Boolean(round)),
  ).size;
}

function summarizeAgentResults(agents: EditorialAgentResult[]) {
  const rounds = countAgentRounds(agents);
  const latest = latestAgentResults(agents);
  const latestText = latest.map((agent) => `${agent.name}: ${humanizeAgentStatus(agent.status)}`).join('; ');
  const failures = agents.filter((agent) => agent.tone === 'error' || agent.tone === 'blocked').length;
  const failureText = failures
    ? ` ${failures.toLocaleString('pt-BR')} falha(s) operacional(is) registrada(s) no diagnostico.`
    : '';
  return `${rounds.toLocaleString('pt-BR')} rodada(s) registradas. Ultimo estado: ${latestText || 'sem avaliacao registrada'}.${failureText}`;
}

export function App() {
  const inputRef = useRef<HTMLInputElement>(null);
  const [protocol, setProtocol] = useState<ProtocolSnapshot>({
    name: 'Nenhum protocolo carregado',
    size: 0,
    lines: 0,
    hash: 'aguardando importacao',
  });
  const [protocolText, setProtocolText] = useState('');
  const [sessionName, setSessionName] = useState('Artigo academico sem titulo');
  const [verbosity, setVerbosity] = useState<VerbosityMode>('detalhado');
  const [editorialPrompt, setEditorialPrompt] = useState(
    'Escreva um artigo acadêmico sobre [...], seguindo rigorosa e integralmente o protocolo editorial ativo.',
  );
  const [showPostEditor, setShowPostEditor] = useState(false);
  const [mainSiteHtml, setMainSiteHtml] = useState(
    '<h1>Artigo em preparacao</h1><p style="text-align: justify">Texto inicial para edicao com o mesmo PostEditor usado pelo MainSite.</p>',
  );
  const [providerMode, setProviderMode] = useState<ProviderMode>('hybrid');
  const [initialAgent, setInitialAgent] = useState<InitialAgentKey>('claude');
  const [activeAgents, setActiveAgents] = useState<InitialAgentKey[]>(defaultActiveAgents);
  const [maxSessionCostUsd, setMaxSessionCostUsd] = useState('');
  const [maxSessionMinutes, setMaxSessionMinutes] = useState('');
  const [promptAttachments, setPromptAttachments] = useState<PromptAttachmentPayload[]>([]);
  const [sessionLinks, setSessionLinks] = useState('');
  const [credentialStorageMode, setCredentialStorageMode] = useState<CredentialStorageMode>('local_json');
  const [activeSection, setActiveSection] = useState<ActiveSection>('session');
  const [activeSettingsTab, setActiveSettingsTab] = useState<SettingsTab>('providers');
  const [cloudflareAccountId, setCloudflareAccountId] = useState('');
  const [cloudflareApiToken, setCloudflareApiToken] = useState('');
  const [cloudflareTokenSource, setCloudflareTokenSource] = useState<CloudflareTokenSource>('prompt_each_launch');
  const [cloudflareTokenEnvVar, setCloudflareTokenEnvVar] = useState('MAESTRO_CLOUDFLARE_API_TOKEN');
  const [cloudflareEnvSnapshot, setCloudflareEnvSnapshot] = useState<CloudflareEnvSnapshot | null>(null);
  const [aiCredentials, setAiCredentials] = useState<Record<AiCredentialKey, string>>({
    openai: '',
    anthropic: '',
    gemini: '',
    deepseek: '',
  });
  const [providerInputUsdPerMillion, setProviderInputUsdPerMillion] = useState<Record<ProviderRateKey, string>>({
    openai: '',
    anthropic: '',
    gemini: '',
    deepseek: '',
  });
  const [providerOutputUsdPerMillion, setProviderOutputUsdPerMillion] = useState<Record<ProviderRateKey, string>>({
    openai: '',
    anthropic: '',
    gemini: '',
    deepseek: '',
  });
  const [sessionRunId, setSessionRunId] = useState<string | null>(null);
  const [lastSessionMinutesPath, setLastSessionMinutesPath] = useState<string | null>(null);
  const [operation, setOperation] = useState<OperationSnapshot>(idleOperation);
  // True after the operator confirms the "Parar sessao" button until the
  // backend's session loop observes the cancellation token and returns
  // STOPPED_BY_USER. Disables the button to prevent duplicate signals.
  const [isStopRequested, setIsStopRequested] = useState(false);
  const [phaseItems, setPhaseItems] = useState<PhaseItem[]>(idlePhases);
  const [activityItems, setActivityItems] = useState<ActivityItem[]>(idleActivityFeed);
  const [discussionItems, setDiscussionItems] = useState<DiscussionRound[]>(initialDiscussionRounds);
  const [agentCards, setAgentCards] = useState<AgentCard[]>(initialAgents);
  const [evidenceRows, setEvidenceRows] = useState<EvidenceRow[]>(initialEvidenceRows);
  const [protocolGateItems, setProtocolGateItems] = useState<ProtocolReadingGate[]>(initialProtocolReadingGates);
  const [cloudflarePermissionRows, setCloudflarePermissionRows] = useState<CloudflarePermissionRow[]>(
    initialCloudflarePermissionChecks,
  );
  const [aiProviderRowsState, setAiProviderRowsState] = useState<AiProviderProbeRow[]>(initialAiProviderChecks);
  const [bootstrapRows, setBootstrapRows] = useState<BootstrapCheckRow[]>(initialBootstrapChecks);
  const [bootstrapConfigStatus, setBootstrapConfigStatus] = useState('bootstrap.json ainda nao carregado');
  const [aiConfigStatus, setAiConfigStatus] = useState('Chaves ainda nao carregadas');
  const [isVerifyingCloudflare, setIsVerifyingCloudflare] = useState(false);
  const [isSavingAiConfig, setIsSavingAiConfig] = useState(false);
  const [isVerifyingAiProviders, setIsVerifyingAiProviders] = useState(false);
  const [isAuditingEvidence, setIsAuditingEvidence] = useState(false);
  const [resumeCandidates, setResumeCandidates] = useState<ResumableSessionInfo[]>([]);
  const [showResumePicker, setShowResumePicker] = useState(false);
  const [isResumeLoading, setIsResumeLoading] = useState(false);
  const [useLoadedProtocolForResume, setUseLoadedProtocolForResume] = useState(false);

  // v0.3.14 / audit closure (MEDIUM): ESC dismissal on the ResumeDialog at
  // line 2574. Mirrors the existing Close button (line 2582) — no new
  // dismissal path, no new state. Hook gated by `showResumePicker` so the
  // window listener is detached when the dialog is hidden. In-place edit
  // per docs/code-split-plan.md ("future splits should start with pure
  // helpers, ... without mixing large refactors with behavior changes").
  const handleResumeDialogEscape = useCallback(() => {
    setShowResumePicker(false);
  }, []);
  useEscapeKey(handleResumeDialogEscape, showResumePicker);

  const readyCount = useMemo(() => agentCards.filter((agent) => agent.state === 'ready').length, [agentCards]);
  const visibleActivity = useMemo(() => {
    if (verbosity === 'resumo') return activityItems.slice(0, 1);
    if (verbosity === 'detalhado') return activityItems.filter((item) => item.level !== 'diagnostic');
    return activityItems;
  }, [activityItems, verbosity]);
  const isRunPreparing = operation.status === 'preparing' || operation.status === 'running';
  const runActionLabel =
    operation.status === 'paused' || operation.status === 'blocked' || operation.status === 'completed'
      ? 'Nova sessao'
      : 'Iniciar sessao';
  const formalState = humanizeRunStatus(operation.status);
  const linkEvidenceState = evidenceRows.find((item) => item.label === 'Links')?.value ?? 'nao iniciado';
  const activeNavItem = navItems.find((item) => item.section === activeSection) ?? navItems[0];
  const cloudflareTokenAvailable = cloudflareApiToken.length > 0 || Boolean(cloudflareEnvSnapshot?.api_token_present);
  const operationIndeterminate = operation.status === 'running';
  const operationProgressLabel = operationMeterLabel(operation.status);
  const hasLoadedProtocolForResume = protocolText.trim().length >= 100 && protocol.hash !== 'aguardando importacao';
  const initialAgentLabel = initialAgentOptions.find((option) => option.key === initialAgent)?.label ?? 'Claude';
  const activeAgentLabels = activeAgents
    .map((agent) => initialAgentOptions.find((option) => option.key === agent)?.label ?? agent)
    .join(', ');
  const attachmentTotalBytes = promptAttachments.reduce((total, item) => total + item.size_bytes, 0);
  const providerForAgent: Record<InitialAgentKey, AiCredentialKey> = {
    claude: 'anthropic',
    codex: 'openai',
    gemini: 'gemini',
    deepseek: 'deepseek',
  };
  const agentUsesApi = (agent: InitialAgentKey) => {
    if (providerMode === 'api') return true;
    if (providerMode === 'cli') return false;
    // "hybrid" is deterministic by agent identity: DeepSeek goes API
    // (no CLI integration in maestro-app), the other peers stay on CLI.
    return agent === 'deepseek';
  };
  const providerRatesConfigured = (provider: AiCredentialKey) =>
    providerInputUsdPerMillion[provider].trim().length > 0 &&
    providerOutputUsdPerMillion[provider].trim().length > 0;
  const agentsMissingCostRates = activeAgents.filter(
    (agent) => agentUsesApi(agent) && !providerRatesConfigured(providerForAgent[agent]),
  );
  const costRatesRequired = agentsMissingCostRates.length > 0;
  const activeApiAttachmentProviders = activeAgents
    .filter((agent) => agentUsesApi(agent))
    .map((agent) => providerForAgent[agent])
    .filter((provider, index, providers) => providers.indexOf(provider) === index);
  const attachmentDeliveryPlans = promptAttachments.map((attachment) =>
    attachmentDeliveryPlan(attachment, activeApiAttachmentProviders),
  );

  useEffect(() => {
    if (!activeAgents.includes(initialAgent)) {
      setInitialAgent(activeAgents[0] ?? 'claude');
    }
  }, [activeAgents, initialAgent]);

  useEffect(() => {
    // CLI mode is incompatible with DeepSeek (no CLI integration in maestro-app).
    // Defense in depth: catches config-load AND resume-contract paths that call
    // setActiveAgents/setInitialAgent directly while providerMode is already 'cli'
    // (peer review v0.3.38: codex + deepseek raised this — providerMode-only deps
    // would miss saved-contract restore that injects DeepSeek without flipping mode).
    // Reads activeAgents/initialAgent directly (not via setState updater closure)
    // so the React-hooks/preserve-manual-memoization lint sees them as real deps;
    // both setState calls are guarded so no render loop is possible.
    if (providerMode !== 'cli') return;
    if (activeAgents.includes('deepseek')) {
      const filtered = activeAgents.filter((agent) => agent !== 'deepseek');
      setActiveAgents(filtered.length === 0 ? ['claude'] : filtered);
    }
    if (initialAgent === 'deepseek') {
      setInitialAgent('claude');
    }
  }, [providerMode, activeAgents, initialAgent]);

  useEffect(() => {
    void logEvent({
      level: 'info',
      category: 'ui.session.loaded',
      message: 'editorial dashboard loaded',
      context: {
        session_name: sessionName,
        protocol_name: protocol.name,
        formal_state: 'auditoria_bibliografica',
      },
    });
  }, []);

  useEffect(() => {
    void loadBootstrapConfig();
    void loadAiProviderConfig();
  }, []);

  function activityTimestamp() {
    return new Date().toLocaleTimeString('pt-BR', {
      timeZone: 'America/Sao_Paulo',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });
  }

  function appendActivity(item: Omit<ActivityItem, 'time'>) {
    setActivityItems((current) => [{ ...item, time: activityTimestamp() }, ...current].slice(0, 8));
  }

  async function verifyAgentsNow() {
    try {
      const preflight = await invoke<DependencyPreflight>('dependency_preflight');
      setBootstrapRows(preflight.checks);
      const byLabel = new Map(preflight.checks.map((check) => [check.label, check]));
      setAgentCards((current) =>
        current.map((agent) => {
          const check = byLabel.get(`${agent.name} CLI`);
          if (!check) return agent;
          return {
            ...agent,
            state: check.tone === 'ok' ? 'ready' : check.tone === 'warn' ? 'evidence' : 'blocked',
            note: check.value,
          };
        }),
      );
      appendActivity({
        level: 'detail',
        title: 'Agentes verificados',
        detail: preflight.checks
          .filter((check) => check.label.endsWith('CLI'))
          .map((check) => `${check.label}: ${check.tone}`)
          .join('; '),
      });
      void logEvent({
        level: 'info',
        category: 'agents.preflight.completed',
        message: 'operator verified local agent CLIs',
        context: {
          checks: preflight.checks.map((check) => ({ label: check.label, tone: check.tone })),
        },
      });
    } catch (error) {
      appendActivity({
        level: 'diagnostic',
        title: 'Falha ao verificar agentes',
        detail: 'Consulte o log desta execucao para o erro completo.',
      });
      void logEvent({
        level: 'error',
        category: 'agents.preflight.failed',
        message: 'failed to verify local agent CLIs',
        context: { error },
      });
    }
  }

  async function revalidateRuntime() {
    appendActivity({
      level: 'detail',
      title: 'Revalidacao iniciada',
      detail: 'Conferindo dependencias, configuracoes locais e chaves carregadas.',
    });
    await Promise.all([loadBootstrapConfig(), loadAiProviderConfig(), verifyAgentsNow()]);
  }

  async function openSessionLedger() {
    if (!lastSessionMinutesPath) {
      appendActivity({
        level: 'summary',
        title: 'Ata indisponivel',
        detail: 'Ainda nao ha ata criada nesta sessao do app.',
      });
      return;
    }

    try {
      const openedPath = await invoke<string>('open_data_file', { path: lastSessionMinutesPath });
      appendActivity({
        level: 'detail',
        title: 'Ata aberta',
        detail: openedPath,
      });
      void logEvent({
        level: 'info',
        category: 'session.ledger.opened',
        message: 'operator opened session ledger file',
        context: { path: openedPath },
      });
    } catch (error) {
      appendActivity({
        level: 'diagnostic',
        title: 'Falha ao abrir ata',
        detail: 'O arquivo nao foi aberto; consulte o log desta execucao.',
      });
      void logEvent({
        level: 'error',
        category: 'session.ledger.open_failed',
        message: 'failed to open session ledger file',
        context: { path: lastSessionMinutesPath, error },
      });
    }
  }

  async function auditEvidenceNow() {
    const sourceText = [editorialPrompt, protocolText, mainSiteHtml].join('\n\n');
    setIsAuditingEvidence(true);
    setEvidenceRows((current) =>
      current.map((row) => (row.label === 'Links' ? { ...row, value: 'verificando links', tone: 'info' } : row)),
    );

    try {
      const result = await invoke<LinkAuditResult>('audit_links', {
        request: { text: sourceText },
      });
      setEvidenceRows((current) =>
        current.map((row) => {
          if (row.label !== 'Links') return row;
          if (result.urls_found === 0) {
            return { ...row, value: 'nenhum link encontrado', tone: 'idle' };
          }
          if (result.failed > 0) {
            return {
              ...row,
              value: `${result.failed.toLocaleString('pt-BR')} falhas em ${result.checked.toLocaleString('pt-BR')} links`,
              tone: 'warn',
            };
          }
          return {
            ...row,
            value: `${result.ok.toLocaleString('pt-BR')} links acessiveis`,
            tone: 'ok',
          };
        }),
      );
      appendActivity({
        level: 'detail',
        title: 'Links auditados',
        detail:
          result.urls_found === 0
            ? 'Nenhum link foi encontrado no prompt, protocolo ou texto em edicao.'
            : `${result.ok.toLocaleString('pt-BR')} acessiveis; ${result.failed.toLocaleString('pt-BR')} com falha.`,
      });
      void logEvent({
        level: result.failed > 0 ? 'warn' : 'info',
        category: 'evidence.audit.completed',
        message: 'link evidence audit completed',
        context: {
          urls_found: result.urls_found,
          checked: result.checked,
          ok: result.ok,
          failed: result.failed,
          rows: result.rows.map((row) => ({ url: row.url, tone: row.tone, status: row.status })),
        },
      });
    } catch (error) {
      setEvidenceRows((current) =>
        current.map((row) => (row.label === 'Links' ? { ...row, value: 'falha na auditoria', tone: 'danger' } : row)),
      );
      void logEvent({
        level: 'error',
        category: 'evidence.audit.failed',
        message: 'link evidence audit failed',
        context: { error },
      });
    } finally {
      setIsAuditingEvidence(false);
    }
  }

  function createRunId() {
    return `run-${new Date().toISOString().replace(/[:.]/g, '-')}`;
  }

  function buildBootstrapConfig(nextMode = credentialStorageMode): BootstrapConfig {
    return {
      schema_version: 1,
      credential_storage_mode: nextMode,
      cloudflare_account_id: cloudflareAccountId.trim() || cloudflareEnvSnapshot?.account_id || null,
      cloudflare_api_token_source: cloudflareTokenSource,
      cloudflare_api_token_env_var: cloudflareTokenEnvVar.trim() || 'MAESTRO_CLOUDFLARE_API_TOKEN',
      cloudflare_persistence_database: 'maestro_db',
      cloudflare_secret_store: 'maestro',
      windows_env_prefix: 'MAESTRO_',
      updated_at: new Date().toISOString(),
    };
  }

  async function loadBootstrapConfig() {
    try {
      const [config, envSnapshot] = await Promise.all([
        invoke<BootstrapConfig>('read_bootstrap_config'),
        invoke<CloudflareEnvSnapshot>('cloudflare_env_snapshot'),
      ]);

      setBootstrapRows(
        initialBootstrapChecks.map((row) => ({
          ...row,
          value: row.label === 'WebView2' ? 'ativo pelo runtime Tauri' : 'verificando',
          tone: row.label === 'WebView2' ? 'ok' : row.tone,
        })),
      );
      setCredentialStorageMode(config.credential_storage_mode);
      setCloudflareTokenSource(envSnapshot.api_token_present ? 'windows_env' : config.cloudflare_api_token_source);
      setCloudflareTokenEnvVar(envSnapshot.api_token_env_var ?? config.cloudflare_api_token_env_var);
      setCloudflareEnvSnapshot(envSnapshot);
      if (!cloudflareAccountId.trim() && (envSnapshot.account_id || config.cloudflare_account_id)) {
        setCloudflareAccountId(envSnapshot.account_id ?? config.cloudflare_account_id ?? '');
      }
      setBootstrapConfigStatus(
        `bootstrap.json carregado; token Cloudflare ${
          envSnapshot.api_token_present
            ? `detectado em ${envSnapshot.api_token_env_var} (${envSnapshot.api_token_env_scope ?? 'process'})`
            : 'nao detectado em env var'
        }`,
      );
      void logEvent({
        level: 'info',
        category: 'bootstrap.config.loaded',
        message: 'bootstrap configuration and Cloudflare environment snapshot loaded',
        context: {
          credential_storage_mode: config.credential_storage_mode,
          cloudflare_account_id_source: envSnapshot.account_id_env_var ? 'windows_env' : config.cloudflare_account_id ? 'bootstrap_json' : 'missing',
          cloudflare_account_id_env_scope: envSnapshot.account_id_env_scope ?? 'missing',
          cloudflare_api_token_source: envSnapshot.api_token_present ? 'windows_env' : config.cloudflare_api_token_source,
          cloudflare_api_token_env_var: envSnapshot.api_token_env_var ?? config.cloudflare_api_token_env_var,
          cloudflare_api_token_env_scope: envSnapshot.api_token_env_scope ?? 'missing',
          cloudflare_api_token_present: envSnapshot.api_token_present,
        },
      });
      void invoke<DependencyPreflight>('dependency_preflight')
        .then((preflight) => {
          setBootstrapRows(preflight.checks);
          void logEvent({
            level: 'info',
            category: 'bootstrap.dependency_preflight.completed',
            message: 'background dependency preflight completed',
            context: {
              checks: preflight.checks.map((check) => ({
                label: check.label,
                tone: check.tone,
              })),
            },
          });
        })
        .catch((error) => {
          setBootstrapRows((current) =>
            current.map((row) =>
              row.label === 'WebView2'
                ? row
                : { ...row, value: 'falha na verificacao; consulte diagnostico', tone: 'warn' },
            ),
          );
          void logEvent({
            level: 'warn',
            category: 'bootstrap.dependency_preflight.failed',
            message: 'background dependency preflight failed',
            context: { error },
          });
        });
    } catch (error) {
      setBootstrapConfigStatus('falha ao carregar bootstrap.json');
      void logEvent({
        level: 'error',
        category: 'bootstrap.config.load_failed',
        message: 'failed to load bootstrap configuration',
        context: { error },
      });
    }
  }

  async function persistBootstrapConfig(nextMode = credentialStorageMode) {
    try {
      const saved = await invoke<BootstrapConfig>('write_bootstrap_config', {
        config: buildBootstrapConfig(nextMode),
      });
      setBootstrapConfigStatus(`bootstrap.json salvo em ${saved.updated_at}`);
      void logEvent({
        level: 'info',
        category: 'bootstrap.config.saved',
        message: 'bootstrap configuration saved without secrets',
        context: {
          credential_storage_mode: saved.credential_storage_mode,
          cloudflare_account_id_present: Boolean(saved.cloudflare_account_id),
          cloudflare_api_token_source: saved.cloudflare_api_token_source,
          cloudflare_api_token_env_var: saved.cloudflare_api_token_env_var,
        },
      });
    } catch (error) {
      setBootstrapConfigStatus('falha ao salvar bootstrap.json');
      void logEvent({
        level: 'error',
        category: 'bootstrap.config.save_failed',
        message: 'failed to save bootstrap configuration',
        context: { error },
      });
    }
  }

  function buildAiProviderConfig(nextProviderMode = providerMode): AiProviderConfig {
    return {
      schema_version: 1,
      provider_mode: nextProviderMode,
      credential_storage_mode: credentialStorageMode,
      openai_api_key: aiCredentials.openai.trim() || null,
      anthropic_api_key: aiCredentials.anthropic.trim() || null,
      gemini_api_key: aiCredentials.gemini.trim() || null,
      deepseek_api_key: aiCredentials.deepseek.trim() || null,
      openai_api_key_remote: false,
      anthropic_api_key_remote: false,
      gemini_api_key_remote: false,
      deepseek_api_key_remote: false,
      openai_input_usd_per_million: parseOptionalPositiveNumber(
        providerInputUsdPerMillion.openai,
        'Tarifa OpenAI de entrada',
        10000,
      ),
      openai_output_usd_per_million: parseOptionalPositiveNumber(
        providerOutputUsdPerMillion.openai,
        'Tarifa OpenAI de saida',
        10000,
      ),
      anthropic_input_usd_per_million: parseOptionalPositiveNumber(
        providerInputUsdPerMillion.anthropic,
        'Tarifa Anthropic de entrada',
        10000,
      ),
      anthropic_output_usd_per_million: parseOptionalPositiveNumber(
        providerOutputUsdPerMillion.anthropic,
        'Tarifa Anthropic de saida',
        10000,
      ),
      gemini_input_usd_per_million: parseOptionalPositiveNumber(
        providerInputUsdPerMillion.gemini,
        'Tarifa Gemini de entrada',
        10000,
      ),
      gemini_output_usd_per_million: parseOptionalPositiveNumber(
        providerOutputUsdPerMillion.gemini,
        'Tarifa Gemini de saida',
        10000,
      ),
      deepseek_input_usd_per_million: parseOptionalPositiveNumber(
        providerInputUsdPerMillion.deepseek,
        'Tarifa DeepSeek de entrada',
        10000,
      ),
      deepseek_output_usd_per_million: parseOptionalPositiveNumber(
        providerOutputUsdPerMillion.deepseek,
        'Tarifa DeepSeek de saida',
        10000,
      ),
      cloudflare_secret_store_id: null,
      cloudflare_secret_store_name: null,
      updated_at: new Date().toISOString(),
    };
  }

  function buildCloudflareProviderStorageRequest(): CloudflareProviderStorageRequest {
    return {
      account_id: cloudflareAccountId.trim() || cloudflareEnvSnapshot?.account_id || '',
      api_token: cloudflareApiToken.trim() || null,
      api_token_env_var:
        cloudflareTokenEnvVar.trim() || cloudflareEnvSnapshot?.api_token_env_var || 'MAESTRO_CLOUDFLARE_API_TOKEN',
      persistence_database: 'maestro_db',
      secret_store: 'maestro',
    };
  }

  function aiConfigStorageLabel(mode: CredentialStorageMode) {
    if (mode === 'cloudflare') return 'Cloudflare D1 + Secrets Store';
    if (mode === 'windows_env') return 'env vars do Windows + JSON local';
    return 'data/config/ai-providers.json';
  }

  async function loadAiProviderConfig() {
    try {
      const config = await invoke<AiProviderConfig>('read_ai_provider_config');
      setProviderMode(config.provider_mode);
      setAiCredentials({
        openai: config.openai_api_key ?? '',
        anthropic: config.anthropic_api_key ?? '',
        gemini: config.gemini_api_key ?? '',
        deepseek: config.deepseek_api_key ?? '',
      });
      applyProviderRatesFromConfig(config);
      const remoteCount = [
        config.openai_api_key_remote,
        config.anthropic_api_key_remote,
        config.gemini_api_key_remote,
        config.deepseek_api_key_remote,
      ].filter(Boolean).length;
      setAiConfigStatus(
        remoteCount > 0
          ? `Configuracao carregada de ${aiConfigStorageLabel(
              config.credential_storage_mode,
            )}; ${remoteCount.toLocaleString('pt-BR')} referencia(s) remota(s) no Cloudflare`
          : `Configuracao carregada de ${aiConfigStorageLabel(config.credential_storage_mode)}`,
      );
      void logEvent({
        level: 'info',
        category: 'settings.ai_provider.config_loaded',
        message: 'AI provider configuration loaded',
        context: {
          provider_mode: config.provider_mode,
          credential_storage_mode: config.credential_storage_mode,
          openai_key_present: Boolean(config.openai_api_key),
          anthropic_key_present: Boolean(config.anthropic_api_key),
          gemini_key_present: Boolean(config.gemini_api_key),
          deepseek_key_present: Boolean(config.deepseek_api_key),
          openai_rate_input_configured: config.openai_input_usd_per_million != null,
          openai_rate_output_configured: config.openai_output_usd_per_million != null,
          anthropic_rate_input_configured: config.anthropic_input_usd_per_million != null,
          anthropic_rate_output_configured: config.anthropic_output_usd_per_million != null,
          gemini_rate_input_configured: config.gemini_input_usd_per_million != null,
          gemini_rate_output_configured: config.gemini_output_usd_per_million != null,
          deepseek_cost_input_configured: config.deepseek_input_usd_per_million != null,
          deepseek_cost_output_configured: config.deepseek_output_usd_per_million != null,
          openai_remote_present: config.openai_api_key_remote,
          anthropic_remote_present: config.anthropic_api_key_remote,
          gemini_remote_present: config.gemini_api_key_remote,
          deepseek_remote_present: config.deepseek_api_key_remote,
        },
      });
    } catch (error) {
      setAiConfigStatus('Falha ao carregar configuracao das APIs');
      void logEvent({
        level: 'error',
        category: 'settings.ai_provider.config_load_failed',
        message: 'failed to load AI provider configuration',
        context: { error },
      });
    }
  }

  async function saveAiProviderConfig(nextProviderMode = providerMode) {
    setIsSavingAiConfig(true);
    try {
      const saved = await invoke<AiProviderConfig>('write_ai_provider_config', {
        config: buildAiProviderConfig(nextProviderMode),
        cloudflare: credentialStorageMode === 'cloudflare' ? buildCloudflareProviderStorageRequest() : null,
      });
      setProviderMode(saved.provider_mode);
      setAiCredentials({
        openai: saved.openai_api_key ?? '',
        anthropic: saved.anthropic_api_key ?? '',
        gemini: saved.gemini_api_key ?? '',
        deepseek: saved.deepseek_api_key ?? '',
      });
      applyProviderRatesFromConfig(saved);
      const storageLabel = aiConfigStorageLabel(saved.credential_storage_mode);
      setAiConfigStatus(`Salvo em ${storageLabel} as ${formatBrazilDateTime(new Date(saved.updated_at))}`);
      appendActivity({
        level: 'detail',
        title: 'Configuracao salva',
        detail:
          saved.credential_storage_mode === 'cloudflare'
            ? 'As chaves informadas foram enviadas ao Cloudflare Secrets Store; o JSON local guarda apenas o marcador do modo remoto.'
            : 'As chaves de API foram salvas conforme o modo de persistencia selecionado.',
      });
      void logEvent({
        level: 'info',
        category: 'settings.ai_provider.config_saved',
        message: 'AI provider configuration saved',
        context: {
          provider_mode: saved.provider_mode,
          credential_storage_mode: saved.credential_storage_mode,
          openai_key_present: Boolean(saved.openai_api_key),
          anthropic_key_present: Boolean(saved.anthropic_api_key),
          gemini_key_present: Boolean(saved.gemini_api_key),
          deepseek_key_present: Boolean(saved.deepseek_api_key),
          openai_rate_input_configured: saved.openai_input_usd_per_million != null,
          openai_rate_output_configured: saved.openai_output_usd_per_million != null,
          anthropic_rate_input_configured: saved.anthropic_input_usd_per_million != null,
          anthropic_rate_output_configured: saved.anthropic_output_usd_per_million != null,
          gemini_rate_input_configured: saved.gemini_input_usd_per_million != null,
          gemini_rate_output_configured: saved.gemini_output_usd_per_million != null,
          deepseek_cost_input_configured: saved.deepseek_input_usd_per_million != null,
          deepseek_cost_output_configured: saved.deepseek_output_usd_per_million != null,
          openai_remote_present: saved.openai_api_key_remote,
          anthropic_remote_present: saved.anthropic_api_key_remote,
          gemini_remote_present: saved.gemini_api_key_remote,
          deepseek_remote_present: saved.deepseek_api_key_remote,
        },
      });
      return saved;
    } catch (error) {
      setAiConfigStatus(error instanceof Error ? error.message : 'Falha ao salvar configuracao das APIs');
      void logEvent({
        level: 'error',
        category: 'settings.ai_provider.config_save_failed',
        message: 'failed to save AI provider configuration',
        context: { error },
      });
      return null;
    } finally {
      setIsSavingAiConfig(false);
    }
  }

  function chooseVerbosity(nextVerbosity: VerbosityMode) {
    setVerbosity(nextVerbosity);
    void logEvent({
      level: 'info',
      category: 'ui.verbosity.changed',
      message: 'operator changed interface verbosity',
      context: { verbosity: nextVerbosity, session_name: sessionName },
    });
  }

  function chooseSection(nextSection: ActiveSection) {
    setActiveSection(nextSection);
    void logEvent({
      level: 'info',
      category: 'ui.navigation.changed',
      message: 'operator changed active Maestro section',
      context: { active_section: nextSection, session_name: sessionName },
    });
  }

  function chooseSettingsTab(nextTab: SettingsTab) {
    setActiveSettingsTab(nextTab);
    void logEvent({
      level: 'info',
      category: 'ui.settings.navigation.changed',
      message: 'operator changed active Maestro settings tab',
      context: { active_settings_tab: nextTab, session_name: sessionName },
    });
  }

  async function importProtocol(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (!file) return;
    const text = await file.text();
    const nextProtocol = {
      name: file.name,
      size: file.size,
      lines: text.split(/\r?\n/).length,
      hash: await sha256(text),
    };
    setProtocol(nextProtocol);
    setProtocolText(text);
    void logEvent({
      level: 'info',
      category: 'protocol.imported',
      message: 'operator imported editorial protocol',
      context: nextProtocol,
    });
    event.target.value = '';
  }

  function formatSessionActivity(session: ResumableSessionInfo) {
    if (!session.last_activity_unix) return 'sem data registrada';
    return formatBrazilDateTime(session.last_activity_unix * 1000);
  }

  function resumeProtocolOptions(useLoadedProtocol: boolean) {
    if (!useLoadedProtocol || !hasLoadedProtocolForResume) {
      return { nextRound: undefined };
    }

    return {
      protocolName: protocol.name,
      protocolText,
      protocolHash: protocol.hash,
      nextRound: undefined,
    };
  }

  async function requestResumeSession() {
    setIsResumeLoading(true);
    setOperation({
      title: 'Buscando sessoes',
      progress: 16,
      current: 'Verificando sessoes interrompidas na pasta de dados.',
      eta: 'aguarde',
      status: 'preparing',
    });

    try {
      const sessions = await invoke<ResumableSessionInfo[]>('list_resumable_sessions');
      setResumeCandidates(sessions);
      setUseLoadedProtocolForResume(hasLoadedProtocolForResume);

      void logEvent({
        level: 'info',
        category: 'session.resume.requested',
        message: 'operator requested resumable session list',
        context: {
          count: sessions.length,
          loaded_protocol_available: hasLoadedProtocolForResume,
          protocol_name: hasLoadedProtocolForResume ? protocol.name : null,
        },
      });

      if (sessions.length === 0) {
        setOperation({
          title: 'Nenhuma sessao para retomar',
          progress: 0,
          current: 'Nao encontrei sessoes interrompidas na pasta de dados.',
          eta: 'inicie uma nova sessao quando quiser',
          status: 'idle',
        });
        appendActivity({
          level: 'summary',
          title: 'Nada para retomar',
          detail: 'A pasta de sessoes nao possui trabalhos interrompidos disponiveis.',
        });
        return;
      }

      if (sessions.length === 1) {
        await startResumeSession(sessions[0], hasLoadedProtocolForResume);
        return;
      }

      setShowResumePicker(true);
      setOperation({
        title: 'Escolha a sessao',
        progress: 28,
        current: `${sessions.length.toLocaleString('pt-BR')} sessoes interrompidas encontradas.`,
        eta: 'selecione qual trabalho continuar',
        status: 'paused',
      });
    } catch (error) {
      setOperation({
        title: 'Retomada indisponivel',
        progress: 0,
        current: 'Nao foi possivel ler as sessoes salvas.',
        eta: 'consulte diagnostico',
        status: 'blocked',
      });
      void logEvent({
        level: 'error',
        category: 'session.resume.list_failed',
        message: 'failed to list resumable sessions',
        context: { error },
      });
    } finally {
      setIsResumeLoading(false);
    }
  }

  async function startResumeSession(session: ResumableSessionInfo, useLoadedProtocol: boolean) {
    setShowResumePicker(false);
    setSessionRunId(session.run_id);
    setSessionName(session.session_name);
    const protocolOverride = resumeProtocolOptions(useLoadedProtocol);

    // B17 fix (v0.3.18): pre-populate React state for ACTIVE_AGENTS and
    // INITIAL_AGENT from the saved session contract before building
    // resumeRunOptions, so that clicking "Retomar" continues with the same
    // peers that were active when the session paused — not the cold-open
    // default of all 4.
    //
    // B20 fix (v0.3.32, operator-reported): time and cost caps are NOT
    // carried forward from the saved session. Each new session — including
    // a resume — must let the operator define new caps OR leave them
    // unlimited. The picker's saved_max_session_* fields stay in
    // ResumableSessionInfo for backward compat / inspection but are NOT
    // applied to React state and NOT injected into resumeRunOptions.
    const validSavedAgents = session.saved_active_agents.filter((agent) =>
      initialAgentOptions.some((option) => option.key === agent),
    ) as InitialAgentKey[];
    let resumeRunOptions: SessionRunOptions;
    if (validSavedAgents.length >= 1 && validSavedAgents.length <= 4) {
      setActiveAgents(validSavedAgents);
      const candidateInitial = (session.saved_initial_agent ?? '') as InitialAgentKey;
      const resolvedInitial: InitialAgentKey = validSavedAgents.includes(candidateInitial)
        ? candidateInitial
        : (validSavedAgents[0] as InitialAgentKey);
      setInitialAgent(resolvedInitial);
      // B20: read whatever cost/minutes the operator currently has in the UI
      // (may be empty = unlimited). Do NOT pre-populate from saved session.
      const currentRunOptions = currentSessionRunOptions();
      resumeRunOptions = {
        activeAgents: validSavedAgents,
        maxSessionCostUsd: currentRunOptions.maxSessionCostUsd,
        maxSessionMinutes: currentRunOptions.maxSessionMinutes,
        attachments: promptAttachments,
        links: parseSessionLinks(),
      };
      void logEvent({
        level: 'info',
        category: 'session.resume.contract_applied',
        message: 'resume populated React state from saved session contract (peers only; B20: caps come from current UI)',
        context: {
          run_id: session.run_id,
          saved_active_agents: validSavedAgents,
          saved_initial_agent: resolvedInitial,
          // B20: caps explicitly read from current UI state, not from saved
          // contract. The saved_max_session_* fields are inspection-only.
          requested_max_session_cost_usd: currentRunOptions.maxSessionCostUsd,
          requested_max_session_minutes: currentRunOptions.maxSessionMinutes,
        },
      });
    } else {
      try {
        resumeRunOptions = currentSessionRunOptions();
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        setOperation({
          title: 'Retomada bloqueada',
          progress: 0,
          current: message,
          eta: 'Ajuste a configuracao de peers e tente novamente.',
          status: 'blocked',
        });
        void logEvent({
          level: 'error',
          category: 'session.resume.run_options_invalid',
          message: 'resume aborted because UI peers/caps state is invalid',
          context: { run_id: session.run_id, error: message },
        });
        return;
      }
    }
    setOperation({
      title: 'Retomando sessao editorial',
      progress: 32,
      current: `Continuando a partir da rodada ${session.next_round.toLocaleString('pt-BR')}.`,
      eta: `Ultima atividade: ${formatSessionActivity(session)}`,
      status: 'preparing',
    });
    setPhaseItems([
      { label: 'Protocolo', detail: useLoadedProtocol && hasLoadedProtocolForResume ? 'atualizado' : 'salvo', state: 'done' },
      { label: 'Verificacoes', detail: 'concluidas', state: 'done' },
      { label: 'Agentes', detail: 'preparando continuidade', state: 'active' },
      { label: 'Entrega', detail: 'aguardando unanimidade', state: 'waiting' },
    ]);
    setDiscussionItems((current) => [
      {
        round: session.next_round.toString().padStart(3, '0'),
        status: 'Retomada',
        note:
          useLoadedProtocol && hasLoadedProtocolForResume
            ? `Sessao retomada com o protocolo carregado: ${protocol.name}.`
            : 'Sessao retomada com o protocolo salvo na pasta da sessao.',
      },
      ...current,
    ]);
    appendActivity({
      level: 'summary',
      title: 'Retomada iniciada',
      detail:
        useLoadedProtocol && hasLoadedProtocolForResume
          ? `Rodada ${session.next_round.toLocaleString('pt-BR')} com protocolo atualizado.`
          : `Rodada ${session.next_round.toLocaleString('pt-BR')} com protocolo salvo.`,
    });
    void logEvent({
      level: 'info',
      category: 'session.resume.selected',
      message: 'operator selected session to resume',
      context: {
        run_id: session.run_id,
        session_name: session.session_name,
        next_round: session.next_round,
        use_loaded_protocol: useLoadedProtocol && hasLoadedProtocolForResume,
        loaded_protocol_name: hasLoadedProtocolForResume ? protocol.name : null,
      },
    });

    const resumeInitialAgent: InitialAgentKey = resumeRunOptions.activeAgents.includes(initialAgent)
      ? initialAgent
      : (resumeRunOptions.activeAgents[0] ?? initialAgent);

    await runRealEditorialSession(
      session.run_id,
      '',
      {
        ...protocolOverride,
        nextRound: session.next_round,
      },
      resumeInitialAgent,
      resumeRunOptions,
    );
  }

  function toggleActiveAgent(agent: InitialAgentKey) {
    setActiveAgents((current) => {
      if (current.includes(agent)) {
        if (current.length === 1) return current;
        return current.filter((item) => item !== agent);
      }
      return [...current, agent].filter((item, index, items) => items.indexOf(item) === index).slice(0, 4);
    });
  }

  function parseOptionalPositiveNumber(value: string, label: string, maxValue?: number) {
    const trimmed = value.trim().replace(',', '.');
    if (!trimmed) return null;
    const parsed = Number(trimmed);
    if (!Number.isFinite(parsed) || parsed <= 0) {
      throw new Error(`${label} precisa ser um numero positivo ou ficar em branco.`);
    }
    if (maxValue != null && parsed > maxValue) {
      throw new Error(`${label} precisa ser menor ou igual a ${maxValue.toLocaleString('pt-BR')}.`);
    }
    return parsed;
  }

  function parseOptionalPositiveInteger(value: string, label: string) {
    const parsed = parseOptionalPositiveNumber(value, label);
    if (parsed == null) return null;
    if (!Number.isInteger(parsed)) {
      throw new Error(`${label} precisa ser um numero inteiro de minutos ou ficar em branco.`);
    }
    return parsed;
  }

  function parseSessionLinks() {
    return sessionLinks
      .split(/\r?\n|,/)
      .map((link) => link.trim())
      .filter(Boolean);
  }

  function currentSessionRunOptions(): SessionRunOptions {
    if (activeAgents.length < 1 || activeAgents.length > 4) {
      throw new Error('Selecione de 1 a 4 peers para a sessao.');
    }
    if (!activeAgents.includes(initialAgent)) {
      throw new Error('O agente da primeira versao precisa estar entre os peers ativos.');
    }
    const missingRateLabels = activeAgents
      .filter((agent) => agentUsesApi(agent))
      .filter((agent) => {
        const provider = providerForAgent[agent];
        parseOptionalPositiveNumber(providerInputUsdPerMillion[provider], `Tarifa ${provider} de entrada`, 10000);
        parseOptionalPositiveNumber(providerOutputUsdPerMillion[provider], `Tarifa ${provider} de saida`, 10000);
        return !providerRatesConfigured(provider);
      })
      .map((agent) => initialAgentOptions.find((option) => option.key === agent)?.label ?? agent);
    if (missingRateLabels.length > 0) {
      throw new Error(
        `Configure as tarifas de entrada e saida em Configuracoes > Agentes via API > Tabela de tarifas para: ${missingRateLabels.join(', ')}.`,
      );
    }
    return {
      activeAgents,
      maxSessionCostUsd: parseOptionalPositiveNumber(maxSessionCostUsd, 'Limite de custo'),
      maxSessionMinutes: parseOptionalPositiveInteger(maxSessionMinutes, 'Limite de tempo'),
      attachments: promptAttachments,
      links: parseSessionLinks(),
    };
  }

  async function handlePromptAttachments(event: ChangeEvent<HTMLInputElement>) {
    const files = Array.from(event.target.files ?? []);
    event.target.value = '';
    if (files.length === 0) return;
    const nextTotal = attachmentTotalBytes + files.reduce((total, file) => total + file.size, 0);
    if (promptAttachments.length + files.length > attachmentLimits.maxFiles) {
      appendActivity({
        level: 'summary',
        title: 'Anexos recusados',
        detail: `Limite de ${attachmentLimits.maxFiles} arquivos por sessao.`,
      });
      return;
    }
    if (files.some((file) => file.size > attachmentLimits.maxFileBytes) || nextTotal > attachmentLimits.maxTotalBytes) {
      appendActivity({
        level: 'summary',
        title: 'Anexos recusados',
        detail: 'Use arquivos de ate 25 MiB cada e ate 75 MiB no total.',
      });
      return;
    }
    const payloads = await Promise.all(files.map(fileToAttachmentPayload));
    setPromptAttachments((current) => [...current, ...payloads]);
  }

  async function fileToAttachmentPayload(file: File): Promise<PromptAttachmentPayload> {
    const bytes = await file.arrayBuffer();
    const view = new Uint8Array(bytes);
    let binary = '';
    const chunkSize = 0x8000;
    for (let index = 0; index < view.length; index += chunkSize) {
      binary += String.fromCharCode(...view.subarray(index, index + chunkSize));
    }
    return {
      name: file.name,
      media_type: file.type || null,
      size_bytes: file.size,
      data_base64: btoa(binary),
    };
  }

  function removePromptAttachment(name: string, sizeBytes: number) {
    setPromptAttachments((current) =>
      current.filter((item) => !(item.name === name && item.size_bytes === sizeBytes)),
    );
  }

  function startEditorialSession() {
    const promptText = editorialPrompt.trim();
    const runId = createRunId();

    if (!promptText) {
      setOperation({
        title: 'Prompt ausente',
        progress: 0,
        current: 'Escreva uma solicitacao antes de iniciar a sessao.',
        eta: 'aguardando entrada',
        status: 'blocked',
      });
      appendActivity({
        level: 'summary',
        title: 'Prompt vazio bloqueado',
        detail: 'Nenhum agente sera acionado sem uma solicitacao editorial concreta.',
      });
      void logEvent({
        level: 'warn',
        category: 'session.prompt.rejected',
        message: 'operator tried to start an editorial session without a prompt',
        context: { session_name: sessionName },
      });
      return;
    }

    if (protocolText.trim().length < 100) {
      setOperation({
        title: 'Protocolo integral ausente',
        progress: 0,
        current: 'Importe o arquivo Markdown integral do protocolo antes de iniciar a sessao.',
        eta: 'aguardando protocolo',
        status: 'blocked',
      });
      appendActivity({
        level: 'summary',
        title: 'Protocolo ausente',
        detail: 'A sessao foi bloqueada porque o texto integral do protocolo ainda nao foi carregado ou e curto demais.',
      });
      void logEvent({
        level: 'warn',
        category: 'session.protocol.rejected',
        message: 'operator tried to start an editorial session without full protocol text loaded',
        context: {
          session_name: sessionName,
          protocol_name: protocol.name,
          protocol_lines: protocol.lines,
          protocol_hash: protocol.hash,
        },
      });
      return;
    }

    let runOptions: SessionRunOptions;
    try {
      runOptions = currentSessionRunOptions();
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Controles da sessao invalidos.';
      setOperation({
        title: 'Controles invalidos',
        progress: 0,
        current: message,
        eta: 'ajuste peers, custo ou tempo',
        status: 'blocked',
      });
      appendActivity({
        level: 'summary',
        title: 'Sessao bloqueada',
        detail: message,
      });
      void logEvent({
        level: 'warn',
        category: 'session.controls.rejected',
        message: 'operator tried to start an editorial session with invalid controls',
        context: { error: message },
      });
      return;
    }

    setSessionRunId(runId);
    const selectedInitialAgent = initialAgent;
    const selectedInitialAgentLabel =
      initialAgentOptions.find((option) => option.key === selectedInitialAgent)?.label ?? 'Claude';
    setOperation({
      title: 'Preparando sessao editorial',
      progress: 8,
      current: 'Prompt recebido; fixando protocolo e abrindo ata operacional.',
      eta: runId,
      status: 'preparing',
    });
    setPhaseItems([
      { label: 'Protocolo', detail: 'registrando', state: 'active' },
      { label: 'Verificacoes', detail: 'aguardando protocolo', state: 'waiting' },
      { label: 'Agentes', detail: 'nao iniciados', state: 'waiting' },
      { label: 'Entrega', detail: 'bloqueada ate unanimidade', state: 'waiting' },
    ]);
    setAgentCards(initialAgents.map((agent) => ({ ...agent, note: 'aguardando verificacoes iniciais' })));
    setEvidenceRows(initialEvidenceRows.map((item) => ({ ...item, value: 'aguardando verificacoes' })));
    setProtocolGateItems(initialProtocolReadingGates);
    setDiscussionItems([
      {
        round: '000',
        status: 'Sessao criada',
        note: `Prompt recebido. ${selectedInitialAgentLabel} abrira a primeira versao; peers ativos: ${activeAgentLabels}.`,
      },
    ]);
    setActivityItems([
      {
        level: 'summary',
        time: activityTimestamp(),
        title: 'Prompt recebido',
        detail: 'Sessao criada. A partir daqui, cada etapa aparecera no acompanhamento e no diagnostico.',
      },
      ...idleActivityFeed,
    ]);
    void logEvent({
      level: 'info',
      category: 'session.prompt.submitted',
      message: 'operator submitted editorial generation prompt',
      context: {
        run_id: runId,
        session_name: sessionName,
        prompt_chars: editorialPrompt.length,
        protocol_name: protocol.name,
        protocol_lines: protocol.lines,
        protocol_chars: protocolText.length,
        required_outputs: finalArtifacts.map((artifact) => artifact.name),
        consensus_gate: 'selected_editorial_agents_ready_same_round',
        initial_agent: selectedInitialAgent,
        active_agents: runOptions.activeAgents,
        max_session_cost_usd: runOptions.maxSessionCostUsd,
        max_session_minutes: runOptions.maxSessionMinutes,
        attachment_count: runOptions.attachments.length,
        link_count: runOptions.links.length,
      },
    });
    void logEvent({
      level: 'info',
      category: 'session.orchestration.started',
      message: 'visible editorial session monitor started',
      context: {
        run_id: runId,
        provider_mode: providerMode,
        credential_storage_mode: credentialStorageMode,
        initial_agent: selectedInitialAgent,
        active_agents: runOptions.activeAgents,
      },
    });

    setOperation({
      title: 'Protocolo fixado',
      progress: 22,
      current: `Protocolo ativo registrado com ${protocol.lines.toLocaleString('pt-BR')} linhas.`,
      eta: runId,
      status: 'preparing',
    });
    setPhaseItems([
      { label: 'Protocolo', detail: 'registrado', state: 'done' },
      { label: 'Verificacoes', detail: 'concluidas', state: 'done' },
      { label: 'Agentes', detail: 'iniciando', state: 'active' },
      { label: 'Entrega', detail: 'bloqueada ate unanimidade', state: 'waiting' },
    ]);
    setEvidenceRows([
      { label: 'DOI', value: 'Aguardando', tone: 'info' },
      { label: 'Links', value: 'Aguardando', tone: 'info' },
      { label: 'ABNT', value: 'Aguardando', tone: 'info' },
      { label: 'Quarentena', value: 'Aguardando', tone: 'info' },
    ]);
    appendActivity({
      level: 'detail',
      title: 'Protocolo registrado',
      detail: `Arquivo ${protocol.name}; ${protocol.lines.toLocaleString('pt-BR')} linhas registradas.`,
    });
    void logEvent({
      level: 'info',
      category: 'session.protocol.pinned',
      message: 'editorial protocol pinned for current visible session',
      context: { run_id: runId, protocol_name: protocol.name, protocol_lines: protocol.lines, protocol_hash: protocol.hash },
    });
    void logEvent({
      level: 'info',
      category: 'session.preflight.completed',
      message: 'local visible preflight completed',
      context: { run_id: runId },
    });
    void runRealEditorialSession(runId, promptText, undefined, selectedInitialAgent, runOptions);
  }

  async function runRealEditorialSession(
    runId: string,
    promptText: string,
    resumeOptions?: {
      protocolName?: string;
      protocolText?: string;
      protocolHash?: string;
      nextRound?: number;
    },
    selectedInitialAgent: InitialAgentKey = initialAgent,
    runOptions?: SessionRunOptions,
  ) {
    const isResume = Boolean(resumeOptions);
    const startedAt = Date.now();
    const startedAtLabel = formatBrazilDateTime(startedAt);
    const selectedInitialAgentLabel =
      initialAgentOptions.find((option) => option.key === selectedInitialAgent)?.label ?? 'Claude';
    setOperation({
      title: isResume ? 'Retomando sessao editorial' : 'Sessao editorial em andamento',
      progress: 44,
      current: isResume
        ? `Continuando a partir da rodada ${resumeOptions?.nextRound?.toLocaleString('pt-BR') ?? 'salva'}.`
        : `${selectedInitialAgentLabel} esta preparando a primeira versao; peers ativos: ${
            runOptions?.activeAgents
              .map((agent) => initialAgentOptions.find((option) => option.key === agent)?.label ?? agent)
              .join(', ') ?? activeAgentLabels
          }.`,
      eta: `Inicio: ${startedAtLabel}`,
      status: 'running',
    });
    setIsStopRequested(false);
    setPhaseItems([
      { label: 'Protocolo', detail: 'registrado', state: 'done' },
      { label: 'Verificacoes', detail: 'concluidas', state: 'done' },
      { label: 'Agentes', detail: 'em execucao', state: 'active' },
      { label: 'Entrega', detail: 'aguardando unanimidade', state: 'waiting' },
    ]);
    setAgentCards([
      ...initialAgentOptions.map((option) => ({
        name: option.label,
        cli: option.key,
        state: runOptions && !runOptions.activeAgents.includes(option.key) ? ('blocked' as AgentState) : ('running' as AgentState),
        note:
          runOptions && !runOptions.activeAgents.includes(option.key)
            ? 'fora desta sessao'
            : option.key === selectedInitialAgent
            ? 'primeira versao e ajustes em andamento'
            : 'leitura e revisao em andamento',
      })),
      { name: 'Maestro', cli: 'motor local', state: 'running', note: 'acompanhando a unanimidade' },
    ]);
    appendActivity({
      level: 'diagnostic',
      title: 'Sessao iniciada',
      detail: 'O Maestro esta acompanhando os agentes e registrando os arquivos da rodada.',
    });
    void logEvent({
      level: 'info',
      category: 'session.editorial.requested',
      message: 'frontend requested real editorial session',
      context: {
        run_id: runId,
        session_name: sessionName,
        prompt_chars: promptText.length,
        resume_mode: isResume,
        resume_next_round: resumeOptions?.nextRound ?? null,
        resume_protocol_override: Boolean(resumeOptions?.protocolText),
        protocol_name: protocol.name,
        protocol_lines: protocol.lines,
        protocol_chars: protocolText.length,
        protocol_hash: protocol.hash,
        provider_mode: providerMode,
        credential_storage_mode: credentialStorageMode,
        initial_agent: selectedInitialAgent,
        active_agents: runOptions?.activeAgents ?? null,
        max_session_cost_usd: runOptions?.maxSessionCostUsd ?? null,
        max_session_minutes: runOptions?.maxSessionMinutes ?? null,
        attachment_count: runOptions?.attachments.length ?? 0,
        link_count: runOptions?.links.length ?? 0,
      },
    });

    let lastLoggedMinute = 0;
    const heartbeat = window.setInterval(() => {
      const elapsedSeconds = Math.max(1, Math.floor((Date.now() - startedAt) / 1000));
      const elapsedMinutes = Math.floor(elapsedSeconds / 60);
      setOperation({
        title: isResume ? 'Retomando sessao editorial' : 'Sessao editorial em andamento',
        progress: 44,
        current: `Trabalho em andamento ha ${formatElapsedTime(elapsedSeconds)}.`,
        eta: `Inicio: ${startedAtLabel}`,
        status: 'running',
      });
      if (elapsedMinutes > lastLoggedMinute) {
        lastLoggedMinute = elapsedMinutes;
        if (elapsedMinutes % 5 === 0) {
          appendActivity({
            level: 'detail',
            title: 'Sessao em andamento',
            detail: `Tempo decorrido: ${formatElapsedTime(elapsedSeconds)}. Rodadas continuam ate a aprovacao final.`,
          });
        }
        void logEvent({
          level: 'info',
          category: 'session.editorial.heartbeat',
          message: 'editorial session heartbeat',
          context: { run_id: runId, elapsed_seconds: elapsedSeconds },
        });
      }
    }, 5000);

    try {
      const result = resumeOptions
        ? await invoke<EditorialSessionResult>('resume_editorial_session', {
            request: {
              run_id: runId,
              protocol_name: resumeOptions.protocolName ?? null,
              protocol_text: resumeOptions.protocolText ?? null,
              protocol_hash: resumeOptions.protocolHash ?? null,
              initial_agent: selectedInitialAgent,
              active_agents: runOptions?.activeAgents ?? null,
              max_session_cost_usd: runOptions?.maxSessionCostUsd ?? null,
              max_session_minutes: runOptions?.maxSessionMinutes ?? null,
              attachments: runOptions?.attachments ?? [],
              links: runOptions?.links ?? null,
            },
          })
        : await invoke<EditorialSessionResult>('run_editorial_session', {
            request: {
              run_id: runId,
              session_name: sessionName,
              prompt: promptText,
              protocol_name: protocol.name,
              protocol_text: protocolText,
              protocol_hash: protocol.hash,
              initial_agent: selectedInitialAgent,
              active_agents: runOptions?.activeAgents ?? null,
              max_session_cost_usd: runOptions?.maxSessionCostUsd ?? null,
              max_session_minutes: runOptions?.maxSessionMinutes ?? null,
              attachments: runOptions?.attachments ?? [],
              links: runOptions?.links ?? [],
            },
          });
      window.clearInterval(heartbeat);
      setLastSessionMinutesPath(result.session_minutes_path);
      const nextAgentCards = latestAgentCards(result.agents);
      setAgentCards([
        ...nextAgentCards,
        {
          name: 'Maestro',
          cli: 'motor local',
          state: result.consensus_ready ? 'ready' : 'evidence',
          note: result.consensus_ready ? 'unanimidade registrada' : 'aguardando continuidade da sessao',
        },
      ]);
      setProtocolGateItems(latestProtocolGateItems(result.agents));
      setEvidenceRows([
        { label: 'DOI', value: 'revisado pelos agentes', tone: result.consensus_ready ? 'ok' : 'warn' },
        { label: 'Links', value: 'exige motor mecanico dedicado', tone: 'warn' },
        { label: 'ABNT', value: 'revisado pelos agentes', tone: result.consensus_ready ? 'ok' : 'warn' },
        {
          label: 'Quarentena',
          value: result.consensus_ready ? 'liberado por unanimidade' : 'texto bloqueado',
          tone: result.consensus_ready ? 'ok' : 'danger',
        },
      ]);
      setDiscussionItems((current) => [
        {
          round: '001',
          status: humanizeAgentStatus(result.status),
          note: result.consensus_ready
            ? `Texto final liberado em ${result.final_markdown_path}; ata em ${result.session_minutes_path}.`
            : `Sem unanimidade. Ata em ${result.session_minutes_path}; artefatos dos agentes em ${result.session_dir}.`,
        },
        ...current,
      ]);
      appendActivity({
        level: result.consensus_ready ? 'summary' : 'detail',
        title: result.consensus_ready ? 'Texto final liberado' : 'Sessao pausada',
        detail: `${summarizeAgentResults(result.agents)} Custo observado: ${
          result.observed_cost_usd == null ? 'nao medido' : `US$ ${result.observed_cost_usd.toFixed(6)}`
        }. Log humano: ${result.human_log_path ?? 'indisponivel'}.`,
      });

      if (result.consensus_ready) {
        setOperation({
          title: 'Texto final liberado',
          progress: 100,
          current: `Unanimidade dos agentes registrada. Texto: ${result.final_markdown_path}`,
          eta: `Ata: ${result.session_minutes_path}`,
          status: 'completed',
        });
        setPhaseItems([
          { label: 'Protocolo', detail: 'registrado', state: 'done' },
          { label: 'Verificacoes', detail: 'concluidas', state: 'done' },
          { label: 'Agentes', detail: 'concluidos', state: 'done' },
          { label: 'Entrega', detail: 'unanimidade registrada', state: 'done' },
        ]);
        void logEvent({
          level: 'info',
          category: 'session.editorial.final_available',
          message: 'final editorial markdown available after real unanimous session',
          context: {
            run_id: runId,
            final_markdown_path: result.final_markdown_path,
            session_minutes_path: result.session_minutes_path,
            active_agents: result.active_agents,
            observed_cost_usd: result.observed_cost_usd,
            human_log_path: result.human_log_path,
            agents: result.agents.map((agent) => ({ name: agent.name, tone: agent.tone })),
          },
        });
      } else {
        setOperation({
          title: 'Sessao pausada sem entrega final',
          progress: 66,
          current:
            result.status === 'PAUSED_DRAFT_UNAVAILABLE'
              ? 'Nenhum agente produziu rascunho utilizavel. A entrega segue indisponivel ate nova tentativa ou intervencao.'
              : result.status === 'TIME_LIMIT_REACHED'
              ? 'O limite de tempo opcional foi atingido. A entrega segue indisponivel ate nova sessao ou retomada ajustada.'
              : result.status === 'COST_LIMIT_REACHED'
              ? 'O limite de custo opcional foi atingido antes de nova chamada paga. A entrega segue indisponivel.'
              : result.status === 'PAUSED_COST_RATES_MISSING'
              ? 'Um peer via API esta selecionado, mas suas tarifas de entrada e saida ainda nao foram configuradas em Configuracoes > Agentes via API.'
              : result.status === 'ALL_PEERS_FAILING'
              ? 'Todos os peers ativos retornaram erro em 3 rodadas consecutivas. Sessao pausada para nao queimar quota e tempo. Verifique conectividade, chaves de API e quotas; depois retome.'
              : 'A sessao nao entregou texto final nesta chamada. Divergencias exigem novas rodadas ate unanimidade.',
          eta: `Ata: ${result.session_minutes_path}`,
          status: 'paused',
        });
        setPhaseItems([
          { label: 'Protocolo', detail: 'registrado', state: 'done' },
          { label: 'Verificacoes', detail: 'concluidas', state: 'done' },
          { label: 'Agentes', detail: 'rodadas registradas', state: 'done' },
          { label: 'Entrega', detail: 'aguardando unanimidade', state: 'waiting' },
        ]);
        void logEvent({
          level: 'warn',
          category: 'session.editorial.blocked',
          message: 'real editorial session completed without unanimous approval',
          context: {
            run_id: runId,
            status: result.status,
            session_minutes_path: result.session_minutes_path,
            session_dir: result.session_dir,
            active_agents: result.active_agents,
            observed_cost_usd: result.observed_cost_usd,
            max_session_cost_usd: result.max_session_cost_usd,
            max_session_minutes: result.max_session_minutes,
            human_log_path: result.human_log_path,
            agents: result.agents.map((agent) => ({
              name: agent.name,
              role: agent.role,
              tone: agent.tone,
              status: agent.status,
              exit_code: agent.exit_code,
              output_path: agent.output_path,
            })),
            final_delivery: 'blocked_without_all_agent_unanimity',
          },
        });
      }
    } catch (error) {
      window.clearInterval(heartbeat);
      setOperation({
        title: 'Sessao editorial falhou',
        progress: 42,
        current: 'O Maestro nao conseguiu concluir a sessao editorial.',
        eta: 'consulte diagnostico',
        status: 'blocked',
      });
      setAgentCards([
        { name: 'Claude', cli: 'claude', state: 'blocked', note: 'falha antes de resultado estruturado' },
        { name: 'Codex', cli: 'codex', state: 'blocked', note: 'falha antes de resultado estruturado' },
        { name: 'Gemini', cli: 'gemini', state: 'blocked', note: 'falha antes de resultado estruturado' },
        { name: 'DeepSeek', cli: 'deepseek-api', state: 'blocked', note: 'falha antes de resultado estruturado' },
        { name: 'Maestro', cli: 'motor local', state: 'blocked', note: 'consulte diagnostico e arquivos da sessao' },
      ]);
      void logEvent({
        level: 'error',
        category: 'session.editorial.invoke_failed',
        message: 'native real editorial session invoke failed',
        context: { run_id: runId, error },
      });
    } finally {
      // Reset stop-button state regardless of how the session ended (success,
      // failure, or operator-stop). Backend STOPPED_BY_USER status arrives
      // through the same try/await branch as success/error.
      setIsStopRequested(false);
    }
  }

  // Operator-driven stop: confirm + invoke `stop_editorial_session`. The
  // backend signals the cancellation token; the in-flight CLI peer is killed
  // by `kill_process_tree` (cancel granularity 250ms via the
  // `run_resolved_command_observed` poll loop) and the in-flight API peer
  // future is dropped via `tokio::select!` in `send_with_retry_async`
  // (cancel <2s). The session loop exits with `STOPPED_BY_USER` status; the
  // existing run-completion branch handles UI cleanup.
  async function handleStopSession() {
    if (!sessionRunId) return;
    if (isStopRequested) return;
    const confirmed = window.confirm(
      'Parar a sessao atual? Drafts em andamento ficam preservados como artifacts mas sem convergencia.\n\nVoce pode retomar a sessao depois pelo botao "Continuar".',
    );
    if (!confirmed) return;
    setIsStopRequested(true);
    try {
      await invoke<boolean>('stop_editorial_session', { runId: sessionRunId });
      void logEvent({
        level: 'info',
        category: 'session.user.stop_requested',
        message: 'operator clicked stop session',
        context: { run_id: sessionRunId },
      });
    } catch (error) {
      // Reset on failed invoke so operator can retry.
      setIsStopRequested(false);
      void logEvent({
        level: 'error',
        category: 'session.user.stop_failed',
        message: 'stop_editorial_session invoke failed',
        context: { run_id: sessionRunId, error: String(error) },
      });
    }
  }

  function updateAiCredential(provider: AiCredentialKey, value: string) {
    setAiCredentials((current) => ({ ...current, [provider]: value }));
  }

  function updateProviderInputRate(provider: ProviderRateKey, value: string) {
    setProviderInputUsdPerMillion((current) => ({ ...current, [provider]: value }));
  }

  function updateProviderOutputRate(provider: ProviderRateKey, value: string) {
    setProviderOutputUsdPerMillion((current) => ({ ...current, [provider]: value }));
  }

  function applyProviderRatesFromConfig(config: AiProviderConfig) {
    setProviderInputUsdPerMillion({
      openai: config.openai_input_usd_per_million == null ? '' : String(config.openai_input_usd_per_million),
      anthropic:
        config.anthropic_input_usd_per_million == null ? '' : String(config.anthropic_input_usd_per_million),
      gemini: config.gemini_input_usd_per_million == null ? '' : String(config.gemini_input_usd_per_million),
      deepseek: config.deepseek_input_usd_per_million == null ? '' : String(config.deepseek_input_usd_per_million),
    });
    setProviderOutputUsdPerMillion({
      openai: config.openai_output_usd_per_million == null ? '' : String(config.openai_output_usd_per_million),
      anthropic:
        config.anthropic_output_usd_per_million == null ? '' : String(config.anthropic_output_usd_per_million),
      gemini: config.gemini_output_usd_per_million == null ? '' : String(config.gemini_output_usd_per_million),
      deepseek: config.deepseek_output_usd_per_million == null ? '' : String(config.deepseek_output_usd_per_million),
    });
  }

  function chooseProviderMode(nextMode: ProviderMode) {
    setProviderMode(nextMode);
    if (nextMode === 'cli') {
      // CLI mode is incompatible with DeepSeek (no CLI integration available).
      // Drop DeepSeek from the peer set and reassign the initial agent so the
      // operator can never enter a state where the run silently falls back to API.
      setActiveAgents((current) => {
        const filtered = current.filter((agent) => agent !== 'deepseek');
        return filtered.length === 0 ? ['claude'] : filtered;
      });
      setInitialAgent((current) => (current === 'deepseek' ? 'claude' : current));
    }
    void saveAiProviderConfig(nextMode);
    void logEvent({
      level: 'info',
      category: 'settings.provider_mode.changed',
      message: 'operator changed AI provider orchestration mode',
      context: { provider_mode: nextMode },
    });
  }

  function chooseCredentialStorage(nextMode: CredentialStorageMode) {
    setCredentialStorageMode(nextMode);
    void persistBootstrapConfig(nextMode);
    void logEvent({
      level: 'info',
      category: 'settings.credential_storage.changed',
      message: 'operator changed credential storage mode',
      context: { credential_storage_mode: nextMode },
    });
  }

  async function verifyCloudflareCredentials() {
    setIsVerifyingCloudflare(true);
    await persistBootstrapConfig();
    const accountId = cloudflareAccountId.trim() || cloudflareEnvSnapshot?.account_id || '';
    const tokenEnvVar = cloudflareTokenEnvVar.trim() || cloudflareEnvSnapshot?.api_token_env_var || 'MAESTRO_CLOUDFLARE_API_TOKEN';
    setCloudflarePermissionRows([
      {
        label: 'Token ativo',
        value: cloudflareTokenAvailable ? `verificando via ${tokenEnvVar}` : 'ausente; informe token ou env var',
        tone: cloudflareTokenAvailable ? 'pending' : 'blocked',
      },
      {
        label: 'Conta acessivel',
        value: accountId ? 'aguardando resposta da API Cloudflare' : 'account id ausente',
        tone: accountId ? 'pending' : 'blocked',
      },
      { label: 'D1 Read/Edit', value: 'aguardando resposta D1', tone: 'pending' },
      { label: 'Secrets Store', value: 'aguardando resposta do Secrets Store', tone: 'pending' },
    ]);
    void logEvent({
      level: 'info',
      category: 'settings.cloudflare.verify_requested',
      message: 'operator requested Cloudflare credential validation',
      context: {
        account_id_present: accountId.length > 0,
        token_present: cloudflareTokenAvailable,
        token_source: cloudflareEnvSnapshot?.api_token_present ? 'windows_env' : cloudflareTokenSource,
        token_env_var: tokenEnvVar,
        target_database: 'bigdata_db',
        target_table: 'mainsite_posts',
        persistence_database: 'maestro_db',
        persistence_secret_store: 'maestro',
        credential_storage_mode: credentialStorageMode,
      },
    });

    try {
      const result = await invoke<CloudflareProbeResult>('verify_cloudflare_credentials', {
        request: {
          account_id: accountId,
          api_token: cloudflareApiToken.trim() || null,
          api_token_env_var: tokenEnvVar,
          persistence_database: 'maestro_db',
          publication_database: 'bigdata_db',
          secret_store: 'maestro',
        },
      });
      setCloudflarePermissionRows(result.rows);
      appendActivity({
        level: 'diagnostic',
        title: 'Cloudflare verificado',
        detail: result.rows.map((row) => `${row.label}: ${row.tone}`).join('; '),
      });
      void logEvent({
        level: result.rows.some((row) => row.tone === 'error' || row.tone === 'blocked') ? 'warn' : 'info',
        category: 'settings.cloudflare.verify_rendered',
        message: 'Cloudflare credential validation rendered in UI',
        context: {
          rows: result.rows.map((row) => ({ label: row.label, tone: row.tone })),
        },
      });
    } catch (error) {
      setCloudflarePermissionRows([
        { label: 'Token ativo', value: 'falha na verificacao local', tone: 'error' },
        { label: 'Conta acessivel', value: 'nao executado', tone: 'blocked' },
        { label: 'D1 Read/Edit', value: 'nao executado', tone: 'blocked' },
        { label: 'Secrets Store', value: 'nao executado', tone: 'blocked' },
      ]);
      void logEvent({
        level: 'error',
        category: 'settings.cloudflare.verify_failed',
        message: 'Cloudflare credential validation failed before receiving API result',
        context: { error },
      });
    } finally {
      setIsVerifyingCloudflare(false);
    }
  }

  async function verifyAiProviderCredentials() {
    setIsVerifyingAiProviders(true);
    setAiProviderRowsState(
      aiProviderRows.map((provider) => ({
        label: provider.name,
        value: aiCredentials[provider.key].trim() ? 'verificando credencial' : 'API key nao informada',
        tone: aiCredentials[provider.key].trim() ? 'pending' : 'warn',
      })),
    );
    void logEvent({
      level: 'info',
      category: 'settings.ai_provider.verify_requested',
      message: 'operator requested AI provider credential validation',
      context: {
        provider_mode: providerMode,
        credential_storage_mode: credentialStorageMode,
        openai_key_present: aiCredentials.openai.length > 0,
        anthropic_key_present: aiCredentials.anthropic.length > 0,
        gemini_key_present: aiCredentials.gemini.length > 0,
        deepseek_key_present: aiCredentials.deepseek.length > 0,
      },
    });

    const saved = await saveAiProviderConfig();
    if (!saved) {
      setAiProviderRowsState([
        { label: 'OpenAI / Codex', value: 'verificacao nao executada: falha ao salvar', tone: 'error' },
        { label: 'Anthropic / Claude', value: 'verificacao nao executada: falha ao salvar', tone: 'error' },
        { label: 'Google / Gemini', value: 'verificacao nao executada: falha ao salvar', tone: 'error' },
        { label: 'DeepSeek', value: 'verificacao nao executada: falha ao salvar', tone: 'error' },
      ]);
      setIsVerifyingAiProviders(false);
      return;
    }

    try {
      const result = await invoke<AiProviderProbeResult>('verify_ai_provider_credentials', {
        config: saved,
      });
      setAiProviderRowsState(result.rows);
      setAiConfigStatus(`Verificado em ${formatBrazilDateTime(new Date(result.checked_at))}`);
      appendActivity({
        level: 'diagnostic',
        title: 'APIs verificadas',
        detail: result.rows.map((row) => `${row.label}: ${row.tone}`).join('; '),
      });
      void logEvent({
        level: result.rows.some((row) => row.tone === 'error' || row.tone === 'blocked') ? 'warn' : 'info',
        category: 'settings.ai_provider.verify_completed',
        message: 'AI provider credential validation completed',
        context: {
          rows: result.rows.map((row) => ({ label: row.label, tone: row.tone })),
        },
      });
    } catch (error) {
      setAiProviderRowsState([
        { label: 'OpenAI / Codex', value: 'falha local na verificacao', tone: 'error' },
        { label: 'Anthropic / Claude', value: 'falha local na verificacao', tone: 'error' },
        { label: 'Google / Gemini', value: 'falha local na verificacao', tone: 'error' },
        { label: 'DeepSeek', value: 'falha local na verificacao', tone: 'error' },
      ]);
      void logEvent({
        level: 'error',
        category: 'settings.ai_provider.verify_failed',
        message: 'AI provider credential validation failed before receiving API result',
        context: { error },
      });
    } finally {
      setIsVerifyingAiProviders(false);
    }
  }

  async function savePostEditorDraft(
    title: string,
    author: string,
    htmlContent: string,
    isPublished: boolean,
    isAboutSite: boolean,
    confirmedAboutAction?: boolean,
    requestedPostId?: number,
  ) {
    setSessionName(title || sessionName);
    setMainSiteHtml(htmlContent);
    void logEvent({
      level: 'info',
      category: 'editor.posteditor.save',
      message: 'operator saved PostEditor-compatible draft',
      context: {
        title,
        author,
        chars: htmlContent.length,
        is_published: isPublished,
        is_about_site: isAboutSite,
        confirmed_about_action: confirmedAboutAction ?? false,
        requested_post_id: requestedPostId ?? null,
        compatibility_target: 'admin-app/MainSite/PostEditor',
      },
    });
    return true;
  }

  function openPostEditor() {
    setShowPostEditor(true);
    void logEvent({
      level: 'info',
      category: 'editor.posteditor.open',
      message: 'operator opened PostEditor-compatible editor panel',
    });
  }

  function closePostEditor() {
    setShowPostEditor(false);
    void logEvent({
      level: 'info',
      category: 'editor.posteditor.close',
      message: 'operator closed PostEditor-compatible editor panel',
    });
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">M</div>
          <div>
            <div className="brand-name">Maestro Editorial AI</div>
            <div className="brand-meta">{APP_VERSION}</div>
          </div>
        </div>

        <nav className="nav-list" aria-label="Principal">
          {navGroups.map((group) => (
            <div className="nav-group" key={group.label}>
              <div className="nav-group-label">{group.label}</div>
              {group.items.map((item) => {
                const Icon = item.icon;
                return (
                  <button
                    className={activeSection === item.section ? 'nav-item active' : 'nav-item'}
                    type="button"
                    key={item.section}
                    aria-current={activeSection === item.section ? 'page' : undefined}
                    onClick={() => chooseSection(item.section)}
                  >
                    <Icon size={18} />
                    {item.label}
                  </button>
                );
              })}
            </div>
          ))}
        </nav>

        <div className="storage-strip">
          <Database size={18} />
          <div>
            <strong>{storageModeSummaries[credentialStorageMode].title}</strong>
            <span>{storageModeSummaries[credentialStorageMode].detail}</span>
          </div>
        </div>
      </aside>

      <main className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">{activeNavItem.label}</p>
            <input
              className="session-title"
              value={sessionName}
              onChange={(event) => setSessionName(event.target.value)}
              aria-label="Nome da sessao"
            />
          </div>
          <div className="toolbar">
            <button
              className="icon-button"
              type="button"
              title="Revalidar"
              onClick={() => void revalidateRuntime()}
            >
              <RefreshCw size={18} />
            </button>
            <button
              className={isResumeLoading ? 'secondary-button busy' : 'secondary-button'}
              type="button"
              onClick={() => void requestResumeSession()}
              aria-busy={isResumeLoading}
              disabled={isRunPreparing || isResumeLoading}
            >
              <Clock3 size={18} />
              {isResumeLoading ? 'Buscando' : 'Retomar'}
            </button>
            <button
              className={isRunPreparing ? 'primary-button busy' : 'primary-button'}
              type="button"
              onClick={startEditorialSession}
              aria-busy={isRunPreparing}
              disabled={isRunPreparing}
            >
              <Play size={18} />
              {isRunPreparing ? 'Preparando' : runActionLabel}
            </button>
            {isRunPreparing && sessionRunId && (
              <button
                type="button"
                className="secondary-button"
                onClick={handleStopSession}
                disabled={isStopRequested}
                aria-busy={isStopRequested}
                title="Para a sessao em andamento (CLI peer cancela em ~250ms; API peer cancela em <2s)."
              >
                <Square size={18} />
                {isStopRequested ? 'Parando…' : 'Parar sessao'}
              </button>
            )}
          </div>
        </header>

        {showResumePicker && (
          <div className="modal-backdrop" role="presentation">
            <section className="resume-dialog" role="dialog" aria-modal="true" aria-label="Retomar sessao">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">Retomar</p>
                  <h2>Escolha uma sessao</h2>
                </div>
                <button className="icon-button" type="button" onClick={() => setShowResumePicker(false)} title="Fechar">
                  <EyeOff size={18} />
                </button>
              </div>

              <label className={hasLoadedProtocolForResume ? 'resume-protocol-option' : 'resume-protocol-option disabled'}>
                <input
                  type="checkbox"
                  checked={useLoadedProtocolForResume && hasLoadedProtocolForResume}
                  disabled={!hasLoadedProtocolForResume}
                  onChange={(event) => setUseLoadedProtocolForResume(event.target.checked)}
                />
                <span>
                  {hasLoadedProtocolForResume
                    ? `Usar protocolo carregado agora: ${protocol.name}`
                    : 'Usar o protocolo salvo dentro de cada sessao'}
                </span>
              </label>

              <div className="resume-list">
                {resumeCandidates.map((session) => (
                  <button
                    className="resume-session-row"
                    type="button"
                    key={session.run_id}
                    onClick={() => void startResumeSession(session, useLoadedProtocolForResume)}
                  >
                    <div>
                      <strong>{session.session_name}</strong>
                      <span>{session.run_id}</span>
                    </div>
                    <div>
                      <strong>Rodada {session.next_round.toLocaleString('pt-BR')}</strong>
                      <span>{formatSessionActivity(session)}</span>
                    </div>
                    <div>
                      <strong>{session.status}</strong>
                      <span>
                        {session.artifact_count.toLocaleString('pt-BR')} arquivos; {session.protocol_lines.toLocaleString('pt-BR')} linhas
                      </span>
                    </div>
                  </button>
                ))}
              </div>
            </section>
          </div>
        )}

        {activeSection === 'session' && (
          <>
            <section className="status-grid" aria-label="Resumo">
              <div className="metric-panel">
                <ShieldCheck size={20} />
                <div>
                  <span>Estado formal</span>
                  <strong>{formalState}</strong>
                </div>
              </div>
              <div className="metric-panel">
                <Bot size={20} />
                <div>
                  <span>Consenso</span>
                  <strong>
                    {readyCount}/{agentCards.length} aprovados
                  </strong>
                </div>
              </div>
              <div className="metric-panel">
                <Link2 size={20} />
                <div>
                  <span>Links</span>
                  <strong>{linkEvidenceState}</strong>
                </div>
              </div>
              <div className="metric-panel">
                <FileText size={20} />
                <div>
                  <span>Protocolo</span>
                  <strong>{protocol.lines} linhas</strong>
                </div>
              </div>
            </section>

            <section className="prompt-grid" aria-label="Prompt editorial">
              <div className="panel prompt-panel">
                <div className="panel-heading">
                  <div>
                    <p className="eyebrow">Geracao</p>
                    <h2>Prompt da sessao</h2>
                  </div>
                  <div className="panel-actions">
                    <button
                      className={isResumeLoading ? 'secondary-button busy' : 'secondary-button'}
                      type="button"
                      onClick={() => void requestResumeSession()}
                      aria-busy={isResumeLoading}
                      disabled={isRunPreparing || isResumeLoading}
                    >
                      <Clock3 size={18} />
                      Retomar
                    </button>
                    <button
                      className={isRunPreparing ? 'primary-button busy' : 'primary-button'}
                      type="button"
                      onClick={startEditorialSession}
                      aria-busy={isRunPreparing}
                      disabled={isRunPreparing}
                    >
                      <Play size={18} />
                      {isRunPreparing ? 'Preparando' : 'Submeter'}
                    </button>
                  </div>
                </div>
                <div className="initial-agent-picker" aria-label="Agente redator inicial">
                  <div>
                    <span>Primeira versao</span>
                    <strong>{initialAgentLabel}</strong>
                  </div>
                  <div className="initial-agent-buttons">
                    {initialAgentOptions.map((option) => {
                      const cliBlocksDeepseek = providerMode === 'cli' && option.key === 'deepseek';
                      return (
                        <button
                          className={initialAgent === option.key ? 'active' : ''}
                          type="button"
                          key={option.key}
                          onClick={() => setInitialAgent(option.key)}
                          aria-pressed={initialAgent === option.key}
                          disabled={isRunPreparing || cliBlocksDeepseek}
                          title={
                            cliBlocksDeepseek
                              ? 'DeepSeek so roda via API. Troque para Hibrido ou API para incluir.'
                              : option.detail
                          }
                        >
                          {option.label}
                        </button>
                      );
                    })}
                  </div>
                </div>
                <div className="session-controls" aria-label="Controles da sessao">
                  <div className="control-row">
                    <div>
                      <span>Peers ativos</span>
                      <strong>{activeAgentLabels}</strong>
                    </div>
                    <div className="initial-agent-buttons">
                      {initialAgentOptions.map((option) => {
                        const cliBlocksDeepseek = providerMode === 'cli' && option.key === 'deepseek';
                        const isLastSelected =
                          activeAgents.length === 1 && activeAgents.includes(option.key);
                        return (
                          <button
                            className={activeAgents.includes(option.key) ? 'active' : ''}
                            type="button"
                            key={option.key}
                            onClick={() => toggleActiveAgent(option.key)}
                            aria-pressed={activeAgents.includes(option.key)}
                            disabled={isRunPreparing || cliBlocksDeepseek || isLastSelected}
                            title={
                              cliBlocksDeepseek
                                ? 'DeepSeek so roda via API. Troque para Hibrido ou API para incluir.'
                                : option.detail
                            }
                          >
                            {option.label}
                          </button>
                        );
                      })}
                    </div>
                  </div>
                  {costRatesRequired && (
                    <div className="session-warning" role="status">
                      <AlertTriangle size={16} />
                      <span>
                        Tarifas obrigatorias para API pendentes:{' '}
                        {agentsMissingCostRates
                          .map((agent) => initialAgentOptions.find((option) => option.key === agent)?.label ?? agent)
                          .join(', ')}
                        .
                      </span>
                    </div>
                  )}
                  <div className="limit-grid">
                    <label title="Verificado entre rodadas e como timeout por chamada. Em branco = sem teto.">
                      <Clock3 size={16} />
                      <span>Tempo max. min</span>
                      <input
                        value={maxSessionMinutes}
                        onChange={(event) => setMaxSessionMinutes(event.target.value)}
                        inputMode="numeric"
                        placeholder="60 (em branco = sem teto)"
                        disabled={isRunPreparing}
                      />
                    </label>
                    <label title="Aplica-se apenas a peers em modo API. Peers via CLI rodam sob assinatura. Em branco = sem teto.">
                      <Database size={16} />
                      <span>Custo max. USD</span>
                      <input
                        value={maxSessionCostUsd}
                        onChange={(event) => setMaxSessionCostUsd(event.target.value)}
                        inputMode="decimal"
                        placeholder="5.00 (em branco = sem teto)"
                        disabled={isRunPreparing}
                      />
                    </label>
                  </div>
                  <div className="attachments-row">
                    <label className="secondary-button attachment-button">
                      <Upload size={16} />
                      Anexos
                      <input type="file" multiple onChange={(event) => void handlePromptAttachments(event)} disabled={isRunPreparing} />
                    </label>
                    <span>
                      {promptAttachments.length.toLocaleString('pt-BR')} arquivo(s), {formatBytes(attachmentTotalBytes)}
                    </span>
                  </div>
                  {promptAttachments.length > 0 && (
                    <div className="attachment-list">
                      {attachmentDeliveryPlans.map((plan) => {
                        const hint = attachmentDeliveryHint(plan);
                        return (
                          <button
                            type="button"
                            key={`${plan.attachment.name}-${plan.attachment.size_bytes}`}
                            onClick={() => removePromptAttachment(plan.attachment.name, plan.attachment.size_bytes)}
                            disabled={isRunPreparing}
                            title={`Remover anexo; previsao de entrega: ${hint}. A decisao final acontece no envio.`}
                          >
                            <span>
                              {plan.attachment.name} · {formatBytes(plan.attachment.size_bytes)}
                            </span>
                            <small>{hint}</small>
                          </button>
                        );
                      })}
                    </div>
                  )}
                  <label className="links-control">
                    <span>
                      <Globe2 size={16} />
                      Links da sessao
                    </span>
                    <textarea
                      value={sessionLinks}
                      onChange={(event) => setSessionLinks(event.target.value)}
                      placeholder="https://..."
                      disabled={isRunPreparing}
                    />
                  </label>
                </div>
                <textarea
                  className="prompt-input"
                  value={editorialPrompt}
                  onChange={(event) => setEditorialPrompt(event.target.value)}
                  aria-label="Prompt de geracao editorial"
                />
                <div className="prompt-footer">
                  <span>{editorialPrompt.length.toLocaleString('pt-BR')} caracteres</span>
                  <span>entrega: unanimidade dos agentes</span>
                  <span>run: {sessionRunId ?? 'sem sessao'}</span>
                  <span>{protocol.lines} linhas de protocolo</span>
                </div>
              </div>

              <div className="panel reading-panel">
                <div className="panel-heading">
                  <div>
                    <p className="eyebrow">Regra obrigatoria</p>
                    <h2>Leitura integral</h2>
                  </div>
                  <ShieldCheck size={20} />
                </div>
                <div className="reading-list">
                  {protocolGateItems.map((gate) => (
                    <div className="reading-row" key={gate.agent}>
                      <div>
                        <strong>{gate.agent}</strong>
                        <span>{gate.status}</span>
                      </div>
                      <div className="mini-progress" aria-label={`${gate.progress}%`}>
                        <div style={{ width: `${gate.progress}%` }} />
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            </section>

            <section className="panel operation-panel" aria-label="Sessao editorial">
              <div className="operation-head">
                <div>
                  <p className="eyebrow">Sessao</p>
                  <h2>{operation.title}</h2>
                  <span className={`run-state-badge ${operation.status}`}>{humanizeRunStatus(operation.status)}</span>
                </div>
                <div className="verbosity-control" aria-label="Verbosidade da interface">
                  {verbosityOptions.map((option) => {
                    const Icon = option.icon;
                    return (
                      <button
                        className={verbosity === option.mode ? 'active' : ''}
                        type="button"
                        key={option.mode}
                        aria-pressed={verbosity === option.mode}
                        onClick={() => chooseVerbosity(option.mode)}
                      >
                        <Icon size={16} />
                        {option.label}
                      </button>
                    );
                  })}
                </div>
              </div>

              <div className="operation-body">
                <div className="operation-summary">
                  <div className={`pulse-icon ${operation.status}`}>
                    <Activity size={22} />
                  </div>
                  <div>
                    <strong>{operation.current}</strong>
                    <span>{operation.eta}</span>
                  </div>
                </div>
                <div className="progress-stack" aria-label={operationProgressLabel}>
                  <div className={`progress-track ${operationIndeterminate ? 'indeterminate' : ''}`}>
                    <div
                      className={`progress-fill ${operation.status} ${operationIndeterminate ? 'indeterminate' : ''}`}
                      style={operationIndeterminate ? undefined : { width: `${operation.progress}%` }}
                    />
                  </div>
                  <span>{operationProgressLabel}</span>
                </div>
              </div>

              <div className="phase-list" aria-label="Fases da rodada">
                {phaseItems.map((phase) => (
                  <div className={`phase-item ${phase.state}`} key={phase.label}>
                    <div className="phase-marker" />
                    <strong>{phase.label}</strong>
                    <span>{phase.detail}</span>
                  </div>
                ))}
              </div>

              <div className="activity-feed" aria-label="Atividade">
                {visibleActivity.map((item) => (
                  <div className={`activity-row ${item.level}`} key={`${item.time}-${item.title}`}>
                    <span>{item.time}</span>
                    <div>
                      <strong>{item.title}</strong>
                      <p>{item.detail}</p>
                    </div>
                  </div>
                ))}
              </div>
            </section>

            <section className="panel session-ledger-panel" aria-label="Discussao editorial">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">Ata viva</p>
                  <h2>Discussao e entrega</h2>
                </div>
                <button
                  className="secondary-button"
                  type="button"
                  onClick={() => void openSessionLedger()}
                >
                  <FileText size={18} />
                  Ver ata
                </button>
              </div>
              <div className="ledger-grid">
                <div className="round-list">
                  {discussionItems.map((item) => (
                    <div className="round-row" key={`${item.round}-${item.status}`}>
                      <span>{item.round}</span>
                      <div>
                        <strong>{item.status}</strong>
                        <p>{item.note}</p>
                      </div>
                    </div>
                  ))}
                </div>
                <div className="artifact-list">
                  {finalArtifacts.map((artifact) => (
                    <div className="artifact-card" key={artifact.name}>
                      <FileText size={18} />
                      <div>
                        <strong>{artifact.name}</strong>
                        <span>{artifact.detail}</span>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            </section>

            <section className="panel posteditor-parity-panel" aria-label="Editor integrado">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">Editor integrado</p>
                  <h2>PostEditor parity</h2>
                </div>
                {showPostEditor ? (
                  <span className="parity-badge">HTML MainSite</span>
                ) : (
                  <button className="primary-button" type="button" onClick={openPostEditor}>
                    <FilePlus2 size={18} />
                    Criar Post
                  </button>
                )}
              </div>
              {showPostEditor && (
                <Suspense
                  fallback={
                    <div className="posteditor-loading" role="status">
                      Carregando editor...
                    </div>
                  }
                >
                  <PostEditor
                    editingPostId={null}
                    initialTitle={sessionName}
                    initialAuthor="Leonardo Cardozo Vargas"
                    initialContent={mainSiteHtml}
                    initialIsPublished={false}
                    initialIsAboutSite={false}
                    savingPost={false}
                    showNotification={(message, type) =>
                      void logEvent({
                        level: type === 'error' ? 'error' : 'info',
                        category: 'editor.posteditor.notification',
                        message,
                        context: { type },
                      })
                    }
                    onSave={savePostEditorDraft}
                    onClose={closePostEditor}
                  />
                </Suspense>
              )}
            </section>
          </>
        )}

        {activeSection === 'protocols' && (
          <section className="main-grid" aria-label="Protocolos">
            <div className="panel protocol-panel">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">Biblioteca</p>
                  <h2>Protocolo ativo</h2>
                </div>
                <button className="secondary-button" type="button" onClick={() => inputRef.current?.click()}>
                  <Upload size={18} />
                  Importar
                </button>
                <input ref={inputRef} className="hidden-input" type="file" accept=".md,text/markdown,text/plain" onChange={importProtocol} />
              </div>

              <div className="protocol-record">
                <div className="file-badge">
                  <FileText size={26} />
                </div>
                <div>
                  <strong>{protocol.name}</strong>
                  <span>{protocol.size ? `${protocol.size.toLocaleString('pt-BR')} bytes` : 'artefato fonte local'}</span>
                </div>
              </div>

              <dl className="detail-list">
                <div>
                  <dt>Hash</dt>
                  <dd>{protocol.hash}</dd>
                </div>
                <div>
                  <dt>Linhas</dt>
                  <dd>{protocol.lines.toLocaleString('pt-BR')}</dd>
                </div>
                <div>
                  <dt>Publicacao</dt>
                  <dd>bloqueada ate unanimidade</dd>
                </div>
              </dl>
            </div>

            <div className="panel">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">Chats compartilhados</p>
                  <h2>Entrada externa</h2>
                </div>
                <Link2 size={20} />
              </div>
              <div className="connector-list">
                {importChannels.map((channel) => (
                  <div className="connector-row" key={channel.provider}>
                    <strong>{channel.provider}</strong>
                    <span>{channel.pattern}</span>
                    <em>{channel.status}</em>
                  </div>
                ))}
              </div>
            </div>

            <div className="panel">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">Arquivos</p>
                  <h2>Importar e exportar</h2>
                </div>
                <FileText size={20} />
              </div>
              <div className="pipeline-list">
                {contentPipelines.map((pipeline) => (
                  <div className="pipeline-row" key={pipeline.label}>
                    <span>{pipeline.label}</span>
                    <strong>{pipeline.value}</strong>
                  </div>
                ))}
              </div>
            </div>
          </section>
        )}

        {activeSection === 'evidence' && (
          <section className="main-grid" aria-label="Evidencias">
            <div className="panel evidence-panel">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">Motor mecanico</p>
                  <h2>Evidencias</h2>
                </div>
                <button
                  className={isAuditingEvidence ? 'secondary-button busy' : 'secondary-button'}
                  type="button"
                  onClick={() => void auditEvidenceNow()}
                  disabled={isAuditingEvidence}
                  aria-busy={isAuditingEvidence}
                >
                  {isAuditingEvidence ? <RefreshCw size={18} /> : <Link2 size={18} />}
                  {isAuditingEvidence ? 'Auditando' : 'Auditar links'}
                </button>
              </div>

              <div className="evidence-grid">
                {evidenceRows.map((item) => (
                  <div className={`evidence-tile ${item.tone}`} key={item.label}>
                    <span>{item.label}</span>
                    <strong>{item.value}</strong>
                  </div>
                ))}
              </div>
            </div>

            <div className="panel">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">Web evidence</p>
                  <h2>Coleta assistida</h2>
                </div>
                <Globe2 size={20} />
              </div>
              <div className="pipeline-list">
                {webEvidenceTools.map((tool) => (
                  <div className="pipeline-row" key={tool.label}>
                    <span>{tool.label}</span>
                    <strong>{tool.value}</strong>
                  </div>
                ))}
              </div>
            </div>

            <div className="panel">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">Cloudflare D1</p>
                  <h2>mainsite_posts</h2>
                </div>
                <Database size={20} />
              </div>
              <dl className="detail-list compact">
                <div>
                  <dt>Banco</dt>
                  <dd>bigdata_db</dd>
                </div>
                <div>
                  <dt>Campos</dt>
                  <dd>id, title, content, author, is_published</dd>
                </div>
                <div>
                  <dt>Regra</dt>
                  <dd>API principal; wrangler@latest fallback</dd>
                </div>
              </dl>
            </div>
          </section>
        )}

        {activeSection === 'agents' && (
          <section className="main-grid" aria-label="Agentes">
            <div className="panel">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">{sessionRunId ?? 'sem run'}</p>
                  <h2>Agentes</h2>
                </div>
                <button
                  className="icon-button"
                  type="button"
                  title="Verificar agentes"
                  onClick={() => void verifyAgentsNow()}
                >
                  <Search size={18} />
                </button>
              </div>

              <div className="agent-list">
                {agentCards.map((agent) => (
                  <div className={`agent-row ${agent.state}`} key={agent.name}>
                    <div className="agent-main">
                      <div className="agent-icon">{stateIcon(agent.state)}</div>
                      <div>
                        <strong>{agent.name}</strong>
                        <span>{agent.cli}</span>
                      </div>
                    </div>
                    <div className="agent-status">
                      <strong>{stateLabel(agent.state)}</strong>
                      <span>{agent.note}</span>
                    </div>
                  </div>
                ))}
              </div>
            </div>

            <div className="panel reading-panel">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">Regra obrigatoria</p>
                  <h2>Leitura integral</h2>
                </div>
                <ShieldCheck size={20} />
              </div>
              <div className="reading-list">
                {protocolGateItems.map((gate) => (
                  <div className="reading-row" key={gate.agent}>
                    <div>
                      <strong>{gate.agent}</strong>
                      <span>{gate.status}</span>
                    </div>
                    <div className="mini-progress" aria-label={`${gate.progress}%`}>
                      <div style={{ width: `${gate.progress}%` }} />
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </section>
        )}

        {activeSection === 'settings' && (
          <section className="settings-layout" aria-label="Configuracoes operacionais">
            <aside className="panel settings-nav-panel" aria-label="Areas de configuracao">
              <div>
                <p className="eyebrow">Configuracoes</p>
                <h2>Ajustes do Maestro</h2>
              </div>
              <div className="settings-tabs">
                {settingsTabs.map((item) => {
                  const Icon = item.icon;
                  return (
                    <button
                      className={activeSettingsTab === item.tab ? 'active' : ''}
                      key={item.tab}
                      type="button"
                      aria-pressed={activeSettingsTab === item.tab}
                      onClick={() => chooseSettingsTab(item.tab)}
                    >
                      <Icon size={18} />
                      <span>
                        <strong>{item.label}</strong>
                        <em>{item.detail}</em>
                      </span>
                    </button>
                  );
                })}
              </div>
            </aside>

            <div className="settings-content">
              {activeSettingsTab === 'cloudflare' && (
                <div className="panel settings-panel">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">Ajustes</p>
                  <h2>Cloudflare</h2>
                </div>
                <Database size={20} />
              </div>

              <div className="storage-mode-list" aria-label="Armazenamento de credenciais">
                {credentialStorageModes.map((item) => (
                  <button
                    key={item.mode}
                    className={credentialStorageMode === item.mode ? 'active' : ''}
                    type="button"
                    aria-pressed={credentialStorageMode === item.mode}
                    onClick={() => chooseCredentialStorage(item.mode)}
                  >
                    <strong>{item.label}</strong>
                    <span>{item.detail}</span>
                  </button>
                ))}
              </div>

              <div className="credential-form">
                <div className="storage-note">
                  <strong>{storageModeSummaries[credentialStorageMode].title}</strong>
                  <span>{storageModeSummaries[credentialStorageMode].detail}</span>
                </div>
                <div className="storage-note">
                  <strong>Bootstrap local sem segredos</strong>
                  <span>{bootstrapConfigStatus}</span>
                </div>
                <div className="storage-note">
                  <strong>Token Cloudflare inicial</strong>
                  <span>
                    {cloudflareTokenAvailable
                      ? `detectado via ${cloudflareEnvSnapshot?.api_token_env_var ?? cloudflareTokenEnvVar}${
                          cloudflareEnvSnapshot?.api_token_env_scope ? ` (${cloudflareEnvSnapshot.api_token_env_scope})` : ''
                        }`
                      : 'nao salvo no bootstrap; informe no campo, env var ou futura cripta local'}
                  </span>
                </div>
                <div className="field-group">
                  <label htmlFor="cloudflare-account-id">Account ID</label>
                  <input
                    id="cloudflare-account-id"
                    autoComplete="off"
                    spellCheck={false}
                    value={cloudflareAccountId}
                    onChange={(event) => setCloudflareAccountId(event.target.value)}
                    placeholder="informar no app local"
                  />
                </div>
                <div className="field-group">
                  <label htmlFor="cloudflare-api-token">API token</label>
                  <input
                    id="cloudflare-api-token"
                    type="password"
                    autoComplete="off"
                    spellCheck={false}
                    value={cloudflareApiToken}
                    onChange={(event) => setCloudflareApiToken(event.target.value)}
                    placeholder="nunca gravar em logs ou artefatos"
                  />
                </div>
                <div className="target-grid">
                  <div>
                    <span>Persistencia</span>
                    <strong>maestro_db</strong>
                  </div>
                  <div>
                    <span>Secrets</span>
                    <strong>Cloudflare Secrets Store</strong>
                  </div>
                  <div>
                    <span>Publicacao</span>
                    <strong>bigdata_db</strong>
                  </div>
                  <div>
                    <span>Tabela</span>
                    <strong>mainsite_posts</strong>
                  </div>
                </div>
                <button
                  className={isVerifyingCloudflare ? 'primary-button busy' : 'primary-button'}
                  type="button"
                  onClick={() => void verifyCloudflareCredentials()}
                  disabled={isVerifyingCloudflare}
                >
                  {isVerifyingCloudflare ? <RefreshCw size={18} /> : <ShieldCheck size={18} />}
                  {isVerifyingCloudflare ? 'Verificando e preparando' : 'Verificar e preparar'}
                </button>
              </div>

              <div className="status-checklist" aria-label="Permissoes Cloudflare">
                {cloudflarePermissionRows.map((item) => (
                  <div className={`check-row ${item.tone}`} key={item.label}>
                    {item.tone === 'ok' ? (
                      <CheckCircle2 size={15} />
                    ) : item.tone === 'blocked' || item.tone === 'error' || item.tone === 'warn' ? (
                      <AlertTriangle size={15} />
                    ) : (
                      <Clock3 size={15} />
                    )}
                    <span>{item.label}</span>
                    <strong>{item.value}</strong>
                  </div>
                ))}
              </div>
                </div>
              )}

              {activeSettingsTab === 'providers' && (
                <div className="panel settings-panel">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">Ajustes</p>
                  <h2>Agentes via API</h2>
                </div>
                <KeyRound size={20} />
              </div>

              <div className="provider-mode" aria-label="Modo dos provedores">
                {(['hybrid', 'cli', 'api'] as const).map((mode) => (
                  <button
                    key={mode}
                    className={providerMode === mode ? 'active' : ''}
                    type="button"
                    aria-pressed={providerMode === mode}
                    onClick={() => chooseProviderMode(mode)}
                  >
                    {mode === 'hybrid' ? 'Hibrido' : mode.toUpperCase()}
                  </button>
                ))}
              </div>
              <div className="provider-mode-note">
                <strong>Execucao API real por peer</strong>
                <span>
                  <strong>API</strong> roda os 4 peers via provedores oficiais.
                  {' '}<strong>Hibrido</strong> reserva DeepSeek para API (nao tem CLI) e
                  Claude, Codex, Gemini para CLI, sempre, independentemente das chaves.
                  {' '}<strong>CLI</strong> roda os 3 peers com CLI; DeepSeek fica desabilitado
                  porque nao possui integracao CLI.
                  Tarifas continuam obrigatorias para qualquer chamada de API.
                </span>
              </div>

              <div className="ai-credential-list">
                {aiProviderRows.map((provider) => (
                  <div className="credential-row" key={provider.key}>
                    <div>
                      <strong>{provider.name}</strong>
                      <span>CLI: {provider.cli}</span>
                    </div>
                    <label>
                      {provider.secretLabel}
                      <input
                        type="password"
                        autoComplete="off"
                        spellCheck={false}
                        value={aiCredentials[provider.key]}
                        onChange={(event) => updateAiCredential(provider.key, event.target.value)}
                        placeholder="informar no app local"
                      />
                    </label>
                    <em>{provider.meta}</em>
                  </div>
                ))}
              </div>

              <div className="rate-card-panel" aria-label="Tabela de tarifas dos provedores">
                <div>
                  <strong>Tabela de tarifas</strong>
                  <span>
                    Valores em USD por 1M tokens. O limite de custo continua sendo unico por sessao; esta tabela
                    apenas calcula e audita consumo observado. Sem fallback por env var.
                  </span>
                </div>
                <div className="rate-card-table">
                  <div className="rate-card-head" aria-hidden="true">
                    <span>Provedor</span>
                    <span>Entrada</span>
                    <span>Saida</span>
                  </div>
                  {providerRateRows.map((provider) => (
                    <div className="rate-card-row" key={provider.key}>
                      <div>
                        <strong>{provider.name}</strong>
                        <span>{provider.hint}</span>
                      </div>
                      <label>
                        <span>Entrada USD / 1M</span>
                        <input
                          inputMode="decimal"
                          value={providerInputUsdPerMillion[provider.key]}
                          onChange={(event) => updateProviderInputRate(provider.key, event.target.value)}
                          placeholder="ex.: 0.55"
                        />
                      </label>
                      <label>
                        <span>Saida USD / 1M</span>
                        <input
                          inputMode="decimal"
                          value={providerOutputUsdPerMillion[provider.key]}
                          onChange={(event) => updateProviderOutputRate(provider.key, event.target.value)}
                          placeholder="ex.: 2.19"
                        />
                      </label>
                    </div>
                  ))}
                </div>
              </div>

              <div className="settings-status" role="status" aria-live="polite">
                {aiConfigStatus}
              </div>

              <div className="button-row">
                <button
                  className={isSavingAiConfig ? 'secondary-button busy' : 'secondary-button'}
                  type="button"
                  onClick={() => void saveAiProviderConfig()}
                  disabled={isSavingAiConfig || isVerifyingAiProviders}
                  aria-busy={isSavingAiConfig}
                >
                  <KeyRound size={18} />
                  {isSavingAiConfig ? 'Salvando' : 'Salvar APIs'}
                </button>
                <button
                  className={isVerifyingAiProviders ? 'secondary-button busy' : 'secondary-button'}
                  type="button"
                  onClick={() => void verifyAiProviderCredentials()}
                  disabled={isSavingAiConfig || isVerifyingAiProviders}
                  aria-busy={isVerifyingAiProviders}
                >
                  <ListChecks size={18} />
                  {isVerifyingAiProviders ? 'Verificando' : 'Verificar APIs'}
                </button>
              </div>

              <div className="check-list compact-checks" aria-label="Resultado da verificacao das APIs">
                {aiProviderRowsState.map((item) => (
                  <div className={`check-row ${item.tone}`} key={item.label}>
                    {item.tone === 'ok' ? (
                      <CheckCircle2 size={15} />
                    ) : item.tone === 'blocked' || item.tone === 'error' || item.tone === 'warn' ? (
                      <AlertTriangle size={15} />
                    ) : (
                      <Clock3 size={15} />
                    )}
                    <span>{item.label}</span>
                    <strong>{item.value}</strong>
                  </div>
                ))}
              </div>
                </div>
              )}
            </div>
          </section>
        )}

        {activeSection === 'setup' && (
          <section className="integration-grid" aria-label="Setup">
            <div className="panel">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">Primeira execucao</p>
                  <h2>Bootstrap</h2>
                </div>
                <HardDriveDownload size={20} />
              </div>
              <div className="pipeline-list">
                {bootstrapRows.map((item) => (
                  <div className={`pipeline-row ${item.tone}`} key={item.label}>
                    <span>{item.label}</span>
                    <strong>{item.value}</strong>
                  </div>
                ))}
              </div>
            </div>

            <div className="panel">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">Runtime</p>
                  <h2>Diagnostico</h2>
                </div>
                <Activity size={20} />
              </div>
              <dl className="detail-list compact">
                <div>
                  <dt>Run atual</dt>
                  <dd>{sessionRunId ?? 'sem sessao editorial'}</dd>
                </div>
                <div>
                  <dt>Estado</dt>
                  <dd>{humanizeRunStatus(operation.status)}</dd>
                </div>
                <div>
                  <dt>Logs</dt>
                  <dd>um arquivo de diagnostico por execucao do app</dd>
                </div>
                <div>
                  <dt>Config inicial</dt>
                  <dd>data/config/bootstrap.json sem segredos</dd>
                </div>
                <div>
                  <dt>Cloudflare env</dt>
                <dd>
                  {cloudflareEnvSnapshot?.api_token_present
                    ? `token em ${cloudflareEnvSnapshot.api_token_env_var} (${cloudflareEnvSnapshot.api_token_env_scope ?? 'process'})`
                    : 'token nao detectado'}
                </dd>
                </div>
              </dl>
            </div>
          </section>
        )}
      </main>
    </div>
  );
}
