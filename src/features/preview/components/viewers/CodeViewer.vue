<template>
  <div ref="hostEl" class="cm-viewer-host"></div>
</template>

<script setup lang="ts">
// Read-only, syntax-highlighted source viewer used by the file detail
// panel's "原类型视图" (native view) for code / text files. Distinct from
// python-ide/CodeMirrorEditor.vue, which is python-only and wired to the
// IDE (run keymap, autocompletion). This one picks a language by filename
// via @codemirror/language-data (each grammar dynamically imported, so
// Vite code-splits them and the initial bundle stays small).
import { onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { EditorState, Compartment, StateField, StateEffect, RangeSetBuilder } from '@codemirror/state';
import { EditorView, lineNumbers, Decoration, ViewPlugin } from '@codemirror/view';
import type { DecorationSet, ViewUpdate } from '@codemirror/view';
import {
  syntaxHighlighting,
  defaultHighlightStyle,
  foldGutter,
  LanguageDescription,
} from '@codemirror/language';
import { oneDark } from '@codemirror/theme-one-dark';
import { languages } from '@codemirror/language-data';

const props = withDefaults(defineProps<{
  code: string;
  /** Filename drives language selection (matched by extension). */
  filename: string;
  /** When the host theme is dark, switch CodeMirror to oneDark. */
  dark?: boolean;
  /** Keyword to highlight (case-insensitive, literal). Empty = no highlight. */
  query?: string;
}>(), {
  dark: false,
  query: '',
});

const hostEl = ref<HTMLDivElement | null>(null);
let view: EditorView | null = null;
const themeCompartment = new Compartment();
const langCompartment = new Compartment();

// ---- Keyword highlight + match navigation -------------------------------
// The full match list (whole doc) and the current match index live in editor
// state so the decoration plugin reacts to them. The component recomputes the
// list whenever the query or doc changes; the parent drives navigation via
// the exposed focusMatch().
interface Match { from: number; to: number; }

const setMatches = StateEffect.define<Match[]>();
const setCurrent = StateEffect.define<number>();

const matchesField = StateField.define<Match[]>({
  create: () => [],
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setMatches)) value = e.value;
    return value;
  },
});
const currentField = StateField.define<number>({
  create: () => -1,
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setCurrent)) value = e.value;
    return value;
  },
});

const matchMark = Decoration.mark({ class: 'cm-kw-match' });
const currentMark = Decoration.mark({ class: 'cm-kw-match cm-kw-current' });

function buildMatches(view: EditorView): DecorationSet {
  const matches = view.state.field(matchesField);
  const current = view.state.field(currentField);
  const builder = new RangeSetBuilder<Decoration>();
  const ranges = view.visibleRanges;
  for (let idx = 0; idx < matches.length; idx += 1) {
    const m = matches[idx];
    // Only decorate matches intersecting the viewport (perf); the current
    // match is scrolled into view first, so it will be visible when drawn.
    if (ranges.every((r) => m.to <= r.from || m.from >= r.to)) continue;
    builder.add(m.from, m.to, idx === current ? currentMark : matchMark);
  }
  return builder.finish();
}

const matchHighlighter = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = buildMatches(view);
    }
    update(u: ViewUpdate) {
      if (
        u.docChanged ||
        u.viewportChanged ||
        u.startState.field(matchesField) !== u.state.field(matchesField) ||
        u.startState.field(currentField) !== u.state.field(currentField)
      ) {
        this.decorations = buildMatches(u.view);
      }
    }
  },
  { decorations: (v) => v.decorations },
);

// Scan the whole doc for the (lowercased, literal) query. Non-overlapping,
// left-to-right — same ordering the parent's regex count uses, so a given
// index refers to the same occurrence across views.
function computeMatches(): Match[] {
  const out: Match[] = [];
  const q = (props.query ?? '').toLowerCase();
  if (!q || !view) return out;
  const text = view.state.doc.toString().toLowerCase();
  let i = text.indexOf(q);
  while (i !== -1) {
    out.push({ from: i, to: i + q.length });
    i = text.indexOf(q, i + q.length);
  }
  return out;
}

