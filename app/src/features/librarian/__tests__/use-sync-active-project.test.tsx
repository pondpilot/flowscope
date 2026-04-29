import { beforeEach, describe, expect, it, vi } from 'vitest';
import { renderHook } from '@testing-library/react';

vi.mock('../services/ai-service', () => ({
  loadAIConfig: vi.fn(() => null),
}));

let mockActiveProjectId: string | null = null;
vi.mock('@/lib/project-store', () => ({
  useProject: () => ({ activeProjectId: mockActiveProjectId }),
}));

import { useLibrarianStore } from '../store';
import { useSyncActiveProject } from '../hooks/use-sync-active-project';

const PROJECT_A = 'proj-a';
const PROJECT_B = 'proj-b';

beforeEach(() => {
  mockActiveProjectId = null;
  useLibrarianStore.setState({
    byProject: {},
    activeProjectId: null,
    isLoading: false,
    hasConfig: false,
    messages: [],
    pdfFiles: [],
    pdfChunks: [],
  });
});

describe('useSyncActiveProject', () => {
  it('pushes the initial activeProjectId from useProject() into the store', () => {
    mockActiveProjectId = PROJECT_A;
    renderHook(() => useSyncActiveProject());
    expect(useLibrarianStore.getState().activeProjectId).toBe(PROJECT_A);
  });

  it('updates the store when activeProjectId changes between renders', () => {
    mockActiveProjectId = PROJECT_A;
    const { rerender } = renderHook(() => useSyncActiveProject());
    expect(useLibrarianStore.getState().activeProjectId).toBe(PROJECT_A);

    mockActiveProjectId = PROJECT_B;
    rerender();
    expect(useLibrarianStore.getState().activeProjectId).toBe(PROJECT_B);
  });

  it('syncs null when no project is active', () => {
    mockActiveProjectId = PROJECT_A;
    const { rerender } = renderHook(() => useSyncActiveProject());
    expect(useLibrarianStore.getState().activeProjectId).toBe(PROJECT_A);

    mockActiveProjectId = null;
    rerender();
    expect(useLibrarianStore.getState().activeProjectId).toBeNull();
  });

  it('switching back re-points the flat mirror to the original bucket', () => {
    // Start on project A and seed it via the store mutators
    mockActiveProjectId = PROJECT_A;
    const { rerender } = renderHook(() => useSyncActiveProject());
    useLibrarianStore.getState().addMessage('user', 'A says hi');
    expect(useLibrarianStore.getState().messages.map((m) => m.content)).toEqual(['A says hi']);

    // Switch to project B — the flat mirror should be empty
    mockActiveProjectId = PROJECT_B;
    rerender();
    expect(useLibrarianStore.getState().activeProjectId).toBe(PROJECT_B);
    expect(useLibrarianStore.getState().messages).toEqual([]);
    useLibrarianStore.getState().addMessage('user', 'B says hi');

    // Switch back to A — the original bucket data must be restored
    mockActiveProjectId = PROJECT_A;
    rerender();
    expect(useLibrarianStore.getState().activeProjectId).toBe(PROJECT_A);
    expect(useLibrarianStore.getState().messages.map((m) => m.content)).toEqual(['A says hi']);
  });
});
