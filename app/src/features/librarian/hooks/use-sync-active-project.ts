import { useEffect } from 'react';

import { useProject } from '@/lib/project-store';

import { useLibrarianStore } from '../store';

export function useSyncActiveProject(): void {
  const { activeProjectId } = useProject();

  useEffect(() => {
    useLibrarianStore.getState().setActiveProjectId(activeProjectId);
  }, [activeProjectId]);
}
