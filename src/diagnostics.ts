import { invoke } from "@tauri-apps/api/core";

type LogLevel = "debug" | "info" | "warn" | "error" | "fatal";

type LogContext = Record<string, unknown>;

type LogEvent = {
  level: LogLevel;
  category: string;
  message: string;
  context?: LogContext;
};

const frontendBootTime = performance.now();
let frontendLogSequence = 0;
let diagnosticsInstalled = false;
const originalConsole = {
  error: console.error.bind(console),
  warn: console.warn.bind(console),
  debug: console.debug.bind(console),
};
const sensitiveKeyPattern = /(secret|token|password|credential|api[_-]?key|auth|cookie|private)/i;
const safeDiagnosticKeyPattern =
  /(_present|_source|_scope|_env_var|_env_scope|_mode|_label|_name|_status|_tone|_kind|_prefix)$/i;
const privateBlockMarker = "-".repeat(5) + "BEGIN";
const secretValuePattern = new RegExp(
  [
    "sk-[A-Za-z0-9_-]{8,}",
    "sk-ant-[A-Za-z0-9_-]{8,}",
    "sk_live_[A-Za-z0-9_-]{8,}",
    "pplx-[A-Za-z0-9_-]{8,}",
    "cfut_[A-Za-z0-9_-]{8,}",
    "cfat_[A-Za-z0-9_-]{8,}",
    "cfk_[A-Za-z0-9_-]{8,}",
    "xox[baprs]-[A-Za-z0-9-]{8,}",
    "gh[pousr]_[A-Za-z0-9_]{8,}",
    "AI" + "za[0-9A-Za-z_-]{8,}",
    "re_[A-Za-z0-9_-]{20,}",
    "AKIA[0-9A-Z]{16}",
    privateBlockMarker,
  ].join("|"),
);

function shouldRedactKey(key: string) {
  const normalized = key.toLowerCase();
  if (normalized === "credential_storage_mode") return false;
  if (normalized === "cloudflare_api_token_source") return false;
  if (normalized === "cloudflare_api_token_env_var") return false;
  if (normalized === "cloudflare_api_token_env_scope") return false;
  if (normalized === "cloudflare_api_token_present") return false;
  if (normalized === "token_source") return false;
  if (normalized === "token_env_var") return false;
  if (normalized === "token_present") return false;
  if (normalized === "secret_store") return false;
  if (safeDiagnosticKeyPattern.test(normalized)) return false;
  return sensitiveKeyPattern.test(normalized);
}

function sanitize(value: unknown, depth = 0): unknown {
  if (depth > 7) return "<max_depth_reached>";
  if (typeof value === "string")
    return secretValuePattern.test(value) ? "<redacted>" : value.slice(0, 1600);
  if (
    typeof value === "number" ||
    typeof value === "boolean" ||
    value === null ||
    value === undefined
  )
    return value;
  if (value instanceof Error) {
    return {
      name: value.name,
      message: sanitize(value.message, depth + 1),
      stack: sanitize(value.stack ?? "", depth + 1),
    };
  }
  if (Array.isArray(value)) return value.slice(0, 80).map((item) => sanitize(item, depth + 1));
  if (typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>)
        .slice(0, 120)
        .map(([key, item]) => [
          key,
          shouldRedactKey(key) ? "<redacted>" : sanitize(item, depth + 1),
        ]),
    );
  }
  return String(value);
}

function activeElementSnapshot() {
  const element = document.activeElement;
  if (!element) return null;
  return {
    tag: element.tagName,
    id: element.id || null,
    class_name: typeof element.className === "string" ? element.className.slice(0, 120) : null,
    aria_label: element.getAttribute("aria-label"),
  };
}

