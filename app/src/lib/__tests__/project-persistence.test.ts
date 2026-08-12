import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  createDebouncedProjectPersistence,
  PROJECT_PERSISTENCE_DEBOUNCE_MS,
} from '../project-persistence';

describe('createDebouncedProjectPersistence', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('persists only the latest snapshot after the idle window', () => {
    const persist = vi.fn();
    const persistence = createDebouncedProjectPersistence(persist);

    persistence.schedule(['SELECT 1']);
    vi.advanceTimersByTime(PROJECT_PERSISTENCE_DEBOUNCE_MS - 1);
    persistence.schedule(['SELECT 12']);
    persistence.schedule(['SELECT 123']);

    vi.advanceTimersByTime(PROJECT_PERSISTENCE_DEBOUNCE_MS - 1);
    expect(persist).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(persist).toHaveBeenCalledTimes(1);
    expect(persist).toHaveBeenCalledWith(['SELECT 123']);
  });

  it('flushes the latest snapshot once and cancels its timer', () => {
    const persist = vi.fn();
    const persistence = createDebouncedProjectPersistence(persist);

    persistence.schedule({ files: ['added.sql'] });
    persistence.schedule({ files: [] });
    persistence.flush();
    persistence.flush();
    vi.runAllTimers();

    expect(persist).toHaveBeenCalledTimes(1);
    expect(persist).toHaveBeenCalledWith({ files: [] });
  });
});
