import { useEffect } from 'react';

import { useProject } from '@/lib/project-store';

import { useLibrarianStore } from '../store';

export function useSyncActiveProject(): void {
  const { activeProjectId, projects } = useProject();

  useEffect(() => {
    useLibrarianStore.getState().setActiveProjectId(activeProjectId);
  }, [activeProjectId]);

  // Drop Librarian buckets for projects that no longer exist (e.g. after
  // delete). Without this, embedded PDF chunks and chat history for
  // deleted projects accumulate in RAM until the tab is closed.
  useEffect(() => {
    const validIds = new Set(projects.map((p) => p.id));
    useLibrarianStore.getState().pruneProjectBuckets(validIds);
  }, [projects]);
}
