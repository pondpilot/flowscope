import { useEffect, useRef } from 'react';
import { toast } from 'sonner';
import { useProject } from '@/lib/project-store';

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

    const hash = window.location.hash;
    const encoded = hash.startsWith('#share=') ? hash.slice('#share='.length) : null;
    if (!encoded) return;

    // Clear URL immediately to prevent re-import on refresh
    window.history.replaceState(null, '', window.location.pathname + window.location.search);

    // Compression support is only needed for share URLs, not normal startup.
    void import('@/lib/share')
      .then(({ decodeProject }) => {
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
      })
      .catch((err: unknown) => {
        console.error('Failed to load shared project support:', err);
        toast.error('Failed to open shared project', {
          description: 'An unexpected error occurred.',
        });
      });
  }, [importProject]);
}
