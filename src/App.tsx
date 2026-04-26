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
  Upload,
  Globe2,
} from 'lucide-react';
import type { ChangeEvent, ComponentType } from 'react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { logEvent } from './diagnostics';
import PostEditor from './editor/posteditor/PostEditor';

type ProtocolSnapshot = {
  name: string;
  size: number;
  lines: number;
  hash: string;
};

type AgentState = 'ready' | 'blocked' | 'evidence';
type VerbosityMode = 'resumo' | 'detalhado' | 'diagnostico';
type PhaseState = 'done' | 'active' | 'waiting';
type ProviderMode = 'cli' | 'api' | 'hybrid';
type AiCredentialKey = 'openai' | 'anthropic' | 'gemini';
type CredentialStorageMode = 'vault' | 'windows_env' | 'local_json';

type AgentCard = {
  name: string;
  cli: string;
  state: AgentState;
  note: string;
};

type ProtocolReadingGate = {
  agent: string;
  progress: number;
  status: string;
};

const agents: AgentCard[] = [
  { name: 'Claude', cli: 'claude', state: 'ready', note: '12 regras editoriais ativas' },
  { name: 'Codex', cli: 'codex', state: 'evidence', note: '2 links aguardando HEAD/GET' },
  { name: 'Gemini', cli: 'gemini', state: 'blocked', note: '1 fonte em quarentena' },
  { name: 'MaestroPeer', cli: 'deterministico', state: 'evidence', note: 'ABNT: 2 citacoes sem localizador' },
];

const evidenceRows = [
  { label: 'DOI', value: '0 pendente', tone: 'ok' },
  { label: 'Links', value: '2 em cross-review', tone: 'warn' },
  { label: 'ABNT', value: '2 bloqueios', tone: 'warn' },
  { label: 'Quarentena', value: '1 item', tone: 'danger' },
];

const protocolReadingGates: ProtocolReadingGate[] = [
  { agent: 'Claude', progress: 100, status: '558/558 linhas confirmadas' },
  { agent: 'Codex', progress: 100, status: 'hash + checklist validados' },
  { agent: 'Gemini', progress: 100, status: 'anexos e regras indexados' },
];

const discussionRounds = [
  { round: '001', status: 'NEEDS_EVIDENCE', note: 'Codex pediu conferencia mecanica de 2 URLs.' },
  { round: '002', status: 'NOT_READY', note: 'Claude manteve 4 bloqueios bibliograficos.' },
  { round: '003', status: 'READY pendente', note: 'Aguardando Gemini reler a ata parcial.' },
];

const finalArtifacts = [
  { name: 'texto-final.md', detail: 'somente entregue com unanimidade trilateral' },
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

const bootstrapChecks = [
  { label: 'WebView2', value: 'pronto' },
  { label: 'Claude CLI', value: 'auth requerida' },
  { label: 'Codex CLI', value: 'verificar versao' },
  { label: 'Gemini CLI', value: 'instalar/configurar' },
  { label: 'Cloudflare API', value: 'D1 principal' },
  { label: 'Wrangler', value: '@latest fallback' },
];

const cloudflarePermissionChecks = [
  { label: 'Token ativo', value: 'verify endpoint' },
  { label: 'Conta acessivel', value: 'account id' },
  { label: 'D1 Read', value: 'importacao' },
  { label: 'D1 Write', value: 'publicacao' },
];

const credentialStorageModes = [
  { mode: 'vault', label: 'Vault local', detail: 'JSON criptografado e ignorado' },
  { mode: 'windows_env', label: 'Env var Windows', detail: 'CurrentUser; maquina exige UAC' },
  { mode: 'local_json', label: 'JSON local', detail: 'somente com alerta de risco' },
] satisfies Array<{ mode: CredentialStorageMode; label: string; detail: string }>;

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
] satisfies Array<{
  key: AiCredentialKey;
  name: string;
  cli: string;
  secretLabel: string;
  meta: string;
}>;

const verbosityOptions = [
  { mode: 'resumo', label: 'Resumo', icon: EyeOff },
  { mode: 'detalhado', label: 'Detalhado', icon: Eye },
  { mode: 'diagnostico', label: 'Diagnostico', icon: ListChecks },
] satisfies Array<{ mode: VerbosityMode; label: string; icon: ComponentType<{ size?: number }> }>;

