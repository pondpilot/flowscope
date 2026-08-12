/** Time without project changes before the latest snapshot is persisted. */
export const PROJECT_PERSISTENCE_DEBOUNCE_MS = 500;

export interface DebouncedProjectPersistence<T> {
  /** Replace the pending snapshot and restart the idle timer. */
  schedule(value: T): void;
  /** Persist the latest pending snapshot immediately, if one exists. */
  flush(): void;
}

/**
 * Keeps serialization out of interactive updates by retaining only the latest
 * immutable snapshot until the idle timer expires. Lifecycle owners must call
 * `flush` before teardown so the debounce window cannot lose the last change.
 */
export function createDebouncedProjectPersistence<T>(
  persist: (value: T) => void,
  delayMs = PROJECT_PERSISTENCE_DEBOUNCE_MS
): DebouncedProjectPersistence<T> {
  let pendingValue: T | undefined;
  let hasPendingValue = false;
  let timeoutId: ReturnType<typeof setTimeout> | null = null;

  const flush = () => {
    if (timeoutId !== null) {
      clearTimeout(timeoutId);
      timeoutId = null;
    }
    if (!hasPendingValue) {
      return;
    }

    const value = pendingValue as T;
    pendingValue = undefined;
    hasPendingValue = false;
    persist(value);
  };

  return {
    schedule(value) {
      pendingValue = value;
      hasPendingValue = true;
      if (timeoutId !== null) {
        clearTimeout(timeoutId);
      }
      timeoutId = setTimeout(flush, delayMs);
    },
    flush,
  };
}
