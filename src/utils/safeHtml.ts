/**
 * Centralised HTML sanitization for every `v-html` site in the app.
 *
 * Why this exists
 * ---------------
 * LovelyCarve renders **attacker-controlled data** (webshell traffic,
 * SQL-injection payloads, raw log lines) into the DOM. Before this file,
 * each `v-html` consumer hand-rolled its own escaper / highlighter and
 * trusted itself to be airtight. That's fragile: any new `v-html`
 * binding that forgets the escape ships an XSS.
 *
 * The branded `SafeHtmlString` type makes "this string has been
 * sanitised" a compile-time property instead of a code-review one. A
 * future ESLint rule can reject `v-html` bindings whose expression is
 * not of type `SafeHtmlString`.
 *
 * What's safe
 * -----------
 * - `escapeHtml(text)` — escapes `& < > " '` and returns SafeHtmlString.
 *   Use for raw user text where no markup is wanted.
 * - `sanitizeHtml(html)` — parses the input with DOMParser and emits a
 *   tree containing only tags/attributes in the allowlist below. Unsafe
 *   nodes are dropped; their text content survives.
 * - `composeSafeHtml(parts)` — concatenates already-SafeHtmlString
 *   fragments without re-escaping. For builder patterns like
 *   "escape user text, then wrap in <span> with a known-safe class".
 * - `markSafeFromTrustedSource(html)` — the explicit escape hatch.
 *   Only call this when the input is a *string literal you wrote in
 *   source*, never on dynamic data. Search for callers in code review.
 *
 * Allowlist policy
 * ----------------
 * Tags: structural inline (`span`, `em`, `strong`, `b`, `i`, `u`,
 * `code`, `mark`, `br`), block (`p`, `div`, `pre`, `blockquote`,
 * `ul`, `ol`, `li`, `hr`), and `a`. No `script`, `style`, `iframe`,
 * `object`, `embed`, `form`, `input`, `link`, `meta`, `svg`, `math`,
 * event handlers, or `style=` attributes.
 *
 * `a[href]` is restricted to `https:` / `mailto:` schemes (no
 * `javascript:`, no `data:`, no protocol-relative `//host/...`). All
 * links get `rel="noopener noreferrer"` + `target="_blank"` forced on,
 * regardless of what the input said.
 */

declare const __safeHtmlBrand: unique symbol;

/**
 * A string that has been audited by this module and is safe to feed to
 * `v-html`. The brand is a phantom type — at runtime this is just a
 * `string`. Don't `as SafeHtmlString` cast; use one of the constructors.
 */
export type SafeHtmlString = string & { readonly [__safeHtmlBrand]: true };

const ALLOWED_TAGS = new Set([
  'a', 'b', 'blockquote', 'br', 'code', 'div', 'em', 'hr', 'i', 'li',
  'mark', 'ol', 'p', 'pre', 'span', 'strong', 'u', 'ul',
]);

/** Per-tag attribute allowlist. Anything else is dropped. */
const ALLOWED_ATTRS_BY_TAG: Record<string, ReadonlySet<string>> = {
  a: new Set(['href', 'title', 'class']),
  span: new Set(['class', 'title']),
  div: new Set(['class']),
  pre: new Set(['class']),
  code: new Set(['class']),
  mark: new Set(['class']),
  p: new Set(['class']),
  ul: new Set(['class']),
  ol: new Set(['class']),
  li: new Set(['class']),
  blockquote: new Set(['class']),
  br: new Set([]),
  hr: new Set([]),
  em: new Set(['class']),
  strong: new Set(['class']),
  b: new Set(['class']),
  i: new Set(['class']),
  u: new Set(['class']),
};

/** Trim, lowercase, and reject anything that isn't an explicit allow. */
function sanitizeHref(href: string): string | null {
  const trimmed = href.trim();
  if (!trimmed) return null;
  // Reject protocol-relative `//evil.example/x` — those inherit the
  // current scheme and bypass naïve "starts with javascript:" checks.
  if (trimmed.startsWith('//')) return null;
  const lower = trimmed.toLowerCase();
  if (lower.startsWith('https://') || lower.startsWith('mailto:')) {
    return trimmed;
  }
  return null;
}