const backgroundOperation = {
  title: 'Conferencia editorial em background',
  progress: 62,
  current: 'Checando links e consolidando evidencias',
  eta: 'rodada 001',
};

const phases = [
  { label: 'Protocolo', detail: 'hash fixado', state: 'done' },
  { label: 'Triagem', detail: 'texto segmentado', state: 'done' },
  { label: 'Evidencias', detail: 'links em HEAD/GET', state: 'active' },
  { label: 'Consenso', detail: 'aguardando unanimidade', state: 'waiting' },
] satisfies Array<{ label: string; detail: string; state: PhaseState }>;

const activityFeed = [
  {
    level: 'summary',
    time: 'agora',
    title: 'Motor editorial ativo',
    detail: 'As CLIs rodam ocultas; a interface mostra somente estados operacionais.',
  },
  {
    level: 'detail',
    time: '00:18',
    title: 'Codex solicitou evidencia externa',
    detail: '2 URLs foram movidas para verificacao mecanica antes da proxima rodada.',
  },
  {
    level: 'detail',
    time: '00:24',
    title: 'Claude marcou correcoes objetivas',
    detail: '12 itens editoriais continuam bloqueando a aceitacao formal.',
  },
  {
    level: 'diagnostic',
    time: '00:31',
    title: 'Evento estruturado gravado',
    detail: 'agent.round.status em data/logs/maestro-YYYY-MM-DD.ndjson.',
  },
];

function stateLabel(state: AgentState) {
  if (state === 'ready') return 'READY';
  if (state === 'evidence') return 'NEEDS_EVIDENCE';
  return 'NOT_READY';
}

function stateIcon(state: AgentState) {
  if (state === 'ready') return <CheckCircle2 size={16} />;
  if (state === 'evidence') return <Clock3 size={16} />;
  return <AlertTriangle size={16} />;
}

