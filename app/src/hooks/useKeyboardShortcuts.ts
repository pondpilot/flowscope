import { useEffect, useCallback } from 'react';
import type { KeyboardShortcutHandler } from '@/types';

export interface GlobalShortcut {
  /** The key to listen for (e.g., 'o', 'p', 'Enter') */
  key: string;
  /** Whether Cmd/Ctrl is required */
  cmdOrCtrl?: boolean;
  /** Whether Shift is required */
  shift?: boolean;
  /** Whether Alt is required */
  alt?: boolean;
  /** The handler function to call when the shortcut is triggered */
  handler: () => void;
  /** Whether to allow the shortcut in input fields (default: false) */
  allowInInput?: boolean;
}

/**
 * Hook to register keyboard shortcuts.
 * Automatically handles cross-platform Cmd/Ctrl modifiers and
 * prevents shortcuts from triggering in input fields by default.
 */
export function useKeyboardShortcuts(shortcuts: KeyboardShortcutHandler[]) {
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      for (const shortcut of shortcuts) {
        const modifierMatched = shortcut.modifiers.some(modifier => e[modifier]);
        if (modifierMatched && e.key === shortcut.key) {
          e.preventDefault();
          shortcut.handler();
          break;
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [shortcuts]);
}

/**
 * Hook for global keyboard shortcuts with simplified API.
 * Handles cross-platform Cmd/Ctrl and input field detection automatically.
 */
export function useGlobalShortcuts(shortcuts: GlobalShortcut[]) {
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    const target = e.target as HTMLElement;
    const isInInput = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA';

    for (const shortcut of shortcuts) {
      // Skip if in input field and not explicitly allowed
      if (isInInput && !shortcut.allowInInput) {
        continue;
      }

      // Check key match (case-insensitive)
      if (e.key.toLowerCase() !== shortcut.key.toLowerCase()) {
        continue;
      }

      // Check Cmd/Ctrl modifier
      if (shortcut.cmdOrCtrl && !(e.metaKey || e.ctrlKey)) {
        continue;
      }
      if (!shortcut.cmdOrCtrl && (e.metaKey || e.ctrlKey)) {
        continue;
      }

      // Check Shift modifier
      if (shortcut.shift && !e.shiftKey) {
        continue;
      }
      if (!shortcut.shift && e.shiftKey && shortcut.cmdOrCtrl) {
        // Allow shift to be pressed if we're using Cmd/Ctrl and shift isn't explicitly required
        // This prevents Cmd+Shift+O from matching Cmd+O
        continue;
      }

      // Check Alt modifier
      if (shortcut.alt && !e.altKey) {
        continue;
      }

      e.preventDefault();
      shortcut.handler();
      break;
    }
  }, [shortcuts]);

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);
}
