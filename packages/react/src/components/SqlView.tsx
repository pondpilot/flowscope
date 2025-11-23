import { useMemo, useCallback, useEffect, useRef } from 'react';
import CodeMirror, { type ReactCodeMirrorRef } from '@uiw/react-codemirror';
import { sql } from '@codemirror/lang-sql';
import { EditorView, Decoration, type DecorationSet } from '@codemirror/view';
import { StateField, StateEffect } from '@codemirror/state';

import { useLineage } from '../store';
import type { SqlViewProps } from '../types';

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

export function SqlView({ className, editable = false, onChange, value }: SqlViewProps): JSX.Element {
  const { state, actions } = useLineage();
  const isControlled = value !== undefined;
  
  const sqlText = isControlled ? value : state.sql;
  const highlightedSpan = isControlled ? null : state.highlightedSpan;
  const issueHighlights = useMemo<HighlightRange[]>(() => {
    if (isControlled) {
      return [];
    }
    const issues = state.result?.issues ?? [];
    return issues
      .filter((issue) => issue.span)
      .map((issue) => {
        const className =
          issue.severity === 'error'
            ? 'flowscope-sql-highlight-error'
            : issue.severity === 'warning'
              ? 'flowscope-sql-highlight-warning'
              : 'flowscope-sql-highlight-info';
        return {
          from: issue.span!.start,
          to: issue.span!.end,
          className,
        };
      });
  }, [state.result, isControlled]);
  
  const editorRef = useRef<ReactCodeMirrorRef>(null);

  const extensions = useMemo(
    () => [sql(), highlightField, baseTheme, EditorView.lineWrapping, EditorView.editable.of(editable)],
    [editable]
  );

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
    if (highlightedSpan) {
      ranges.push({
        from: highlightedSpan.start,
        to: highlightedSpan.end,
        className: 'flowscope-sql-highlight-active',
      });
    }

    view.dispatch({
      effects: setHighlights.of(ranges),
    });

    if (highlightedSpan) {
      view.dispatch({
        selection: { anchor: highlightedSpan.start },
        scrollIntoView: true,
      });
    }
  }, [highlightedSpan, issueHighlights, isControlled]);

  return (
    <div className={`flowscope-sql-view ${className || ''}`}>
      <CodeMirror
        ref={editorRef}
        value={sqlText}
        onChange={handleChange}
        extensions={extensions}
        editable={editable}
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
