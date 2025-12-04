import { useSyncExternalStore } from 'react';
import { COLORS, COLORS_DARK } from '../constants';

/**
 * Subscribes to changes in the document's dark mode class.
 * Uses MutationObserver to detect when 'dark' class is added/removed.
 */
function subscribeToDarkMode(callback: () => void): () => void {
  // Check if we're in a browser environment
  if (typeof window === 'undefined' || typeof document === 'undefined') {
    return () => {};
  }

  const observer = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      if (mutation.attributeName === 'class') {
        callback();
      }
    }
  });

  observer.observe(document.documentElement, { attributes: true });

  // Also listen for media query changes (system preference)
  const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
  mediaQuery.addEventListener('change', callback);

  return () => {
    observer.disconnect();
    mediaQuery.removeEventListener('change', callback);
  };
}

/**
 * Returns the current snapshot of dark mode state.
 */
function getSnapshot(): boolean {
  if (typeof document === 'undefined') {
    return false;
  }
  return document.documentElement.classList.contains('dark');
}

/**
 * Server snapshot always returns false (light mode).
 */
function getServerSnapshot(): boolean {
  return false;
}

/**
 * Hook to detect dark mode and return appropriate color palette.
 * Automatically updates when theme changes.
 */
export function useColors() {
  const isDark = useSyncExternalStore(subscribeToDarkMode, getSnapshot, getServerSnapshot);
  return isDark ? COLORS_DARK : COLORS;
}

/**
 * Hook that returns just the dark mode state.
 */
export function useIsDarkMode(): boolean {
  return useSyncExternalStore(subscribeToDarkMode, getSnapshot, getServerSnapshot);
}
