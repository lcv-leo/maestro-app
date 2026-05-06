// Modulo: src/constants.ts
// Descricao: Initial seed values and static option tables extracted from
// `src/App.tsx` in v0.5.9 per `docs/code-split-plan.md` (frontend track).
// Pure data-only extraction — every const definition preserved verbatim
// from App.tsx v0.5.8 (commit cbfc02d). No runtime computations beyond
// `defaultActiveAgents = initialAgentOptions.map(...)` and
// `navItems = navGroups.flatMap(...)`, both immutable derivations.

import {
  Bot,
  Database,
  Eye,
  EyeOff,
  FileText,
  GitBranch,
  Globe2,
  HardDriveDownload,
  KeyRound,
  ListChecks,
  Settings,
} from 'lucide-react';
import type { ComponentType } from 'react';

import type {
  ActivityItem,
  AgentCard,
  AiCredentialKey,
  AiProviderProbeRow,
  BootstrapCheckRow,
  CloudflarePermissionRow,
  CredentialStorageMode,
  DiscussionRound,
  EvidenceRow,
  InitialAgentKey,
  NavItem,
  OperationSnapshot,
  PhaseItem,
  ProtocolReadingGate,
  ProviderRateKey,
  SettingsTab,
  VerbosityMode,
} from './types';

export const initialAgents: AgentCard[] = [
  { name: 'Claude', cli: 'claude', state: 'blocked', note: 'aguardando sessao editorial' },
  { name: 'Codex', cli: 'codex', state: 'blocked', note: 'aguardando sessao editorial' },
  { name: 'Gemini', cli: 'gemini', state: 'blocked', note: 'aguardando sessao editorial' },
  { name: 'DeepSeek', cli: 'deepseek-api', state: 'blocked', note: 'aguardando chave de API' },
  { name: 'Grok', cli: 'grok-api', state: 'blocked', note: 'aguardando chave de API' },
  { name: 'Maestro', cli: 'motor local', state: 'blocked', note: 'aguardando verificacoes iniciais' },
];

export const initialEvidenceRows: EvidenceRow[] = [
  { label: 'DOI', value: 'nao iniciado', tone: 'idle' },
  { label: 'Links', value: 'nao iniciado', tone: 'idle' },
  { label: 'ABNT', value: 'nao iniciado', tone: 'idle' },
  { label: 'Quarentena', value: 'nao iniciado', tone: 'idle' },
];

export const initialProtocolReadingGates: ProtocolReadingGate[] = [
  { agent: 'Claude', progress: 0, status: 'Aguardando' },
  { agent: 'Codex', progress: 0, status: 'Aguardando' },
  { agent: 'Gemini', progress: 0, status: 'Aguardando' },
  { agent: 'DeepSeek', progress: 0, status: 'Aguardando' },
  { agent: 'Grok', progress: 0, status: 'Aguardando' },
];

export const initialDiscussionRounds: DiscussionRound[] = [
  { round: '--', status: 'Sem rodada', note: 'Submeta um prompt para criar a primeira ata operacional.' },
];

export const finalArtifacts = [
  { name: 'texto-final.md', detail: 'somente entregue com unanimidade dos agentes' },
  { name: 'ata-da-sessao.md', detail: 'prompt, protocolo, rounds, divergencias e decisoes' },
];

export const importChannels = [
  { provider: 'ChatGPT', pattern: 'chatgpt.com/share/<id>', status: 'snapshot publico' },
  { provider: 'Claude', pattern: 'claude.ai/share/...', status: 'snapshot com artifacts' },
  { provider: 'Gemini', pattern: 'g.co/gemini/share/...', status: 'link publico normalizado' },
];

export const contentPipelines = [
  { label: 'Editor PostEditor', value: 'mesma funcionalidade e HTML' },
  { label: 'Markdown puro', value: 'ler + gerar' },
  { label: 'Markdown + HTML', value: 'preservar tabelas e midia' },
  { label: 'PDF', value: 'importar, extrair e exportar' },
  { label: 'D1 mainsite_posts', value: 'sincronizar com BigData' },
];

export const webEvidenceTools = [
  { label: 'fetch', value: 'HEAD/GET, redirects, hash' },
  { label: 'curl', value: 'replay com segredos ocultos' },
  { label: 'web search', value: 'provedores configuraveis' },
  { label: 'navegador assistido', value: 'CAPTCHA/login com humano' },
];

export const initialBootstrapChecks: BootstrapCheckRow[] = [
  { label: 'WebView2', value: 'verificacao pendente', tone: 'pending' },
  { label: 'Claude CLI', value: 'verificacao pendente', tone: 'pending' },
  { label: 'Codex CLI', value: 'verificacao pendente', tone: 'pending' },
  { label: 'Gemini CLI', value: 'verificacao pendente', tone: 'pending' },
  { label: 'Cloudflare env', value: 'verificacao pendente', tone: 'pending' },
  { label: 'Wrangler', value: 'aguardando autorizacao', tone: 'pending' },
];

