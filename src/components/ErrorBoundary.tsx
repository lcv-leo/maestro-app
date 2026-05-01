/*
 * Copyright (C) 2026 Leonardo Cardozo Vargas
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

import { Component, type ErrorInfo, type ReactNode } from 'react';
import { logEvent } from '../diagnostics';

/**
 * v0.3.14 / maestro-app audit closure (HIGH): top-level Error Boundary.
 *
 * Pre-fix, maestro-app had NO render-phase error containment. Why this matters
 * even though `installGlobalDiagnostics()` already exists:
 *   - The `window.error` and `unhandledrejection` listeners installed by
 *     `installGlobalDiagnostics` (diagnostics.ts) only catch exceptions that
 *     bubble OUTSIDE React's reconciler (event handlers, async tasks).
 *   - Render-phase exceptions are swallowed by React, which silently unmounts
 *     the offending subtree. The diagnostic listeners never fire for those.
 *   - Without a boundary, any throw inside JSX rendering, useState/useMemo
 *     selectors, or component initialization can blank the whole webview
 *     until the operator manually restarts the executable.
 *
 * The boundary is strictly additive: it does NOT replace `installGlobalDiagnostics`
 * (those listeners still catch async/event errors). It catches the one class
 * the diagnostic listeners can't reach — render exceptions — and forwards the
 * captured payload to the SAME `logEvent('error', ...)` channel so the NDJSON
 * log keeps a single audit trail.
 *
 * React 19 still requires a class component for `componentDidCatch`. Mirrors
 * the boundary shipped in admin-app v02.00.00 and mainsite-frontend v03.22.00,
 * adapted for the desktop context (no `window.dataLayer` — Tauri webview never
 * has a GTM pixel; logEvent → backend NDJSON log is the canonical telemetry
 * path).
 */

interface ErrorBoundaryProps {
  children: ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { hasError: false, error: null };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    // Forward the render-phase exception to the same NDJSON log channel
    // installGlobalDiagnostics uses, so the operator's diagnostic timeline
    // contains the failure regardless of which listener actually catches it.
    void logEvent({
      level: 'error',
      category: 'react.render',
      message: 'ErrorBoundary caught render-phase exception',
      context: {
        error_message: error.message,
        error_name: error.name,
        component_stack: info.componentStack ?? null,
      },
    });
  }

  private handleReload = (): void => {
    window.location.reload();
  };

  render(): ReactNode {
    if (!this.state.hasError) return this.props.children;
    return (
      <div
        role="alert"
        aria-live="assertive"
        style={{
          minHeight: '100vh',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          padding: '24px',
          backgroundColor: '#0b0b0d',
          color: '#f5f5f5',
          fontFamily: 'system-ui, sans-serif',
        }}
      >
        <div style={{ maxWidth: 480, textAlign: 'center' }}>
          <h1 style={{ fontSize: 20, fontWeight: 700, marginBottom: 12 }}>
            Algo deu errado no Maestro.
          </h1>
          <p style={{ fontSize: 14, opacity: 0.8, marginBottom: 20 }}>
            Um erro inesperado interrompeu a renderização. Recarregar normalmente
            recupera o estado da sessão. O erro foi gravado no log NDJSON local.
          </p>
          <button
            type="button"
            onClick={this.handleReload}
            style={{
              padding: '10px 20px',
              borderRadius: 6,
              border: '1px solid #444',
              backgroundColor: '#1a1a1d',
              color: '#f5f5f5',
              cursor: 'pointer',
              fontSize: 14,
              fontWeight: 600,
            }}
          >
            Recarregar
          </button>
        </div>
      </div>
    );
  }
}

export default ErrorBoundary;
