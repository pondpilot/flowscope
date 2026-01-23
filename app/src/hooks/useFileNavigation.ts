import { useEffect } from 'react';
import { useLineageState } from '@pondpilot/flowscope-react';
import { useProject } from '@/lib/project-store';

/**
 * Handles navigation requests from the lineage graph to switch files
 */
export function useFileNavigation() {
  const lineageState = useLineageState();
  const { currentProject, selectFile } = useProject();

  useEffect(() => {
    const request = lineageState.navigationRequest;
    if (request && currentProject) {
      // Match by path first (analysis uses paths as sourceName to avoid basename collisions),
      // then fall back to name for backwards compatibility with files that have no path.
      const targetFile = currentProject.files.find(
        (f) => f.path === request.sourceName || f.name === request.sourceName
      );
      if (targetFile && targetFile.id !== currentProject.activeFileId) {
        selectFile(targetFile.id);
      }
    }
  }, [lineageState.navigationRequest, currentProject, selectFile]);
}
