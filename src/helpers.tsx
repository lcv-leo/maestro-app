// Modulo: src/helpers.tsx
// Descricao: Pure helper functions extracted from `src/App.tsx` in v0.5.9
// per `docs/code-split-plan.md` (frontend track). Every function preserved
// verbatim from App.tsx v0.5.8 (commit cbfc02d). No hooks, no React
// component declarations beyond the inline JSX that `stateIcon` already
// returned in the original location. The .tsx extension is used because
// of `stateIcon`'s JSX return value; the rest of the helpers are plain
// TypeScript.

import { AlertTriangle, CheckCircle2, Clock3, RefreshCw } from 'lucide-react';

import { attachmentLimits } from './constants';
import type {
  AgentCard,
  AgentState,
  AiCredentialKey,
  AttachmentDeliveryPlan,
  EditorialAgentResult,
  NativeAttachmentProvider,
  PromptAttachmentPayload,
  ProtocolReadingGate,
  RunStatus,
} from './types';

export function stateLabel(state: AgentState) {
  if (state === 'ready') return 'Aprovado';
  if (state === 'running') return 'Em andamento';
  if (state === 'evidence') return 'Precisa de revisao';
  return 'Aguardando';
}

export function stateIcon(state: AgentState) {
  if (state === 'ready') return <CheckCircle2 size={16} />;
  if (state === 'running') return <RefreshCw size={16} />;
  if (state === 'evidence') return <Clock3 size={16} />;
  return <AlertTriangle size={16} />;
}