export const initialCloudflarePermissionChecks: CloudflarePermissionRow[] = [
  { label: 'Token ativo', value: 'pendente de verificacao', tone: 'pending' },
  { label: 'Conta acessivel', value: 'pendente de verificacao', tone: 'pending' },
  { label: 'D1 Read/Edit', value: 'pendente de verificacao', tone: 'pending' },
  { label: 'Secrets Store', value: 'pendente de verificacao', tone: 'pending' },
];

export const initialAiProviderChecks: AiProviderProbeRow[] = [
  { label: 'OpenAI / Codex', value: 'pendente de verificacao', tone: 'pending' },
  { label: 'Anthropic / Claude', value: 'pendente de verificacao', tone: 'pending' },
  { label: 'Google / Gemini', value: 'pendente de verificacao', tone: 'pending' },
  { label: 'DeepSeek', value: 'pendente de verificacao', tone: 'pending' },
  { label: 'Grok / xAI', value: 'pendente de verificacao', tone: 'pending' },
];

export const credentialStorageModes = [
  { mode: 'local_json', label: 'JSON local', detail: 'configuracoes e segredos em JSON ignorado' },
  { mode: 'windows_env', label: 'Env var Windows', detail: 'segredos em env var; configs em JSON' },
  { mode: 'cloudflare', label: 'Cloudflare', detail: 'maestro_db + Secrets Store remoto (execucao local exige MAESTRO_*_API_KEY em env)' },
] satisfies Array<{ mode: CredentialStorageMode; label: string; detail: string }>;

export const storageModeSummaries: Record<CredentialStorageMode, { title: string; detail: string }> = {
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
      'Configuracoes em D1 maestro_db; segredos centralizados no Cloudflare Secrets Store. Importante: o app nao busca segredos remotos em runtime; para executar peers via API localmente, mantenha MAESTRO_OPENAI_API_KEY / MAESTRO_ANTHROPIC_API_KEY / MAESTRO_GEMINI_API_KEY / MAESTRO_DEEPSEEK_API_KEY / MAESTRO_GROK_API_KEY em env vars (ou na config local). Esse modo e escolha de armazenamento canonico, nao alimenta execucao local sozinho.',
  },
};

export const aiProviderRows = [
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
  {
    key: 'grok',
    name: 'Grok / xAI',
    cli: 'grok-api',
    secretLabel: 'Grok API key',
    meta: 'API oficial xAI; melhor modelo disponivel via /models',
  },
] satisfies Array<{
  key: AiCredentialKey;
  name: string;
  cli: string;
  secretLabel: string;
  meta: string;
}>;

export const providerRateRows = [
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
  {
    key: 'grok',
    name: 'Grok / xAI',
    hint: 'Obrigatorio para sessoes com Grok via API.',
  },
] satisfies Array<{ key: ProviderRateKey; name: string; hint: string }>;

export const initialAgentOptions = [
  { key: 'claude', label: 'Claude', detail: 'primeira versao e revisoes' },
  { key: 'codex', label: 'Codex', detail: 'primeira versao e revisoes' },
  { key: 'gemini', label: 'Gemini', detail: 'primeira versao e revisoes' },
  { key: 'deepseek', label: 'DeepSeek', detail: 'primeira versao e revisoes via API' },
  { key: 'grok', label: 'Grok', detail: 'primeira versao e revisoes via API' },
] satisfies Array<{ key: InitialAgentKey; label: string; detail: string }>;

export const defaultActiveAgents = initialAgentOptions.map((option) => option.key);
export const attachmentLimits = {
  maxFiles: 8,
  maxFileBytes: 25 * 1024 * 1024,
  maxTotalBytes: 75 * 1024 * 1024,
  maxNativeApiBytes: 20 * 1024 * 1024,
};

export const verbosityOptions = [
  { mode: 'resumo', label: 'Resumo', icon: EyeOff },
  { mode: 'detalhado', label: 'Detalhado', icon: Eye },
  { mode: 'diagnostico', label: 'Diagnostico', icon: ListChecks },
] satisfies Array<{ mode: VerbosityMode; label: string; icon: ComponentType<{ size?: number }> }>;

export const navGroups: Array<{ label: string; items: NavItem[] }> = [
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

export const navItems: NavItem[] = navGroups.flatMap((group) => group.items);

export const settingsTabs = [
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

export const idleOperation: OperationSnapshot = {
  title: 'Aguardando sessao editorial',
  progress: 0,
  current: 'Nenhum prompt foi submetido nesta execucao.',
  eta: 'ocioso',
  status: 'idle',
};

export const idlePhases: PhaseItem[] = [
  { label: 'Protocolo', detail: 'aguardando prompt', state: 'waiting' },
  { label: 'Verificacoes', detail: 'nao iniciadas', state: 'waiting' },
  { label: 'Agentes', detail: 'nao iniciados', state: 'waiting' },
  { label: 'Entrega', detail: 'bloqueada ate unanimidade', state: 'waiting' },
];

export const idleActivityFeed: ActivityItem[] = [
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