function runtimeContext() {
  const navigatorWithHints = navigator as Navigator & {
    deviceMemory?: number;
    connection?: {
      effectiveType?: string;
      downlink?: number;
      rtt?: number;
      saveData?: boolean;
    };
  };

  return {
    frontend_log_sequence: ++frontendLogSequence,
    performance_ms: Math.round(performance.now() - frontendBootTime),
    url: window.location.href,
    path: window.location.pathname,
    hash: window.location.hash,
    visibility_state: document.visibilityState,
    online: navigator.onLine,
    language: navigator.language,
    platform: navigator.platform,
    hardware_concurrency: navigator.hardwareConcurrency,
    device_memory_gb: navigatorWithHints.deviceMemory ?? null,
    connection: navigatorWithHints.connection
      ? {
          effective_type: navigatorWithHints.connection.effectiveType ?? null,
          downlink: navigatorWithHints.connection.downlink ?? null,
          rtt: navigatorWithHints.connection.rtt ?? null,
          save_data: navigatorWithHints.connection.saveData ?? null,
        }
      : null,
    color_scheme: window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light",
    device_pixel_ratio: window.devicePixelRatio,
    active_element: activeElementSnapshot(),
    user_agent: navigator.userAgent,
    viewport: {
      width: window.innerWidth,
      height: window.innerHeight,
    },
    screen: {
      width: window.screen.width,
      height: window.screen.height,
      avail_width: window.screen.availWidth,
      avail_height: window.screen.availHeight,
    },
  };
}

export async function logEvent(event: LogEvent) {
  const payload = {
    ...event,
    context: sanitize({
      ...event.context,
      runtime: runtimeContext(),
    }),
  };

  try {
    await invoke("write_log_event", { event: payload });
  } catch (error) {
    const logger =
      event.level === "error" || event.level === "fatal"
        ? originalConsole.error
        : originalConsole.debug;
    logger("diagnostic log fallback", {
      event: payload,
      error,
    });
  }
}

export function installGlobalDiagnostics() {
  if (diagnosticsInstalled) return;
  diagnosticsInstalled = true;

  void logEvent({
    level: "info",
    category: "app.lifecycle",
    message: "frontend runtime booted with expanded diagnostics",
    context: {
      app: "Maestro Editorial AI",
      log_policy: "one_ndjson_file_per_app_execution",
      capture: [
        "ui_events",
        "frontend_errors",
        "unhandled_rejections",
        "console_warn_error",
        "network_state",
      ],
    },
  });

  console.warn = (...args: unknown[]) => {
    originalConsole.warn(...args);
    void logEvent({
      level: "warn",
      category: "frontend.console.warn",
      message: "console.warn captured",
      context: { args },
    });
  };

  console.error = (...args: unknown[]) => {
    originalConsole.error(...args);
    void logEvent({
      level: "error",
      category: "frontend.console.error",
      message: "console.error captured",
      context: { args },
    });
  };

  window.addEventListener("error", (event) => {
    void logEvent({
      level: "error",
      category: "frontend.error",
      message: event.message || "uncaught frontend error",
      context: {
        filename: event.filename,
        lineno: event.lineno,
        colno: event.colno,
        error: event.error,
      },
    });
  });

  window.addEventListener("unhandledrejection", (event) => {
    void logEvent({
      level: "error",
      category: "frontend.unhandled_rejection",
      message: "unhandled promise rejection",
      context: { reason: event.reason },
    });
  });

  window.addEventListener("online", () => {
    void logEvent({
      level: "info",
      category: "frontend.network.online",
      message: "browser network changed to online",
    });
  });

  window.addEventListener("offline", () => {
    void logEvent({
      level: "warn",
      category: "frontend.network.offline",
      message: "browser network changed to offline",
    });
  });

  document.addEventListener("visibilitychange", () => {
    void logEvent({
      level: "info",
      category: "frontend.visibility.changed",
      message: "document visibility changed",
      context: { visibility_state: document.visibilityState },
    });
  });

  window.addEventListener("beforeunload", () => {
    void logEvent({
      level: "info",
      category: "app.lifecycle",
      message: "frontend beforeunload fired",
    });
  });
}
