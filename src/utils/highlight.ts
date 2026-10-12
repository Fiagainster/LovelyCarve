// Keyword-highlight helpers shared by text-style previews.
//
// The same recipe FollowStream.vue uses: HTML-escape the text first, then
// wrap case-insensitive matches of the (also escaped) query in <mark>. By
// escaping before we ever touch the string we never feed raw bytes into the
// DOM — the only markup we introduce is the literal <mark> tags.
import { escapeHtml } from './safeHtml';

/** Escape the regex metacharacters in `s` so a user keyword is matched
 *  literally. */
export function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

export interface HighlightResult {
  /** HTML safe to feed to `v-html`: escaped text with <mark> around matches. */
  html: string;
  /** Number of matches found. */
  count: number;
}

/**
 * HTML-escape `text` and wrap every case-insensitive occurrence of `query`
 * in `<mark>`. Empty / unparseable queries return the escaped text untouched.
 */
export function highlightHtml(text: string, query: string): HighlightResult {
  const escaped = escapeHtml(text);
  if (!query) return { html: escaped, count: 0 };
  // Match against the escaped text, so escape the query the same way first.
  const needle = escapeRegExp(escapeHtml(query));
  try {
    const re = new RegExp(`(${needle})`, 'gi');
    const count = (escaped.match(re) || []).length;
    return { html: escaped.replace(re, '<mark>$1</mark>'), count };
  } catch {
    return { html: escaped, count: 0 };
  }
}

/** Count case-insensitive literal occurrences of `query` in `text`. */
export function countMatches(text: string, query: string): number {
  if (!query || !text) return 0;
  try {
    const re = new RegExp(escapeRegExp(query), 'gi');
    return (text.match(re) || []).length;
  } catch {
    return 0;
  }
}
