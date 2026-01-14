import { useState, useCallback, useEffect, useRef, useMemo } from 'react';
import { Search } from 'lucide-react';
import {
  Dialog,
  DialogContent,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import {
  SHORTCUTS,
  formatShortcut,
  type ShortcutCategory,
} from '@/lib/shortcuts';

interface CommandPaletteProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onExecuteCommand: (commandId: string) => void;
}

interface CommandItem {
  id: string;
  label: string;
  description?: string;
  shortcut?: string;
  category: ShortcutCategory;
}

const CATEGORY_LABELS: Record<ShortcutCategory, string> = {
  navigation: 'Navigation',
  view: 'View',
  actions: 'Actions',
  settings: 'Settings',
};

const CATEGORY_ORDER: ShortcutCategory[] = ['navigation', 'view', 'actions', 'settings'];

/**
 * Simple fuzzy search that matches if all characters in query appear in target in order.
 */
function fuzzyMatch(query: string, target: string): boolean {
  const q = query.toLowerCase();
  const t = target.toLowerCase();

  let qi = 0;
  for (let ti = 0; ti < t.length && qi < q.length; ti++) {
    if (t[ti] === q[qi]) {
      qi++;
    }
  }
  return qi === q.length;
}

/**
 * Score a match for sorting (higher = better match).
 */
function scoreMatch(query: string, target: string): number {
  const q = query.toLowerCase();
  const t = target.toLowerCase();

  // Exact match gets highest score
  if (t === q) return 1000;

  // Starts with query gets high score
  if (t.startsWith(q)) return 500 + (100 - t.length);

  // Contains query as substring
  if (t.includes(q)) return 200 + (100 - t.indexOf(q));

  // Fuzzy match gets lower score
  return 100;
}