/**
 * Escape the five HTML metacharacters (`& < > " '`). Returns a
 * SafeHtmlString suitable for `v-html` (though if there is no
 * highlighting to apply, prefer `{{ text }}` mustache interpolation
 * which is just as safe and avoids the v-html attack surface entirely).
 */
export function escapeHtml(value: string): SafeHtmlString {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;') as SafeHtmlString;
}

/**
 * Concatenate several already-safe fragments. The result is itself
 * SafeHtmlString; no additional escaping is performed (so make sure
 * every argument was produced by one of this module's constructors).
 */
export function composeSafeHtml(...parts: SafeHtmlString[]): SafeHtmlString {
  return parts.join('') as SafeHtmlString;
}

/**
 * Escape hatch for static, code-author-controlled HTML literals (e.g.
 * a hard-coded SVG icon or a `<br>`). Never call with attacker data —
 * the whole point of `SafeHtmlString` is that calling this is a code
 * smell that shows up in review.
 */
export function markSafeFromTrustedSource(html: string): SafeHtmlString {
  return html as SafeHtmlString;
}

/**
 * Parse the input as HTML and emit a sanitised string containing only
 * allowlisted tags / attributes. Unsafe nodes are dropped (their text
 * content survives — `<script>alert(1)</script>` becomes `alert(1)`).
 *
 * Uses the browser's DOMParser, so this only works in a browser /
 * webview context. In Vitest with happy-dom it works because happy-dom
 * provides DOMParser; in Node without a DOM environment this throws.
 */
export function sanitizeHtml(html: string): SafeHtmlString {
  if (!html) return '' as SafeHtmlString;
  const parser = new DOMParser();
  // Wrap in a sandbox element to avoid pollution of document title /
  // metadata-style nodes promoted to head by the parser.
  const doc = parser.parseFromString(`<div id="__sb">${html}</div>`, 'text/html');
  const root = doc.getElementById('__sb');
  if (!root) return '' as SafeHtmlString;
  walkAndScrub(root);
  return root.innerHTML as SafeHtmlString;
}

/**
 * Recursively scrub an element subtree in place. Disallowed elements
 * are replaced by their text content; disallowed attributes are
 * removed; `a[href]` is rewritten with sanitised URL + forced rel/target.
 */
function walkAndScrub(node: Element): void {
  const children = Array.from(node.children);
  for (const child of children) {
    const tag = child.tagName.toLowerCase();
    if (!ALLOWED_TAGS.has(tag)) {
      // Replace the disallowed element with a text node holding its
      // text content. Strips event-handler vectors entirely while
      // preserving the user-visible text inside, which matches what a
      // careful operator would want when staring at webshell payloads.
      const textNode = child.ownerDocument!.createTextNode(child.textContent ?? '');
      child.replaceWith(textNode);
      continue;
    }

    const allowed = ALLOWED_ATTRS_BY_TAG[tag] ?? new Set<string>();
    // Iterate over a snapshot — `removeAttribute` mutates the live list.
    for (const attr of Array.from(child.attributes)) {
      const name = attr.name.toLowerCase();
      // Drop every `on*` handler and any attribute starting with `xmlns`
      // (foreign namespace injection) regardless of the per-tag list.
      if (name.startsWith('on') || name.startsWith('xmlns')) {
        child.removeAttribute(attr.name);
        continue;
      }
      if (!allowed.has(name)) {
        child.removeAttribute(attr.name);
        continue;
      }
      if (tag === 'a' && name === 'href') {
        const safe = sanitizeHref(attr.value);
        if (safe === null) {
          child.removeAttribute('href');
        } else {
          child.setAttribute('href', safe);
        }
      }
      // Class allowlisting could go here. We currently let classes
      // through unfiltered because most callers pass values they
      // generated themselves (`payload-keyword`, `seg-meta`, …). If a
      // future caller starts forwarding user-controlled classes, add
      // a class-name allowlist here.
    }

    if (tag === 'a') {
      // Force safe link behaviour, regardless of what the input said.
      // `noopener` blocks window.opener tampering, `noreferrer` plugs
      // the referrer leak, and the explicit target keeps the app
      // window intact even if the input forgot `target=`.
      child.setAttribute('rel', 'noopener noreferrer');
      child.setAttribute('target', '_blank');
    }

    walkAndScrub(child);
  }
}
