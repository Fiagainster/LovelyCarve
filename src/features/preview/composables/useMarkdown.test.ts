import { describe, expect, it } from 'vitest';
import { renderMarkdownPreview, renderMarkdownToHtml } from './useMarkdown';

describe('renderMarkdownToHtml', () => {
  it('escapes HTML in plain text', () => {
    const out = renderMarkdownToHtml('<script>alert(1)</script>');
    expect(out).not.toContain('<script>');
    expect(out).toContain('&lt;script&gt;');
  });

  it('renders bold and italic', () => {
    expect(renderMarkdownToHtml('**bold** and *italic*'))
      .toContain('<strong>bold</strong>');
    expect(renderMarkdownToHtml('**bold** and *italic*'))
      .toContain('<em>italic</em>');
  });

  it('renders inline code without re-applying inline transforms', () => {
    const out = renderMarkdownToHtml('try `**not bold**` here');
    expect(out).toContain('<code class="md-inline-code">**not bold**</code>');
    expect(out).not.toContain('<strong>not bold</strong>');
  });

  it('escapes inline code exactly once', () => {
    const out = renderMarkdownToHtml('use `a<b && c` here');
    expect(out).toContain('<code class="md-inline-code">a&lt;b &amp;&amp; c</code>');
  });

  it('renders fenced code blocks with language class', () => {
    const out = renderMarkdownToHtml('```python\nprint("hi")\n```');
    expect(out).toContain('class="md-code-block language-python"');
    expect(out).toContain('data-lang="python"');
    expect(out).toContain('print(&quot;hi&quot;)');
  });

  it('renders headings up to six levels', () => {
    expect(renderMarkdownToHtml('# H1')).toContain('<h1 class="md-heading md-h1">H1</h1>');
    expect(renderMarkdownToHtml('###### H6')).toContain('<h6 class="md-heading md-h6">H6</h6>');
  });

  it('renders unordered and ordered lists', () => {
    const ul = renderMarkdownToHtml('- a\n- b\n- c');
    expect(ul).toContain('<ul class="md-list">');
    expect((ul.match(/<li>/g) ?? []).length).toBe(3);

    const ol = renderMarkdownToHtml('1. one\n2. two');
    expect(ol).toContain('<ol class="md-list">');
    expect(ol).toContain('<li>one</li>');
  });

  it('renders blockquote and horizontal rule', () => {
    expect(renderMarkdownToHtml('> quoted')).toContain('<blockquote class="md-quote">quoted</blockquote>');
    expect(renderMarkdownToHtml('---')).toContain('<hr class="md-hr"');
  });

  it('renders only safe links', () => {
    const ok = renderMarkdownToHtml('[Anthropic](https://anthropic.com)');
    expect(ok).toContain('href="https://anthropic.com"');
    expect(ok).toContain('rel="noopener noreferrer"');

    const evil = renderMarkdownToHtml('[click](javascript:alert(1))');
    // Disallowed schemes must never produce an actual <a> tag; the literal
    // markdown source falls through as escaped text instead.
    expect(evil).not.toContain('<a ');
    expect(evil).not.toContain('href=');
    expect(evil).toContain('[click](javascript:alert(1))');
  });

  it('lets unterminated fenced code blocks still render', () => {
    const out = renderMarkdownToHtml('```\nstill open');
    expect(out).toContain('md-code-block');
    expect(out).toContain('still open');
  });

  it('renders a GFM table with header, separator, and body rows', () => {
    const src = [
      '| # | 问题 | 答案 |',
      '|---|------|------|',
      '| 1 | SSID | laozhaoWIFI |',
      '| 2 | 握手组数 | 4 |',
    ].join('\n');
    const out = renderMarkdownToHtml(src);
    expect(out).toContain('<table class="md-table">');
    expect(out).toContain('<thead>');
    expect(out).toContain('<th>#</th>');
    expect(out).toContain('<th>问题</th>');
    expect(out).toContain('<td>SSID</td>');
    expect(out).toContain('<td>laozhaoWIFI</td>');
    // The raw pipes must not survive into the rendered HTML.
    expect(out).not.toContain('| # | 问题 |');
  });

  it('honours alignment markers in the separator row', () => {
    const out = renderMarkdownToHtml(
      ['| L | C | R |', '|:--|:-:|--:|', '| 1 | 2 | 3 |'].join('\n'),
    );
    expect(out).toContain('text-align:left');
    expect(out).toContain('text-align:center');
    expect(out).toContain('text-align:right');
  });

  it('runs inline markdown inside table cells', () => {
    const out = renderMarkdownToHtml(
      ['| key | value |', '|---|---|', '| **bold** | `code` |'].join('\n'),
    );
    expect(out).toContain('<strong>bold</strong>');
    expect(out).toContain('<code class="md-inline-code">code</code>');
  });

  it('does NOT treat a stray pipe line as a table (no separator row)', () => {
    // Regression: any prose line with a `|` used to derail downstream
    // parsing if we got the table heuristic wrong. Without a separator
    // row immediately after, this must fall through as a paragraph.
    const out = renderMarkdownToHtml('this | that');
    expect(out).not.toContain('<table');
    expect(out).toContain('<p>this | that</p>');
  });
});

describe('renderMarkdownPreview wrapper', () => {
  it('returns empty string for non-string input', () => {
    expect(renderMarkdownPreview(null)).toBe('');
    expect(renderMarkdownPreview(undefined)).toBe('');
    expect(renderMarkdownPreview(42)).toBe('');
  });

  it('caps absurdly long inputs and announces the truncation', () => {
    const huge = 'x'.repeat(100_000);
    const out = renderMarkdownPreview(huge, 1_000);
    expect(out.length).toBeLessThan(2_500);
    expect(out).toContain('output capped at');
  });
});
