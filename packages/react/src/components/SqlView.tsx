import { useMemo, useCallback, useEffect, useRef, type JSX } from 'react';
import CodeMirror, { type ReactCodeMirrorRef } from '@uiw/react-codemirror';
import { sql } from '@codemirror/lang-sql';
import { EditorView, Decoration, type DecorationSet } from '@codemirror/view';
import { StateField, StateEffect } from '@codemirror/state';
import { oneDark } from '@codemirror/theme-one-dark';

import { useLineage } from '../store';
import type { SqlViewProps } from '../types';
import { trySpanToCharRange } from '../utils/sqlSpans';

type HighlightRange = { from: number; to: number; className: string };

const setHighlights = StateEffect.define<HighlightRange[]>();

const highlightField = StateField.define<DecorationSet>({
  create() {
    return Decoration.none;
  },
  update(highlights, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setHighlights)) {
        if (effect.value.length === 0) {
          return Decoration.none;
        }
        const marks = effect.value.map(({ from, to, className }) =>
          Decoration.mark({ class: className }).range(from, to)
        );
        return Decoration.set(marks);
      }
    }
    if (tr.docChanged) {
      return highlights.map(tr.changes);
    }
    return highlights;
  },
  provide: (f) => EditorView.decorations.from(f),
});

const baseTheme = EditorView.baseTheme({
  '.flowscope-sql-highlight-active': {
    backgroundColor: 'rgba(102, 126, 234, 0.3)',
    borderRadius: '2px',
  },
  '.flowscope-sql-highlight-error': {
    backgroundColor: 'rgba(239, 72, 111, 0.25)',
    borderRadius: '2px',
  },
  '.flowscope-sql-highlight-warning': {
    backgroundColor: 'rgba(244, 164, 98, 0.25)',
    borderRadius: '2px',
  },
  '.flowscope-sql-highlight-info': {
    backgroundColor: 'rgba(76, 97, 255, 0.15)',
    borderRadius: '2px',
  },
});

export function SqlView({
  className,
  editable = false,
  onChange,
  value,
  isDark,
  highlightedSpan: highlightedSpanProp,
}: SqlViewProps): JSX.Element {
  const { state, actions } = useLineage();
  const isControlled = value !== undefined;

  // Warn in dev mode if highlightedSpan is passed without value (it will be ignored)
  if (process.env.NODE_ENV !== 'production' && !isControlled && highlightedSpanProp !== undefined) {
    console.warn(
      'SqlView: `highlightedSpan` prop is ignored in uncontrolled mode. Pass a `value` prop to use controlled mode.'
    );
  }

  const sqlText = isControlled ? value : state.sql;
  // In controlled mode, prefer the prop; in uncontrolled mode, use store state
  // Normalize undefined to null for consistent type handling downstream
  const highlightedSpan = isControlled ? (highlightedSpanProp ?? null) : state.highlightedSpan;
  const issueHighlights = useMemo<HighlightRange[]>(() => {
    if (isControlled) {
      return [];
    }
    const issues = state.result?.issues ?? [];
    return issues.flatMap((issue) => {
      if (!issue.span) return [];
      const className =
        issue.severity === 'error'
          ? 'flowscope-sql-highlight-error'
          : issue.severity === 'warning'
            ? 'flowscope-sql-highlight-warning'
            : 'flowscope-sql-highlight-info';
      const range = trySpanToCharRange(sqlText, issue.span, 'issue highlight');
      return range ? [{ ...range, className }] : [];
    });
  }, [state.result, isControlled, sqlText]);

  const editorRef = useRef<ReactCodeMirrorRef>(null);
  const lastAutoScrolledHighlightKeyRef = useRef<string | null>(null);

  const extensions = useMemo(
    () => [
      sql(),
      highlightField,
      baseTheme,
      EditorView.lineWrapping,
      EditorView.editable.of(editable),
    ],
    [editable]
  );

  const theme = useMemo(() => (isDark ? oneDark : 'light'), [isDark]);

  const handleChange = useCallback(
    (val: string) => {
      if (!isControlled) {
        actions.setSql(val);
      }
      onChange?.(val);
    },
    [actions, onChange, isControlled]
  );

  useEffect(() => {
    const view = editorRef.current?.view;
    if (!view) return;

    const ranges: HighlightRange[] = [];
    if (!isControlled) {
      ranges.push(...issueHighlights);
    }
    const activeRange = highlightedSpan
      ? trySpanToCharRange(sqlText, highlightedSpan, 'active highlight')
      : null;
    if (activeRange) {
      ranges.push({
        from: activeRange.from,
        to: activeRange.to,
        className: 'flowscope-sql-highlight-active',
      });
    }

    view.dispatch({
      effects: setHighlights.of(ranges),
    });

    const highlightKey = highlightedSpan ? `${highlightedSpan.start}:${highlightedSpan.end}` : null;
    if (!activeRange) {
      lastAutoScrolledHighlightKeyRef.current = null;
      return;
    }

    const shouldAutoScroll =
      lastAutoScrolledHighlightKeyRef.current !== highlightKey || !view.hasFocus;
    if (shouldAutoScroll) {
      view.dispatch({
        selection: { anchor: activeRange.from },
        scrollIntoView: true,
      });
      lastAutoScrolledHighlightKeyRef.current = highlightKey;
    }
  }, [highlightedSpan, issueHighlights, isControlled, sqlText]);

  return (
    <div className={`flowscope-sql-view ${className || ''}`}>
      <CodeMirror
        ref={editorRef}
        value={sqlText}
        onChange={handleChange}
        extensions={extensions}
        editable={editable}
        theme={theme}
        basicSetup={{
          lineNumbers: true,
          highlightActiveLineGutter: true,
          foldGutter: true,
        }}
        className="flowscope-codemirror"
      />
    </div>
  );
}
