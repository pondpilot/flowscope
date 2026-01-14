import { useMemo } from 'react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog';
import {
  SHORTCUTS,
  formatShortcut,
  type ShortcutDefinition,
  type ShortcutCategory,
  type ShortcutContext,
} from '@/lib/shortcuts';

interface KeyboardShortcutsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Current active tab to filter view shortcuts contextually */
  activeTab?: string;
}

interface ShortcutItemProps {
  shortcut: ShortcutDefinition;
}

function ShortcutItem({ shortcut }: ShortcutItemProps) {
  return (
    <div className="flex items-center justify-between gap-4 py-1">
      <span className="text-sm text-foreground">{shortcut.label}</span>
      <kbd className="inline-flex h-5 min-w-[20px] select-none items-center justify-center gap-0.5 rounded border bg-muted px-1.5 font-mono text-[10px] font-medium text-muted-foreground shrink-0">
        {formatShortcut(shortcut)}
      </kbd>
    </div>
  );
}

interface ShortcutSectionProps {
  title: string;
  shortcuts: ShortcutDefinition[];
}

function ShortcutSection({ title, shortcuts }: ShortcutSectionProps) {
  if (shortcuts.length === 0) return null;

  return (
    <div className="space-y-1">
      <h3 className="text-xs font-medium text-muted-foreground uppercase tracking-wider mb-2">
        {title}
      </h3>
      <div className="space-y-0.5">
        {shortcuts.map(shortcut => (
          <ShortcutItem key={shortcut.id} shortcut={shortcut} />
        ))}
      </div>
    </div>
  );
}

const CATEGORY_LABELS: Record<ShortcutCategory, string> = {
  navigation: 'Navigation',
  view: 'View',
  actions: 'Actions',
  settings: 'Settings',
};

const CATEGORY_ORDER: ShortcutCategory[] = ['navigation', 'view', 'actions', 'settings'];

/** Map active tab to shortcut context for filtering */
const TAB_TO_CONTEXT: Record<string, ShortcutContext> = {
  lineage: 'lineage',
  hierarchy: 'hierarchy',
  matrix: 'matrix',
};

const CONTEXT_LABELS: Record<ShortcutContext, string> = {
  global: 'Global',
  lineage: 'Lineage',
  hierarchy: 'Hierarchy',
  matrix: 'Matrix',
  editor: 'Editor',
};

export function KeyboardShortcutsDialog({ open, onOpenChange, activeTab }: KeyboardShortcutsDialogProps) {
  // Determine current context from active tab
  const currentContext = activeTab ? TAB_TO_CONTEXT[activeTab] : undefined;

  // Group shortcuts by category, filtering view shortcuts by current context
  const shortcutsByCategory = useMemo(() => {
    return CATEGORY_ORDER.reduce(
      (acc, category) => {
        if (category === 'view') {
          // Filter view shortcuts to show only those relevant to current tab
          // If no tab context, show all view shortcuts
          acc[category] = SHORTCUTS.filter(s => {
            if (s.category !== 'view') return false;
            if (!currentContext) return true;
            return s.context === currentContext || s.context === 'global';
          });
        } else {
          acc[category] = SHORTCUTS.filter(s => s.category === category);
        }
        return acc;
      },
      {} as Record<ShortcutCategory, ShortcutDefinition[]>
    );
  }, [currentContext]);

  // Contextual view section title
  const viewTitle = currentContext
    ? `${CATEGORY_LABELS.view} (${CONTEXT_LABELS[currentContext]})`
    : CATEGORY_LABELS.view;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl max-h-[80vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle className="text-xl">Keyboard Shortcuts</DialogTitle>
          <DialogDescription>
            Press <kbd className="px-1.5 py-0.5 text-xs bg-muted rounded border font-mono">?</kbd> anytime to show this dialog
          </DialogDescription>
        </DialogHeader>

        <div className="grid grid-cols-2 gap-8 py-4 overflow-y-auto">
          {/* Left column: Navigation */}
          <div className="space-y-6">
            <ShortcutSection
              title={CATEGORY_LABELS.navigation}
              shortcuts={shortcutsByCategory.navigation}
            />
          </div>

          {/* Right column: View, Actions, Settings */}
          <div className="space-y-6">
            <ShortcutSection
              title={viewTitle}
              shortcuts={shortcutsByCategory.view}
            />
            <ShortcutSection
              title={CATEGORY_LABELS.actions}
              shortcuts={shortcutsByCategory.actions}
            />
            <ShortcutSection
              title={CATEGORY_LABELS.settings}
              shortcuts={shortcutsByCategory.settings}
            />
          </div>
        </div>

        <div className="border-t pt-3 mt-auto">
          <p className="text-xs text-muted-foreground text-center">
            Press <kbd className="px-1 py-0.5 text-[10px] bg-muted rounded border font-mono">Esc</kbd> to close
          </p>
        </div>
      </DialogContent>
    </Dialog>
  );
}
