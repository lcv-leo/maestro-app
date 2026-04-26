import { invoke } from '@tauri-apps/api/core';

type LogLevel = 'debug' | 'info' | 'warn' | 'error' | 'fatal';

type LogContext = Record<string, unknown>;

type LogEvent = {
  level: LogLevel;
  category: string;
  message: string;
  context?: LogContext;
};

const secretKeyPattern = /(secret|token|password|credential|api[_-]?key|auth)/i;
const privateBlockMarker = '-'.repeat(5) + 'BEGIN';
const secretValuePattern = new RegExp(
  [
    'sk-[A-Za-z0-9_-]{8,}',
    'sk-ant-[A-Za-z0-9_-]{8,}',
    'sk_live_[A-Za-z0-9_-]{8,}',
    'cfut_[A-Za-z0-9_-]{8,}',
    'xox[baprs]-[A-Za-z0-9-]{8,}',
    'gh[pousr]_[A-Za-z0-9_]{8,}',
    'AI' + 'za[0-9A-Za-z_-]{8,}',
    're_[A-Za-z0-9_-]{20,}',
    'AKIA[0-9A-Z]{16}',
    privateBlockMarker,
  ].join('|'),
);

function sanitize(value: unknown, depth = 0): unknown {
  if (depth > 7) return '<max_depth_reached>';
  if (typeof value === 'string') return secretValuePattern.test(value) ? '<redacted>' : value.slice(0, 1600);
  if (typeof value === 'number' || typeof value === 'boolean' || value === null || value === undefined) return value;
  if (value instanceof Error) {
    return {
      name: value.name,
      message: sanitize(value.message, depth + 1),
      stack: sanitize(value.stack ?? '', depth + 1),
    };
  }
  if (Array.isArray(value)) return value.slice(0, 80).map((item) => sanitize(item, depth + 1));
  if (typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>)
        .slice(0, 120)
        .map(([key, item]) => [key, secretKeyPattern.test(key) ? '<redacted>' : sanitize(item, depth + 1)]),
    );
  }
  return String(value);
}

export async function logEvent(event: LogEvent) {
  const payload = {
    ...event,
    context: sanitize({
      ...event.context,
      url: window.location.href,
      user_agent: navigator.userAgent,
      viewport: {
        width: window.innerWidth,
        height: window.innerHeight,
      },
    }),
  };

  try {
    await invoke('write_log_event', { event: payload });
  } catch (error) {
    console[event.level === 'error' || event.level === 'fatal' ? 'error' : 'debug']('diagnostic log fallback', {
      event: payload,
      error,
    });
  }
}

export function installGlobalDiagnostics() {
  void logEvent({
    level: 'info',
    category: 'app.lifecycle',
    message: 'frontend runtime booted',
    context: { app: 'Maestro Editorial AI' },
  });

  window.addEventListener('error', (event) => {
    void logEvent({
      level: 'error',
      category: 'frontend.error',
      message: event.message || 'uncaught frontend error',
      context: {
        filename: event.filename,
        lineno: event.lineno,
        colno: event.colno,
        error: event.error,
      },
    });
  });

  window.addEventListener('unhandledrejection', (event) => {
    void logEvent({
      level: 'error',
      category: 'frontend.unhandled_rejection',
      message: 'unhandled promise rejection',
      context: { reason: event.reason },
    });
  });
}