async function sha256(text: string) {
  const bytes = new TextEncoder().encode(text);
  const buffer = await crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(buffer)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

export function App() {
  const inputRef = useRef<HTMLInputElement>(null);
  const [protocol, setProtocol] = useState<ProtocolSnapshot>({
    name: 'protocolo-editorial-v1-10-0.md',
    size: 0,
    lines: 558,
    hash: 'artefato local ignorado pelo Git',
  });
  const [sessionName, setSessionName] = useState('Artigo academico sem titulo');
  const [verbosity, setVerbosity] = useState<VerbosityMode>('detalhado');
  const [editorialPrompt, setEditorialPrompt] = useState(
    'Escreva um artigo academico sobre o tema informado, seguindo integralmente o protocolo editorial ativo.',
  );
  const [mainSiteHtml, setMainSiteHtml] = useState(
    '<h1>Artigo em preparacao</h1><p style="text-align: justify">Texto inicial para edicao com o mesmo PostEditor usado pelo MainSite.</p>',
  );
  const [providerMode, setProviderMode] = useState<ProviderMode>('hybrid');
  const [credentialStorageMode, setCredentialStorageMode] = useState<CredentialStorageMode>('vault');
  const [cloudflareAccountId, setCloudflareAccountId] = useState('');
  const [cloudflareApiToken, setCloudflareApiToken] = useState('');
  const [aiCredentials, setAiCredentials] = useState<Record<AiCredentialKey, string>>({
    openai: '',
    anthropic: '',
    gemini: '',
  });

  const readyCount = useMemo(() => agents.filter((agent) => agent.state === 'ready').length, []);
  const visibleActivity = useMemo(() => {
    if (verbosity === 'resumo') return activityFeed.slice(0, 1);
    if (verbosity === 'detalhado') return activityFeed.filter((item) => item.level !== 'diagnostic');
    return activityFeed;
  }, [verbosity]);

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

  function chooseVerbosity(nextVerbosity: VerbosityMode) {
    setVerbosity(nextVerbosity);
    void logEvent({
      level: 'info',
      category: 'ui.verbosity.changed',
      message: 'operator changed interface verbosity',
      context: { verbosity: nextVerbosity, session_name: sessionName },
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
    void logEvent({
      level: 'info',
      category: 'protocol.imported',
      message: 'operator imported editorial protocol',
      context: nextProtocol,
    });
    event.target.value = '';
  }

  function startEditorialSession() {
    void logEvent({
      level: 'info',
      category: 'session.prompt.submitted',
      message: 'operator submitted editorial generation prompt',
      context: {
        session_name: sessionName,
        prompt_chars: editorialPrompt.length,
        protocol_name: protocol.name,
        protocol_lines: protocol.lines,
        required_outputs: finalArtifacts.map((artifact) => artifact.name),
        consensus_gate: 'trilateral_ready_same_round',
      },
    });
  }

  function updateAiCredential(provider: AiCredentialKey, value: string) {
    setAiCredentials((current) => ({ ...current, [provider]: value }));
  }

  function chooseProviderMode(nextMode: ProviderMode) {
    setProviderMode(nextMode);
    void logEvent({
      level: 'info',
      category: 'settings.provider_mode.changed',
      message: 'operator changed AI provider orchestration mode',
      context: { provider_mode: nextMode },
    });
  }

  function chooseCredentialStorage(nextMode: CredentialStorageMode) {
    setCredentialStorageMode(nextMode);
    void logEvent({
      level: 'info',
      category: 'settings.credential_storage.changed',
      message: 'operator changed credential storage mode',
      context: { credential_storage_mode: nextMode },
    });
  }

  function verifyCloudflareCredentials() {
    void logEvent({
      level: 'info',
      category: 'settings.cloudflare.verify_requested',
      message: 'operator requested Cloudflare credential validation',
      context: {
        account_id_present: cloudflareAccountId.trim().length > 0,
        token_present: cloudflareApiToken.length > 0,
        target_database: 'bigdata_db',
        target_table: 'mainsite_posts',
        credential_storage_mode: credentialStorageMode,
      },
    });
  }

  function verifyAiProviderCredentials() {
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
      },
    });
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

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">M</div>
          <div>
            <div className="brand-name">Maestro Editorial AI</div>
            <div className="brand-meta">Windows 11+ portable</div>
          </div>
        </div>

        <nav className="nav-list" aria-label="Principal">
          <button className="nav-item active" type="button">
            <GitBranch size={18} />
            Sessao
          </button>
          <button className="nav-item" type="button">
            <FileText size={18} />
            Protocolos
          </button>
          <button className="nav-item" type="button">
            <Globe2 size={18} />
            Evidencias
          </button>
          <button className="nav-item" type="button">
            <Bot size={18} />
            Agentes
          </button>
          <button className="nav-item" type="button">
            <Settings size={18} />
            Ajustes
          </button>
          <button className="nav-item" type="button">
            <HardDriveDownload size={18} />
            Setup
          </button>
        </nav>

        <div className="storage-strip">
          <Database size={18} />
          <div>
            <strong>JSON local</strong>
            <span>data/logs/*.ndjson pronto para diagnostico</span>
          </div>
        </div>
      </aside>

      <main className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">Sessao editorial</p>
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
              onClick={() =>
                void logEvent({
                  level: 'info',
                  category: 'ui.command',
                  message: 'operator requested revalidation',
                  context: { session_name: sessionName },
                })
              }
            >
              <RefreshCw size={18} />
            </button>
            <button
              className="primary-button"
              type="button"
              onClick={startEditorialSession}
            >
              <Play size={18} />
              Iniciar sessao
            </button>
          </div>
        </header>

        <section className="status-grid" aria-label="Resumo">
          <div className="metric-panel">
            <ShieldCheck size={20} />
            <div>
              <span>Estado formal</span>
              <strong>auditoria_bibliografica</strong>
            </div>
          </div>
          <div className="metric-panel">
            <Bot size={20} />
            <div>
              <span>Consenso</span>
              <strong>{readyCount}/{agents.length} READY</strong>
            </div>
          </div>
          <div className="metric-panel">
            <Link2 size={20} />
            <div>
              <span>Links</span>
              <strong>2 pendentes</strong>
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
              <button className="primary-button" type="button" onClick={startEditorialSession}>
                <Play size={18} />
                Submeter
              </button>
            </div>
            <textarea
              className="prompt-input"
              value={editorialPrompt}
              onChange={(event) => setEditorialPrompt(event.target.value)}
              aria-label="Prompt de geracao editorial"
            />
            <div className="prompt-footer">
              <span>{editorialPrompt.length.toLocaleString('pt-BR')} caracteres</span>
              <span>bloqueio: unanimidade trilateral</span>
              <span>MaestroPeer precisa aprovar</span>
              <span>divergencia mantem sessao aberta</span>
              <span>{protocol.lines} linhas de protocolo</span>
            </div>
          </div>

          <div className="panel reading-panel">
            <div className="panel-heading">
              <div>
                <p className="eyebrow">Hard gate</p>
                <h2>Leitura integral</h2>
              </div>
              <ShieldCheck size={20} />
            </div>
            <div className="reading-list">
              {protocolReadingGates.map((gate) => (
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

        <section className="panel operation-panel" aria-label="Operacao em background">
          <div className="operation-head">
            <div>
              <p className="eyebrow">Orquestracao</p>
              <h2>{backgroundOperation.title}</h2>
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
              <div className="pulse-icon">
                <Activity size={22} />
              </div>
              <div>
                <strong>{backgroundOperation.current}</strong>
                <span>{backgroundOperation.eta}</span>
              </div>
            </div>
            <div className="progress-stack" aria-label={`${backgroundOperation.progress}% concluido`}>
              <div className="progress-track">
                <div className="progress-fill" style={{ width: `${backgroundOperation.progress}%` }} />
              </div>
              <span>{backgroundOperation.progress}%</span>
            </div>
          </div>

          <div className="phase-list" aria-label="Fases da rodada">
            {phases.map((phase) => (
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

        <section className="panel session-ledger-panel" aria-label="Discussao trilateral">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">Ata viva</p>
              <h2>Discussao e entrega</h2>
            </div>
            <button className="secondary-button" type="button">
              <FileText size={18} />
              Ver ata
            </button>
          </div>
          <div className="ledger-grid">
            <div className="round-list">
              {discussionRounds.map((item) => (
                <div className="round-row" key={item.round}>
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
            <span className="parity-badge">HTML MainSite</span>
          </div>
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
            onClose={() =>
              void logEvent({
                level: 'info',
                category: 'editor.posteditor.close',
                message: 'operator closed PostEditor-compatible editor panel',
              })
            }
          />
        </section>

        <section className="main-grid">
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
                <dt>Pin de sessao</dt>
                <dd>versao + timestamp + sha256</dd>
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
                <p className="eyebrow">Rodada 001</p>
                <h2>Agentes</h2>
              </div>
              <button className="icon-button" type="button" title="Procurar">
                <Search size={18} />
              </button>
            </div>

            <div className="agent-list">
              {agents.map((agent) => (
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

          <div className="panel evidence-panel">
            <div className="panel-heading">
              <div>
                <p className="eyebrow">Motor mecanico</p>
                <h2>Evidencias</h2>
              </div>
              <button className="secondary-button" type="button">
                <Link2 size={18} />
                Auditar
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
        </section>

        <section className="settings-grid" aria-label="Configuracoes operacionais">
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
                  placeholder="armazenamento local criptografado"
                />
              </div>
              <div className="target-grid">
                <div>
                  <span>Banco</span>
                  <strong>bigdata_db</strong>
                </div>
                <div>
                  <span>Tabela</span>
                  <strong>mainsite_posts</strong>
                </div>
              </div>
              <button className="primary-button" type="button" onClick={verifyCloudflareCredentials}>
                <ShieldCheck size={18} />
                Verificar token
              </button>
            </div>

            <div className="status-checklist" aria-label="Permissoes Cloudflare">
              {cloudflarePermissionChecks.map((item) => (
                <div className="check-row" key={item.label}>
                  <CheckCircle2 size={15} />
                  <span>{item.label}</span>
                  <strong>{item.value}</strong>
                </div>
              ))}
            </div>
          </div>

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

            <button className="secondary-button" type="button" onClick={verifyAiProviderCredentials}>
              <ListChecks size={18} />
              Verificar APIs
            </button>
          </div>
        </section>

        <section className="integration-grid" aria-label="Importacao e publicacao">
          <div className="panel">
            <div className="panel-heading">
              <div>
                <p className="eyebrow">Primeira execucao</p>
                <h2>Bootstrap</h2>
              </div>
              <HardDriveDownload size={20} />
            </div>
            <div className="pipeline-list">
              {bootstrapChecks.map((item) => (
                <div className="pipeline-row" key={item.label}>
                  <span>{item.label}</span>
                  <strong>{item.value}</strong>
                </div>
              ))}
            </div>
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
      </main>
    </div>
  );
}
