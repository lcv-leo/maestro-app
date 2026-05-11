import { describe, expect, it } from 'vitest';

import { convertMarkdownToFormattedHtml } from './markdownImport';

describe('convertMarkdownToFormattedHtml', () => {
  it('extracts the first H1 as title and sanitizes imported HTML', () => {
    const result = convertMarkdownToFormattedHtml(`---
private: true
---

# Public Title

Plain paragraph.

<img src="x" onerror="alert(1)">
<script>alert('xss')</script>
<a href="javascript:alert(1)">unsafe</a>
`);

    expect(result.title).toBe('Public Title');
    expect(result.html).not.toContain('<script');
    expect(result.html).not.toContain('onerror');
    expect(result.html).not.toContain('javascript:');
    expect(result.html).toContain('text-align: justify');
  });
});
