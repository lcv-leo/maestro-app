# Integrated Text Editor Decision

Status: binding architecture decision.
Date: 2026-04-26.

## Decision

Maestro uses the same TipTap/ProseMirror editor surface as `admin-app/MainSite/PostEditor` for MainSite-bound content.

This is not a generic TipTap implementation. The accepted target is functional and output parity with PostEditor:

- Same effective TipTap extension set.
- Same Markdown import semantics.
- Same media, table, task list, mention, search/replace, slash command, link, color, spacing, and AI action affordances.
- Same `editor.getHTML()` persistence contract.
- Same pre-save link normalization.
- Same sanitizer and `PostReader` render compatibility before direct D1 publish is treated as stable.

The compatibility copy lives in `src/editor/posteditor/`.

For runtime weight, Maestro must follow the admin-app pattern: the PostEditor parity module is lazy-loaded and rendered only after the operator clicks `Criar Post`. The session dashboard must not import or mount the Tiptap-heavy editor on initial load.

## Why TipTap/ProseMirror

TipTap OSS is MIT-licensed and free. Its React integration officially uses `@tiptap/react`, `@tiptap/pm`, and `@tiptap/starter-kit`. ProseMirror gives schema-level control over documents, which matters because Maestro must emit controlled MainSite HTML rather than arbitrary browser DOM.

The decisive reason is existing production compatibility: `admin-app/MainSite/PostEditor` already uses TipTap and `mainsite-frontend/PostReader` already renders its HTML.

## Alternatives Considered

- Lexical: strong, MIT, accessible, and modern, but would require a new Lexical-to-MainSite/TipTap HTML compatibility layer.
- Slate: flexible and MIT, but too much editor behavior, paste handling, and HTML serialization would need to be custom-built.
- Quill: mature and BSD-licensed, but its Delta model is a second document model and would add conversion risk.
- Milkdown: good Markdown-centered ProseMirror option, but the MainSite source of truth is PostEditor HTML, not Markdown-first documents.
- CKEditor 5: powerful, but GPL/commercial licensing is not the right default for this public AGPL project.
- TinyMCE: powerful, but GPL/commercial/attribution posture is not the right default here.

## Non-Negotiable Gate

No Maestro direct write to `bigdata_db.mainsite_posts` may be labeled stable until parity fixtures prove that Maestro-authored content and PostEditor-authored content render equivalently in `PostReader`.
