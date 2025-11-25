import { useEffect, useRef } from 'react';
import { toast } from 'sonner';
import { useProject } from '@/lib/project-store';
import { getShareDataFromUrl, clearShareDataFromUrl, decodeProject } from '@/lib/share';

/**
 * Hook that auto-imports a project from the URL hash on mount.
 * Only runs once per app lifecycle.
 */
export function useShareImport() {
  const { importProject } = useProject();
  const hasChecked = useRef(false);

  useEffect(() => {
    // Only check once
    if (hasChecked.current) return;
    hasChecked.current = true;

    const encoded = getShareDataFromUrl();
    if (!encoded) return;

    // Clear URL immediately to prevent re-import on refresh
    clearShareDataFromUrl();

    const payload = decodeProject(encoded);
    if (!payload) {
      toast.error('Failed to open shared project', {
        description: 'The link may be corrupted or invalid.',
      });
      return;
    }

    try {
      const projectName = importProject(payload);
      toast.success(`Imported "${projectName}"`, {
        description: `${payload.f.length} file${payload.f.length !== 1 ? 's' : ''} loaded`,
      });
    } catch (err) {
      console.error('Failed to import project:', err);
      toast.error('Failed to import shared project', {
        description: 'An unexpected error occurred.',
      });
    }
  }, [importProject]);
}