export async function sha256(text: string) {
  const bytes = new TextEncoder().encode(text);
  const buffer = await crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(buffer)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

export function formatElapsedTime(totalSeconds: number) {
  const safeSeconds = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(safeSeconds / 3600);
  const minutes = Math.floor((safeSeconds % 3600) / 60);
  const seconds = safeSeconds % 60;
  return [hours, minutes, seconds].map((value) => value.toString().padStart(2, '0')).join(':');
}

export function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes.toLocaleString('pt-BR')} B`;
  const kib = bytes / 1024;
  if (kib < 1024) return `${kib.toLocaleString('pt-BR', { maximumFractionDigits: 1 })} KiB`;
  const mib = kib / 1024;
  return `${mib.toLocaleString('pt-BR', { maximumFractionDigits: 1 })} MiB`;
}

export function normalizedAttachmentMediaType(attachment: PromptAttachmentPayload) {
  const media = attachment.media_type?.trim().toLowerCase();
  if (!media) return 'application/octet-stream';
  if (media === 'image/jpg') return 'image/jpeg';
  return media;
}

export function attachmentExtension(name: string) {
  const lastSegment = name.split(/[\\/]/).pop() ?? name;
  const index = lastSegment.lastIndexOf('.');
  if (index < 0 || index === lastSegment.length - 1) return '';
  return lastSegment.slice(index + 1).toLowerCase();
}

export function isTextLikeAttachment(attachment: PromptAttachmentPayload) {
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

export function isImageAttachment(attachment: PromptAttachmentPayload) {
  return ['image/png', 'image/jpeg', 'image/webp', 'image/gif'].includes(normalizedAttachmentMediaType(attachment));
}

export function isPdfAttachment(attachment: PromptAttachmentPayload) {
  return normalizedAttachmentMediaType(attachment) === 'application/pdf' || attachmentExtension(attachment.name) === 'pdf';
}

export function isKnownDocumentAttachment(attachment: PromptAttachmentPayload) {
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
export function providerSupportsNativeAttachment(provider: NativeAttachmentProvider, attachment: PromptAttachmentPayload) {
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

export function attachmentDeliveryPlan(
  attachment: PromptAttachmentPayload,
  activeApiProviders: AiCredentialKey[],
): AttachmentDeliveryPlan {
  const nativeProviders = activeApiProviders.filter(
    (provider): provider is NativeAttachmentProvider =>
      provider !== 'deepseek' && provider !== 'grok' && providerSupportsNativeAttachment(provider, attachment),
  );
  const manifestProviders = activeApiProviders.filter(
    (provider) =>
      provider === 'deepseek' || provider === 'grok' || !nativeProviders.includes(provider as NativeAttachmentProvider),
  );
  let fallbackReason: string | null = null;
  if (manifestProviders.length > 0 || nativeProviders.length === 0) {
    fallbackReason =
      attachment.size_bytes > attachmentLimits.maxNativeApiBytes
        ? `excede envio nativo (${formatBytes(attachmentLimits.maxNativeApiBytes)})`
        : activeApiProviders.length === 0
          ? 'peers ativos usam CLI'
          : manifestProviders.length > 0 &&
              manifestProviders.every((provider) => provider === 'deepseek' || provider === 'grok')
            ? 'API text-only'
            : nativeProviders.length > 0
              ? 'sem suporte nativo nesses peers'
              : 'tipo sem suporte nativo nos peers API ativos';
  }
  return { attachment, nativeProviders, manifestProviders, fallbackReason };
}

export function providerShortLabel(provider: AiCredentialKey) {
  if (provider === 'openai') return 'OpenAI';
  if (provider === 'anthropic') return 'Anthropic';
  if (provider === 'gemini') return 'Gemini';
  if (provider === 'grok') return 'Grok';
  return 'DeepSeek';
}

export function attachmentDeliveryHint(plan: AttachmentDeliveryPlan) {
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

export function formatBrazilDateTime(value: Date | number) {
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

export function humanizeRunStatus(status: RunStatus) {
  if (status === 'idle') return 'Aguardando';
  if (status === 'preparing') return 'Preparando';
  if (status === 'running') return 'Em andamento';
  if (status === 'paused') return 'Aguardando ajustes';
  if (status === 'completed') return 'Concluido';
  return 'Bloqueado';
}

export function operationMeterLabel(status: RunStatus) {
  if (status === 'running') return 'Em andamento';
  if (status === 'completed') return 'Concluido';
  if (status === 'paused') return 'Aguardando ajustes';
  if (status === 'blocked') return 'Bloqueado';
  if (status === 'preparing') return 'Preparando';
  return 'Aguardando';
}

export function humanizeAgentStatus(status: string) {
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
  if (normalized === 'PAUSED_COST_LIMIT_REQUIRED') return 'Limite de custo necessario';
  if (normalized === 'PAUSED_COST_RATES_MISSING') return 'Tarifas de custo ausentes';
  if (normalized.startsWith('PROVIDER_INCOMPLETE_RESPONSE')) return 'Resposta incompleta do provedor';
  if (normalized.startsWith('PROVIDER_EMPTY_CONTENT')) return 'Resposta vazia do provedor';
  if (normalized === 'PAUSED_DRAFT_AUTHOR_UNKNOWN') return 'Autor do rascunho nao identificado';
  if (normalized === 'PAUSED_REVIEWERS_UNAVAILABLE') return 'Sem revisor independente';
  if (normalized === 'PAUSED_REVIEWER_OPERATIONAL_OUTAGE') return 'Revisores indisponiveis';
  if (normalized === 'ALL_PEERS_FAILING') return 'Todos os peers em erro';
  if (normalized === 'PAUSED_WITH_REAL_AGENT_OUTPUTS') return 'Aguardando ajustes';
  return status
    .replace(/_/g, ' ')
    .toLowerCase()
    .replace(/(^|\s)\S/g, (value) => value.toUpperCase());
}

export function humanizeRole(role: string) {
  if (role === 'draft') return 'Rascunho';
  if (role === 'revision') return 'Ajuste';
  if (role === 'review') return 'Revisao';
  return 'Atividade';
}

export function agentStateFromTone(tone: EditorialAgentResult['tone']): AgentState {
  if (tone === 'ok') return 'ready';
  if (tone === 'warn') return 'evidence';
  return 'blocked';
}

export function agentResultRank(agent: EditorialAgentResult) {
  const match = agent.output_path.match(/round-(\d{3})-/i);
  const round = match ? Number.parseInt(match[1], 10) : 0;
  const roleRank = agent.role === 'review' ? 3 : agent.role === 'revision' ? 2 : agent.role === 'draft' ? 1 : 0;
  return round * 10 + roleRank;
}

export function latestAgentResults(agents: EditorialAgentResult[]) {
  const byName = new Map<string, EditorialAgentResult>();
  for (const agent of agents) {
    const current = byName.get(agent.name);
    if (!current || agentResultRank(agent) >= agentResultRank(current)) {
      byName.set(agent.name, agent);
    }
  }
  return ['Claude', 'Codex', 'Gemini', 'DeepSeek', 'Grok']
    .map((name) => byName.get(name))
    .filter((agent): agent is EditorialAgentResult => Boolean(agent));
}

export function latestAgentCards(agents: EditorialAgentResult[]): AgentCard[] {
  return latestAgentResults(agents).map((agent) => ({
    name: agent.name,
    cli: agent.cli,
    state: agentStateFromTone(agent.tone),
    note: `${humanizeRole(agent.role)}: ${humanizeAgentStatus(agent.status)}; ${formatElapsedTime(
      Math.round(agent.duration_ms / 1000),
    )}`,
  }));
}

export function latestProtocolGateItems(agents: EditorialAgentResult[]): ProtocolReadingGate[] {
  return latestAgentResults(agents).map((agent) => ({
    agent: agent.name,
    progress: agent.tone === 'ok' ? 100 : agent.tone === 'warn' ? 70 : 35,
    status: agent.tone === 'ok' ? 'Protocolo lido na ultima rodada' : humanizeAgentStatus(agent.status),
  }));
}

export function countAgentRounds(agents: EditorialAgentResult[]) {
  return new Set(
    agents
      .map((agent) => agent.output_path.match(/round-(\d{3})-/i)?.[1])
      .filter((round): round is string => Boolean(round)),
  ).size;
}

export function summarizeAgentResults(agents: EditorialAgentResult[]) {
  const rounds = countAgentRounds(agents);
  const latest = latestAgentResults(agents);
  const latestText = latest.map((agent) => `${agent.name}: ${humanizeAgentStatus(agent.status)}`).join('; ');
  const failures = agents.filter((agent) => agent.tone === 'error' || agent.tone === 'blocked').length;
  const failureText = failures
    ? ` ${failures.toLocaleString('pt-BR')} falha(s) operacional(is) registrada(s) no diagnostico.`
    : '';
  return `${rounds.toLocaleString('pt-BR')} rodada(s) registradas. Ultimo estado: ${latestText || 'sem avaliacao registrada'}.${failureText}`;
}