function refreshMatches() {
  if (!view) return;
  view.dispatch({ effects: [setMatches.of(computeMatches()), setCurrent.of(-1)] });
}

function baseExtensions() {
  return [
    lineNumbers(),
    foldGutter(),
    syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
    EditorView.lineWrapping,
    EditorState.readOnly.of(true),
    EditorView.editable.of(false),
    matchesField,
    currentField,
    matchHighlighter,
    langCompartment.of([]),
    themeCompartment.of(props.dark ? oneDark : []),
  ];
}

// Resolve the grammar for `filename` and swap it into the language
// compartment. No match → plain text (still line-numbered). Guards against
// a stale async load racing a newer filename via the `token` capture.
let loadToken = 0;
async function applyLanguage(filename: string) {
  const token = ++loadToken;
  const desc = LanguageDescription.matchFilename(languages, filename);
  if (!desc) {
    view?.dispatch({ effects: langCompartment.reconfigure([]) });
    return;
  }
  try {
    const support = await desc.load();
    if (token !== loadToken || !view) return;
    view.dispatch({ effects: langCompartment.reconfigure(support) });
  } catch {
    // Grammar chunk failed to load — fall back to plain text silently.
    if (token === loadToken && view) {
      view.dispatch({ effects: langCompartment.reconfigure([]) });
    }
  }
}

onMounted(() => {
  if (!hostEl.value) return;
  view = new EditorView({
    state: EditorState.create({ doc: props.code, extensions: baseExtensions() }),
    parent: hostEl.value,
  });
  applyLanguage(props.filename);
  refreshMatches();
});

onBeforeUnmount(() => {
  view?.destroy();
  view = null;
});

watch(() => props.code, (next) => {
  if (!view) return;
  const current = view.state.doc.toString();
  if (current === next) return;
  view.dispatch({ changes: { from: 0, to: current.length, insert: next } });
  refreshMatches();
});

watch(() => props.filename, (next) => {
  applyLanguage(next);
});

watch(() => props.dark, (isDark) => {
  if (!view) return;
  view.dispatch({ effects: themeCompartment.reconfigure(isDark ? oneDark : []) });
});

watch(() => props.query, () => {
  refreshMatches();
});

defineExpose({
  /** Number of matches currently found in the doc. */
  matchCount(): number {
    return view ? view.state.field(matchesField).length : 0;
  },
  /** Mark match `i` as current, decorate it distinctly, and scroll it into
   *  view. Out-of-range indices are ignored. */
  focusMatch(i: number, scroll = true) {
    if (!view) return;
    const matches = view.state.field(matchesField);
    if (i < 0 || i >= matches.length) return;
    view.dispatch({
      effects: scroll ? [setCurrent.of(i), EditorView.scrollIntoView(matches[i].from, { y: 'center' })] : [setCurrent.of(i)],
    });
  },
});
</script>

<style scoped>
.cm-viewer-host {
  height: 100%;
  width: 100%;
  overflow: hidden;
}
.cm-viewer-host :deep(.cm-editor) {
  height: 100%;
  font-size: 12px;
  font-family: var(--font-mono);
  background: transparent;
}
.cm-viewer-host :deep(.cm-scroller) {
  font-family: var(--font-mono);
  line-height: 1.55;
}
.cm-viewer-host :deep(.cm-content) {
  padding: 4px 0;
}
.cm-viewer-host :deep(.cm-gutters) {
  background: transparent;
  border-right: 1px solid var(--border-color);
  color: var(--text-secondary);
  font-size: 11px;
}
.cm-viewer-host :deep(.cm-activeLineGutter),
.cm-viewer-host :deep(.cm-activeLine) {
  background: transparent;
}
.cm-viewer-host :deep(.cm-focused) {
  outline: none;
}
.cm-viewer-host :deep(.cm-kw-match) {
  background: var(--sev-warn-bg-hover);
  border-radius: 2px;
}
.cm-viewer-host :deep(.cm-kw-current) {
  background: var(--primary-solid);
  color: var(--accent-contrast);
}
</style>
