# PostEditor Parity Module

This folder is the Maestro-local compatibility copy of `admin-app/MainSite/PostEditor`.

Hard rule: if Maestro uses TipTap for MainSite content, it must match the PostEditor feature set and persisted HTML contract, not merely use the same editor framework.

## Parity Requirements

- Keep the TipTap extension set aligned with `admin-app/src/modules/mainsite/editor/extensions.ts`.
- Keep Markdown import behavior aligned with `admin-app/src/modules/mainsite/editor/markdownImport.ts`.
- Keep link normalization aligned with PostEditor save behavior.
- Keep figure, image, YouTube, table, task-list, mention, search/replace, slash-command, AI action, and import affordances available.
- Validate generated HTML against the MainSite sanitizer and `mainsite-frontend/PostReader` before enabling direct D1 publish as stable.

## Drift Policy

When `admin-app/MainSite/PostEditor` changes, Maestro must receive the equivalent change or explicitly record why the behavior does not apply. A planned parity check should compare this folder against a reviewed admin-source snapshot and fail CI on unreviewed drift once the public repo has access to that baseline.

## Source Snapshot

Snapshot imported on 2026-04-26 from the local `admin-app/src/modules/mainsite/` tree.

```text
08c1f8507259c400dce1ada293d82bce0ea499ef1054c29f3984f0ea5cb36300  src/editor/posteditor/PostEditor.tsx
817cafe0c2fd8a0ef51e330d265290872e3f661899a6b326ace92803e54bd89e  src/editor/posteditor/editor/BubbleMenu.tsx
85acc5c9391095354f4f1c3419aee6c2224b36f2aaf96a4dd154ee92a6993310  src/editor/posteditor/editor/extensions.ts
1bc55fa2b63dd819ab89d6110a83b56deaa4c4c69601b13505f8533889da24d8  src/editor/posteditor/editor/FloatingMenu.tsx
de17662994ee2b0478e093e814b6ca8b5a0dcf8d4ca718d56e82f1d599181757  src/editor/posteditor/editor/markdownImport.ts
33de784400b5d03275e0fb0700f522f15163e81b367b3cb0769bbdf99560ed46  src/editor/posteditor/editor/NodeViews.tsx
e92ad9b3f0a5632c5acd862873d28a119d43dd4bb8cd96abbecf4a6d4701d16f  src/editor/posteditor/editor/PromptModal.tsx
3c57a8b2b63462cdd36a049645595ccddbfc61de069bccc25fd55e542febaddc  src/editor/posteditor/editor/promptModalState.ts
82a1e3631bac115ad805b343ffd2cf596b66ad2d8355c841ca20c20ffa798afd  src/editor/posteditor/editor/SearchReplace.tsx
2f6778f07c709c93e98386e500356612ec0f6870346707de642b3f479e50e7d1  src/editor/posteditor/editor/searchReplaceCore.ts
494f1a3362435dc4f501f837824aade51cb45f93c22700733d123e4d4a36c393  src/editor/posteditor/editor/SlashCommands.ts
718815e8300b16e0a965570dcc9f0a3da1f4d6a8291b18b3f4f15005483c9e53  src/editor/posteditor/editor/utils.ts
```

---

<p align="center"><sub>© LCV Ideas &amp; Software<br>LEONARDO CARDOZO VARGAS TECNOLOGIA DA INFORMACAO LTDA<br>Rua Pais Leme, 215 Conj 1713  - Pinheiros<br>São Paulo - SP<br>CEP 05.424-150<br>CNPJ: 66.584.678/0001-77<br>IM 05.424-150</sub></p>