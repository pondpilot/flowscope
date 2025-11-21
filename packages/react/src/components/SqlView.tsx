import { useMemo, useCallback, useEffect, useRef } from 'react';
import CodeMirror, { type ReactCodeMirrorRef } from '@uiw/react-codemirror';
import { sql } from '@codemirror/lang-sql';
import { EditorView, Decoration, type DecorationSet } from '@codemirror/view';
import { StateField, StateEffect } from '@codemirror/state';

import { useLineage } from '../context';
import type { SqlViewProps } from '../types';

const setHighlight = StateEffect.define<{ from: number; to: number } | null>();

const highlightField = StateField.define<DecorationSet>({
  create() {
    return Decoration.none;
  },
  update(highlights, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setHighlight)) {
        if (effect.value === null) {
          return Decoration.none;
        }
        const { from, to } = effect.value;
        const mark = Decoration.mark({
          class: 'flowscope-sql-highlight',
        });
        return Decoration.set([mark.range(from, to)]);
      }
    }
    return highlights;
  },
  provide: (f) => EditorView.decorations.from(f),
});

const baseTheme = EditorView.baseTheme({
  '.flowscope-sql-highlight': {
    backgroundColor: 'rgba(102, 126, 234, 0.3)',
    borderRadius: '2px',
  },
});

export function SqlView({ className, editable = false, onChange, value }: SqlViewProps): JSX.Element {
  const { state, actions } = useLineage();
  const isControlled = value !== undefined;
  
  const sqlText = isControlled ? value : state.sql;
  const highlightedSpan = isControlled ? null : state.highlightedSpan;
  
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

    if (highlightedSpan) {
      view.dispatch({
        effects: setHighlight.of({
          from: highlightedSpan.start,
          to: highlightedSpan.end,
        }),
      });
      view.dispatch({
        selection: { anchor: highlightedSpan.start },
        scrollIntoView: true,
      });
    } else {
      view.dispatch({
        effects: setHighlight.of(null),
      });
    }
  }, [highlightedSpan]);

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