export function CommandPalette({ open, onOpenChange, onExecuteCommand }: CommandPaletteProps) {
  const [query, setQuery] = useState('');
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // Convert shortcuts to command items
  const allCommands: CommandItem[] = useMemo(() => {
    return SHORTCUTS.map(shortcut => ({
      id: shortcut.id,
      label: shortcut.label,
      description: shortcut.description,
      shortcut: formatShortcut(shortcut),
      category: shortcut.category,
    }));
  }, []);

  // Filter and sort commands based on query
  const filteredCommands = useMemo(() => {
    if (!query.trim()) {
      return allCommands;
    }

    return allCommands
      .filter(cmd =>
        fuzzyMatch(query, cmd.label) ||
        (cmd.description && fuzzyMatch(query, cmd.description))
      )
      .sort((a, b) => {
        const scoreA = Math.max(
          scoreMatch(query, a.label),
          a.description ? scoreMatch(query, a.description) : 0
        );
        const scoreB = Math.max(
          scoreMatch(query, b.label),
          b.description ? scoreMatch(query, b.description) : 0
        );
        return scoreB - scoreA;
      });
  }, [query, allCommands]);

  // Group commands by category
  const groupedCommands = useMemo(() => {
    const groups: Record<ShortcutCategory, CommandItem[]> = {
      navigation: [],
      view: [],
      actions: [],
      settings: [],
    };

    for (const cmd of filteredCommands) {
      groups[cmd.category].push(cmd);
    }

    return groups;
  }, [filteredCommands]);

  // Flat list for keyboard navigation
  const flatList = useMemo(() => {
    const result: CommandItem[] = [];
    for (const category of CATEGORY_ORDER) {
      result.push(...groupedCommands[category]);
    }
    return result;
  }, [groupedCommands]);

  // Reset selection when query changes
  useEffect(() => {
    setSelectedIndex(0);
  }, [query]);

  // Reset state when dialog opens
  useEffect(() => {
    if (open) {
      setQuery('');
      setSelectedIndex(0);
      // Focus input after dialog animation
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  // Scroll selected item into view
  useEffect(() => {
    if (listRef.current && flatList.length > 0) {
      const selectedItem = listRef.current.querySelector(`[data-index="${selectedIndex}"]`);
      selectedItem?.scrollIntoView({ block: 'nearest' });
    }
  }, [selectedIndex, flatList.length]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        setSelectedIndex(prev => Math.min(prev + 1, flatList.length - 1));
        break;
      case 'ArrowUp':
        e.preventDefault();
        setSelectedIndex(prev => Math.max(prev - 1, 0));
        break;
      case 'Enter':
        e.preventDefault();
        if (flatList[selectedIndex]) {
          onExecuteCommand(flatList[selectedIndex].id);
          onOpenChange(false);
        }
        break;
      case 'Escape':
        e.preventDefault();
        onOpenChange(false);
        break;
    }
  }, [flatList, selectedIndex, onExecuteCommand, onOpenChange]);

  const handleItemClick = useCallback((commandId: string) => {
    onExecuteCommand(commandId);
    onOpenChange(false);
  }, [onExecuteCommand, onOpenChange]);

  // Calculate flat index for an item
  const getFlatIndex = (category: ShortcutCategory, indexInCategory: number): number => {
    let flatIndex = 0;
    for (const cat of CATEGORY_ORDER) {
      if (cat === category) {
        return flatIndex + indexInCategory;
      }
      flatIndex += groupedCommands[cat].length;
    }
    return flatIndex;
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="sm:max-w-lg p-0 gap-0 overflow-hidden"
        onKeyDown={handleKeyDown}
      >
        {/* Search Input */}
        <div className="flex items-center border-b px-3">
          <Search className="h-4 w-4 text-muted-foreground shrink-0" />
          <Input
            ref={inputRef}
            value={query}
            onChange={e => setQuery(e.target.value)}
            placeholder="Type a command..."
            className="border-0 focus-visible:ring-0 shadow-none h-12 text-base"
          />
        </div>

        {/* Command List */}
        <div ref={listRef} className="max-h-80 overflow-y-auto py-2">
          {flatList.length === 0 ? (
            <div className="px-4 py-8 text-center text-sm text-muted-foreground">
              No commands found
            </div>
          ) : (
            CATEGORY_ORDER.map(category => {
              const commands = groupedCommands[category];
              if (commands.length === 0) return null;

              return (
                <div key={category}>
                  <div className="px-3 py-1.5 text-xs font-medium text-muted-foreground uppercase tracking-wider">
                    {CATEGORY_LABELS[category]}
                  </div>
                  {commands.map((cmd, indexInCategory) => {
                    const flatIndex = getFlatIndex(category, indexInCategory);
                    const isSelected = flatIndex === selectedIndex;

                    return (
                      <button
                        key={cmd.id}
                        data-index={flatIndex}
                        className={`w-full flex items-center justify-between px-3 py-2 text-sm transition-colors ${
                          isSelected
                            ? 'bg-accent text-accent-foreground'
                            : 'hover:bg-muted'
                        }`}
                        onClick={() => handleItemClick(cmd.id)}
                        onMouseEnter={() => setSelectedIndex(flatIndex)}
                      >
                        <span>{cmd.label}</span>
                        {cmd.shortcut && (
                          <kbd className="inline-flex h-5 min-w-[20px] select-none items-center justify-center gap-0.5 rounded border bg-muted px-1.5 font-mono text-[10px] font-medium text-muted-foreground shrink-0">
                            {cmd.shortcut}
                          </kbd>
                        )}
                      </button>
                    );
                  })}
                </div>
              );
            })
          )}
        </div>

        {/* Footer hint */}
        <div className="border-t px-3 py-2 text-xs text-muted-foreground flex items-center justify-between">
          <span>
            <kbd className="px-1 py-0.5 rounded border bg-muted font-mono text-[10px]">↑↓</kbd>
            {' '}to navigate
          </span>
          <span>
            <kbd className="px-1 py-0.5 rounded border bg-muted font-mono text-[10px]">Enter</kbd>
            {' '}to select
          </span>
          <span>
            <kbd className="px-1 py-0.5 rounded border bg-muted font-mono text-[10px]">Esc</kbd>
            {' '}to close
          </span>
        </div>
      </DialogContent>
    </Dialog>
  );
}
