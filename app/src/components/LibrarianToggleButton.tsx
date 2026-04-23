import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { useViewStateStore } from '@/lib/view-state-store';
import { getShortcutDisplay } from '@/lib/shortcuts';

/**
 * Toolbar button that toggles the Librarian side panel. Lives next to the
 * Schema button in the analysis toolbar.
 */
export function LibrarianToggleButton() {
  const librarianOpen = useViewStateStore((s) => s.librarianOpen);
  const toggleLibrarian = useViewStateStore((s) => s.toggleLibrarian);
  const shortcut = getShortcutDisplay('toggle-librarian') ?? '⌘L';

  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant={librarianOpen ? 'secondary' : 'outline'}
            size="sm"
            onClick={toggleLibrarian}
            className="h-7 rounded-full text-xs"
            data-testid="librarian-toggle-button"
            aria-pressed={librarianOpen}
          >
            <img src="/polly-icon.svg" alt="" className="h-5 w-5 mr-1" />
            Librarian
          </Button>
        </TooltipTrigger>
        <TooltipContent>
          <p className="flex items-center gap-2">
            Toggle Librarian
            <kbd className="px-1.5 py-0.5 text-xs bg-muted rounded border font-mono">
              {shortcut}
            </kbd>
          </p>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
