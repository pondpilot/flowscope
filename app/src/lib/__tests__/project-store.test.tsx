import { act, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { STORAGE_KEYS } from '../constants';
import type { Project } from '../project-store';

const backendState = vi.hoisted(() => ({
  type: 'wasm' as 'wasm' | 'rest' | null,
  files: null as Array<{ name: string; content: string }> | null,
  refresh: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('@pondpilot/flowscope-core', () => ({
  VALID_DIALECTS: ['generic', 'ansi'],
}));

vi.mock('../backend-context', () => ({
  useBackend: () => ({ backendType: backendState.type }),
}));

vi.mock('@/hooks/useBackendFiles', () => ({
  useBackendFiles: () => ({
    files: backendState.files,
    schema: null,
    dialect: 'generic',
    watchDirs: [],
    templateMode: 'raw',
    refresh: backendState.refresh,
  }),
}));

import { ProjectProvider, useProject } from '../project-store';
import { PROJECT_PERSISTENCE_DEBOUNCE_MS } from '../project-persistence';

const projects: Project[] = [
  {
    id: 'project-a',
    name: 'Project A',
    files: [
      {
        id: 'a.sql',
        name: 'a.sql',
        path: 'a.sql',
        content: 'SELECT 1',
        language: 'sql',
      },
    ],
    activeFileId: 'a.sql',
    dialect: 'generic',
    runMode: 'all',
    selectedFileIds: [],
    schemaSQL: '',
    templateMode: 'raw',
  },
  {
    id: 'project-b',
    name: 'Project B',
    files: [
      {
        id: 'b.sql',
        name: 'b.sql',
        path: 'b.sql',
        content: 'SELECT 2',
        language: 'sql',
      },
      {
        id: 'removed.sql',
        name: 'removed.sql',
        path: 'removed.sql',
        content: 'SELECT 0',
        language: 'sql',
      },
    ],
    activeFileId: 'b.sql',
    dialect: 'generic',
    runMode: 'all',
    selectedFileIds: [],
    schemaSQL: '',
    templateMode: 'raw',
  },
];

let projectApi: ReturnType<typeof useProject>;

function ProjectConsumer() {
  projectApi = useProject();
  return null;
}

function renderProjectProvider() {
  return render(
    <ProjectProvider>
      <ProjectConsumer />
    </ProjectProvider>
  );
}

describe('ProjectProvider persistence', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    localStorage.clear();
    localStorage.setItem(STORAGE_KEYS.PROJECTS, JSON.stringify(projects));
    localStorage.setItem(STORAGE_KEYS.ACTIVE_PROJECT_ID, 'project-a');
    backendState.type = 'wasm';
    backendState.files = null;
    backendState.refresh.mockClear();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('batches edits and deletions while project selection stays immediate', () => {
    const setItem = vi.spyOn(localStorage, 'setItem');
    const stringify = vi.spyOn(JSON, 'stringify');
    renderProjectProvider();
    setItem.mockClear();
    stringify.mockClear();

    act(() => projectApi.selectProject('project-b'));
    expect(localStorage.getItem(STORAGE_KEYS.ACTIVE_PROJECT_ID)).toBe('project-b');
    expect(setItem.mock.calls.filter(([key]) => key === STORAGE_KEYS.PROJECTS)).toHaveLength(0);

    act(() => {
      projectApi.updateFile('b.sql', 'SELECT 20');
      projectApi.updateFile('b.sql', 'SELECT 200');
      projectApi.deleteFile('removed.sql');
    });

    act(() => vi.advanceTimersByTime(PROJECT_PERSISTENCE_DEBOUNCE_MS - 1));
    expect(setItem.mock.calls.filter(([key]) => key === STORAGE_KEYS.PROJECTS)).toHaveLength(0);
    expect(stringify).not.toHaveBeenCalled();

    act(() => vi.advanceTimersByTime(1));
    const projectWrites = setItem.mock.calls.filter(([key]) => key === STORAGE_KEYS.PROJECTS);
    expect(projectWrites).toHaveLength(1);
    const persisted = JSON.parse(projectWrites[0][1]) as Project[];
    expect(stringify).toHaveBeenCalledTimes(1);
    expect(persisted[1].files).toEqual([
      expect.objectContaining({ id: 'b.sql', content: 'SELECT 200' }),
    ]);
  });

  it('flushes the latest edit when the provider unmounts', () => {
    const setItem = vi.spyOn(localStorage, 'setItem');
    const view = renderProjectProvider();
    setItem.mockClear();

    act(() => projectApi.updateFile('a.sql', 'SELECT before_teardown'));
    view.unmount();

    const projectWrites = setItem.mock.calls.filter(([key]) => key === STORAGE_KEYS.PROJECTS);
    expect(projectWrites).toHaveLength(1);
    const persisted = JSON.parse(projectWrites[0][1]) as Project[];
    expect(persisted[0].files[0].content).toBe('SELECT before_teardown');

    act(() => vi.runAllTimers());
    expect(setItem.mock.calls.filter(([key]) => key === STORAGE_KEYS.PROJECTS)).toHaveLength(1);
  });

  it('flushes additions and deletions when the page is hidden', () => {
    const setItem = vi.spyOn(localStorage, 'setItem');
    renderProjectProvider();
    setItem.mockClear();

    act(() => {
      projectApi.createFile('added.sql', 'SELECT added');
      projectApi.deleteFile('a.sql');
    });
    act(() => {
      window.dispatchEvent(new Event('pagehide'));
    });

    const projectWrites = setItem.mock.calls.filter(([key]) => key === STORAGE_KEYS.PROJECTS);
    expect(projectWrites).toHaveLength(1);
    const persisted = JSON.parse(projectWrites[0][1]) as Project[];
    expect(persisted[0].files).toEqual([
      expect.objectContaining({ name: 'added.sql', content: 'SELECT added' }),
    ]);

    act(() => vi.runAllTimers());
    expect(setItem.mock.calls.filter(([key]) => key === STORAGE_KEYS.PROJECTS)).toHaveLength(1);
  });

  it('never persists the virtual backend project', () => {
    const setItem = vi.spyOn(localStorage, 'setItem');
    const view = renderProjectProvider();
    setItem.mockClear();

    backendState.type = 'rest';
    backendState.files = [{ name: 'server.sql', content: 'SELECT server' }];
    view.rerender(
      <ProjectProvider>
        <ProjectConsumer />
      </ProjectProvider>
    );
    expect(projectApi.currentProject?.id).toBe('__backend__');

    act(() => vi.advanceTimersByTime(PROJECT_PERSISTENCE_DEBOUNCE_MS));
    const projectWrites = setItem.mock.calls.filter(([key]) => key === STORAGE_KEYS.PROJECTS);
    expect(projectWrites).toHaveLength(1);
    const persisted = JSON.parse(projectWrites[0][1]) as Project[];
    expect(persisted.map((project) => project.id)).toEqual(['project-a', 'project-b']);
  });
});
