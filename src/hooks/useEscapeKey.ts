/*
 * Copyright (C) 2026 Leonardo Cardozo Vargas
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

import { useEffect } from "react";

/**
 * v0.3.14 / maestro-app audit closure (MEDIUM): centralized ESC-key handler
 * for custom dialogs that aren't built on a UI library with native ESC support.
 *
 * Pre-fix, two of maestro-app's dialogs lacked ESC dismissal: the editor
 * `PromptModal` (`src/editor/posteditor/editor/PromptModal.tsx`) — used for
 * link/image URL/YouTube/caption/Gemini-import inputs — and the editorial
 * `ResumeDialog` block embedded in `App.tsx` (lines 2574-2628). Both already
 * had explicit Close buttons, so ESC just mirrors that path; no new dismissal
 * semantics, no UX-intent change. Closes the WCAG 2.1 AA gap for keyboard-only
 * operators.
 *
 * Same shape as the hook shipped in admin-app v02.00.00 and mainsite-frontend
 * v03.22.00 (verbatim port). Each consumer calls with `enabled` tied to the
 * dialog's visibility flag so the listener auto-detaches when the dialog
 * closes; the hook must be called BEFORE any early `return null` in the
 * component to satisfy Rules of Hooks.
 */
export function useEscapeKey(onEscape: () => void, enabled: boolean): void {
  useEffect(() => {
    if (!enabled) return;
    const handler = (event: KeyboardEvent): void => {
      if (event.key === "Escape" || event.key === "Esc") {
        onEscape();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onEscape, enabled]);
}
