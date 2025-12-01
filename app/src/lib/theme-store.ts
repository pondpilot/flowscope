/**
 * Theme Store
 *
 * Manages theme preference (light/dark/system) with localStorage persistence.
 * Syncs with system preference when 'system' mode is selected.
 */

import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';

// ============================================================================
// Types
// ============================================================================

export type Theme = 'light' | 'dark' | 'system';
export type ResolvedTheme = 'light' | 'dark';

interface ThemeState {
  theme: Theme;
  setTheme: (theme: Theme) => void;
}

// ============================================================================
// Utilities
// ============================================================================

/**
 * Get the system's preferred color scheme
 */
export function getSystemTheme(): ResolvedTheme {
  if (typeof window === 'undefined') return 'light';
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

/**
 * Resolve the actual theme based on preference
 */
export function resolveTheme(theme: Theme): ResolvedTheme {
  if (theme === 'system') {
    return getSystemTheme();
  }
  return theme;
}

/**
 * Apply the theme to the document
 */
export function applyTheme(theme: ResolvedTheme): void {
  if (typeof document === 'undefined') return;

  const root = document.documentElement;
  if (theme === 'dark') {
    root.classList.add('dark');
  } else {
    root.classList.remove('dark');
  }
}

// ============================================================================
// Store
// ============================================================================

export const useThemeStore = create<ThemeState>()(
  persist(
    (set) => ({
      theme: 'system',
      setTheme: (theme) => set({ theme }),
    }),
    {
      name: 'flowscope-theme',
      storage: createJSONStorage(() => localStorage),
    }
  )
);

// ============================================================================
// Initialization Hook
// ============================================================================

/**
 * Initialize theme on app startup and listen for system preference changes.
 * Call this once at the app root level.
 */
export function initializeTheme(): () => void {
  const { theme } = useThemeStore.getState();

  // Apply initial theme
  applyTheme(resolveTheme(theme));

  // Subscribe to store changes
  const unsubscribeStore = useThemeStore.subscribe((state) => {
    applyTheme(resolveTheme(state.theme));
  });

  // Listen for system preference changes
  const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
  const handleSystemChange = () => {
    const { theme } = useThemeStore.getState();
    if (theme === 'system') {
      applyTheme(getSystemTheme());
    }
  };

  mediaQuery.addEventListener('change', handleSystemChange);

  // Return cleanup function
  return () => {
    unsubscribeStore();
    mediaQuery.removeEventListener('change', handleSystemChange);
  };
}
