/**
 * Focus Registry - A context-based approach for managing focusable elements.
 *
 * Components register their focusable elements (like search inputs) with unique keys.
 * Shortcut handlers can then focus elements by key without DOM queries.
 */

import { createContext, useContext, useCallback, useRef, type ReactNode } from 'react';

type FocusableElement = HTMLElement | null;

interface FocusRegistry {
  /** Register a focusable element with a unique key */
  register: (key: string, element: FocusableElement) => void;
  /** Unregister a focusable element */
  unregister: (key: string) => void;
  /** Focus an element by key. Returns true if successful. */
  focus: (key: string) => boolean;
  /** Get a ref callback for registering an element */
  getRefCallback: (key: string) => (element: FocusableElement) => void;
}

const FocusRegistryContext = createContext<FocusRegistry | null>(null);

export function FocusRegistryProvider({ children }: { children: ReactNode }) {
  const elementsRef = useRef<Map<string, FocusableElement>>(new Map());

  const register = useCallback((key: string, element: FocusableElement) => {
    if (element) {
      elementsRef.current.set(key, element);
    } else {
      elementsRef.current.delete(key);
    }
  }, []);

  const unregister = useCallback((key: string) => {
    elementsRef.current.delete(key);
  }, []);

  const focus = useCallback((key: string): boolean => {
    const element = elementsRef.current.get(key);
    if (element && typeof element.focus === 'function') {
      element.focus();
      return true;
    }
    if (import.meta.env.DEV) {
      console.warn(`FocusRegistry: Element "${key}" not found or not focusable`);
    }
    return false;
  }, []);

  const getRefCallback = useCallback(
    (key: string) => {
      return (element: FocusableElement) => {
        register(key, element);
      };
    },
    [register]
  );

  const value: FocusRegistry = {
    register,
    unregister,
    focus,
    getRefCallback,
  };

  return <FocusRegistryContext.Provider value={value}>{children}</FocusRegistryContext.Provider>;
}

/**
 * Hook to access the focus registry.
 */
export function useFocusRegistry(): FocusRegistry {
  const context = useContext(FocusRegistryContext);
  if (!context) {
    throw new Error('useFocusRegistry must be used within a FocusRegistryProvider');
  }
  return context;
}

/**
 * Hook to register a focusable element with the registry.
 * Returns a ref callback to attach to the element.
 */
export function useFocusRegistration(key: string): (element: FocusableElement) => void {
  const registry = useFocusRegistry();
  return registry.getRefCallback(key);
}

/** Well-known focus keys for type safety */
export const FOCUS_KEYS = {
  LINEAGE_SEARCH: 'lineage-search',
  HIERARCHY_SEARCH: 'hierarchy-search',
  MATRIX_SEARCH: 'matrix-search',
} as const;

export type FocusKey = (typeof FOCUS_KEYS)[keyof typeof FOCUS_KEYS];
