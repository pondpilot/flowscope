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
      const targetFile = currentProject.files.find(f => f.name === request.sourceName);
      if (targetFile && targetFile.id !== currentProject.activeFileId) {
        selectFile(targetFile.id);
      }
    }
  }, [lineageState.navigationRequest, currentProject, selectFile]);
}
