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
        const modifierMatched = shortcut.modifiers.some((modifier) => e[modifier]);
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
/**
 * Check if the event target is an editable element where bare key shortcuts shouldn't fire.
 */
function isEditableElement(target: HTMLElement): boolean {
  const tagName = target.tagName;
  if (tagName === 'INPUT' || tagName === 'TEXTAREA' || tagName === 'SELECT') {
    return true;
  }
  // Check for contentEditable
  if (target.isContentEditable) {
    return true;
  }
  // Check for role="textbox" (custom editable components)
  if (target.getAttribute('role') === 'textbox') {
    return true;
  }
  return false;
}

/**
 * Generate a unique key for a shortcut to detect collisions.
 */
function getShortcutKey(shortcut: GlobalShortcut): string {
  const parts: string[] = [];
  if (shortcut.cmdOrCtrl) parts.push('cmd');
  if (shortcut.shift) parts.push('shift');
  if (shortcut.alt) parts.push('alt');
  parts.push(shortcut.key.toLowerCase());
  return parts.join('+');
}

/**
 * Check for duplicate shortcuts and warn in development mode.
 */
function detectCollisions(shortcuts: GlobalShortcut[]): void {
  if (!import.meta.env.DEV) return;

  const seen = new Map<string, number>();
  shortcuts.forEach((shortcut, index) => {
    const key = getShortcutKey(shortcut);
    const existingIndex = seen.get(key);
    if (existingIndex !== undefined) {
      console.warn(
        `Shortcut collision detected: "${key}" is registered at indices ${existingIndex} and ${index}. ` +
          'The first handler will always be used.'
      );
    } else {
      seen.set(key, index);
    }
  });
}

export function useGlobalShortcuts(shortcuts: GlobalShortcut[]) {
  // Detect shortcut collisions in development mode
  useEffect(() => {
    detectCollisions(shortcuts);
  }, [shortcuts]);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      const isInEditable = isEditableElement(target);

      for (const shortcut of shortcuts) {
        // Modifier shortcuts (Cmd/Ctrl+key) are safe in inputs by default
        // Bare key shortcuts (no modifiers) are blocked in inputs unless explicitly allowed
        const hasModifier = shortcut.cmdOrCtrl || shortcut.alt;
        const blockedInEditable = isInEditable && !hasModifier && !shortcut.allowInInput;

        if (blockedInEditable) {
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
    },
    [shortcuts]
  );

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);
}
