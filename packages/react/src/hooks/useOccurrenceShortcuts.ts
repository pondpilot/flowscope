import { useEffect } from 'react';

import { useLineageActions } from '../store';

/**
 * Global keyboard shortcuts for cycling through the focused node's
 * `nameSpans`: `n` advances to the next occurrence and `Shift+n` returns to
 * the previous one. The listener is suppressed while the user is typing in
 * an editable surface (inputs, textareas, contenteditable, CodeMirror) so
 * the shortcut does not eat ordinary `n` keystrokes.
 *
 * Composition keys (Cmd/Ctrl/Alt + n) are also ignored so platform shortcuts
 * such as "new tab" still reach the browser.
 */
export function useOccurrenceShortcuts(): void {
  const { cycleOccurrence } = useLineageActions();

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (event.key !== 'n' && event.key !== 'N') return;
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      if (isTypingTarget(event.target)) return;

      event.preventDefault();
      cycleOccurrence(event.shiftKey ? 'prev' : 'next');
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [cycleOccurrence]);
}

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
  if (target.isContentEditable) return true;
  // CodeMirror's editor surface is a contenteditable wrapper; the check above
  // covers it, but also bail if the focused element sits inside the editor —
  // some skin variants use a non-contenteditable wrapper around the editable
  // line area.
  if (target.closest('.cm-editor')) return true;
  return false;
}
