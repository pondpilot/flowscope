/**
 * Centralized keyboard shortcuts registry.
 * All shortcuts are defined here and consumed by the help dialog,
 * command palette, and individual components.
 */

export type ShortcutCategory = 'navigation' | 'view' | 'actions' | 'settings';
export type ShortcutContext = 'global' | 'lineage' | 'hierarchy' | 'matrix' | 'editor';

export interface ShortcutDefinition {
  /** Unique identifier for the shortcut */
  id: string;
  /** The key to listen for (e.g., 'o', 'Enter', '1') */
  key: string;
  /** Whether Cmd (Mac) or Ctrl (Windows/Linux) is required */
  cmdOrCtrl?: boolean;
  /** Whether Shift is required */
  shift?: boolean;
  /** Whether Alt/Option is required */
  alt?: boolean;
  /** Short label for display (e.g., "Open Files") */
  label: string;
  /** Longer description for command palette */
  description?: string;
  /** Category for grouping in help dialog */
  category: ShortcutCategory;
  /** Context where shortcut is active ('global' = always) */
  context: ShortcutContext;
}

/**
 * Detect if the current platform is macOS.
 */
export function isMac(): boolean {
  if (typeof navigator === 'undefined') return false;
  return navigator.platform.toUpperCase().indexOf('MAC') >= 0;
}

/**
 * Format a shortcut definition for display.
 * Returns platform-appropriate symbols (⌘ on Mac, Ctrl on Windows/Linux).
 */
export function formatShortcut(def: ShortcutDefinition): string {
  const parts: string[] = [];
  const mac = isMac();

  if (def.cmdOrCtrl) {
    parts.push(mac ? '⌘' : 'Ctrl+');
  }
  if (def.shift) {
    parts.push(mac ? '⇧' : 'Shift+');
  }
  if (def.alt) {
    parts.push(mac ? '⌥' : 'Alt+');
  }

  // Format special keys
  let keyDisplay = def.key;
  if (def.key === 'Enter') {
    keyDisplay = mac ? '↵' : 'Enter';
  } else if (def.key === 'Escape') {
    keyDisplay = 'Esc';
  } else if (def.key === '/') {
    keyDisplay = '/';
  } else if (def.key === '?') {
    keyDisplay = '?';
  } else if (def.key === '\\') {
    keyDisplay = '\\';
  } else {
    keyDisplay = def.key.toUpperCase();
  }

  parts.push(keyDisplay);

  return parts.join('');
}

/**
 * Format shortcut for display in a more verbose way (for tooltips).
 * Returns individual parts for flexible rendering.
 */
export function getShortcutParts(def: ShortcutDefinition): string[] {
  const parts: string[] = [];
  const mac = isMac();

  if (def.cmdOrCtrl) {
    parts.push(mac ? '⌘' : 'Ctrl');
  }
  if (def.shift) {
    parts.push(mac ? '⇧' : 'Shift');
  }
  if (def.alt) {
    parts.push(mac ? '⌥' : 'Alt');
  }

  let keyDisplay = def.key;
  if (def.key === 'Enter') {
    keyDisplay = mac ? '↵' : 'Enter';
  } else if (def.key === 'Escape') {
    keyDisplay = 'Esc';
  } else {
    keyDisplay = def.key.toUpperCase();
  }

  parts.push(keyDisplay);
  return parts;
}

/**
 * All keyboard shortcuts in the application.
 */
export const SHORTCUTS: ShortcutDefinition[] = [
  // Navigation - Global
  {
    id: 'help',
    key: '?',
    label: 'Keyboard shortcuts',
    description: 'Show keyboard shortcuts help',
    category: 'navigation',
    context: 'global',
  },
  {
    id: 'command-palette',
    key: 'k',
    cmdOrCtrl: true,
    label: 'Command palette',
    description: 'Open command palette',
    category: 'navigation',
    context: 'global',
  },
  {
    id: 'open-files',
    key: 'o',
    cmdOrCtrl: true,
    label: 'Open files',
    description: 'Open file selector',
    category: 'navigation',
    context: 'global',
  },
  {
    id: 'open-projects',
    key: 'p',
    cmdOrCtrl: true,
    label: 'Open projects',
    description: 'Open project selector',
    category: 'navigation',
    context: 'global',
  },
  {
    id: 'open-dialect',
    key: 'd',
    cmdOrCtrl: true,
    label: 'Select dialect',
    description: 'Open SQL dialect selector',
    category: 'navigation',
    context: 'global',
  },
  {
    id: 'toggle-editor',
    key: 'b',
    cmdOrCtrl: true,
    label: 'Toggle editor',
    description: 'Expand or collapse editor panel',
    category: 'navigation',
    context: 'global',
  },

  // Navigation - Tabs
  {
    id: 'tab-lineage',
    key: '1',
    label: 'Lineage tab',
    description: 'Switch to Lineage view',
    category: 'navigation',
    context: 'global',
  },
  {
    id: 'tab-hierarchy',
    key: '2',
    label: 'Hierarchy tab',
    description: 'Switch to Hierarchy view',
    category: 'navigation',
    context: 'global',
  },
  {
    id: 'tab-matrix',
    key: '3',
    label: 'Matrix tab',
    description: 'Switch to Matrix view',
    category: 'navigation',
    context: 'global',
  },
  {
    id: 'tab-schema',
    key: '4',
    label: 'Schema tab',
    description: 'Switch to Schema view',
    category: 'navigation',
    context: 'global',
  },
  {
    id: 'tab-issues',
    key: '5',
    label: 'Issues tab',
    description: 'Switch to Issues view',
    category: 'navigation',
    context: 'global',
  },

  // View - Lineage controls
  {
    id: 'toggle-view-mode',
    key: 'v',
    label: 'Toggle view',
    description: 'Switch between Script and Table view',
    category: 'view',
    context: 'lineage',
  },
  {
    id: 'toggle-column-edges',
    key: 'c',
    label: 'Column edges',
    description: 'Toggle column-level lineage edges',
    category: 'view',
    context: 'lineage',
  },
  {
    id: 'expand-all',
    key: 'e',
    label: 'Expand all',
    description: 'Expand all table nodes',
    category: 'view',
    context: 'lineage',
  },
  {
    id: 'collapse-all',
    key: 'e',
    shift: true,
    label: 'Collapse all',
    description: 'Collapse all table nodes',
    category: 'view',
    context: 'lineage',
  },
  {
    id: 'toggle-script-tables',
    key: 't',
    label: 'Script tables',
    description: 'Toggle script tables visibility',
    category: 'view',
    context: 'lineage',
  },
  {
    id: 'cycle-layout',
    key: 'l',
    label: 'Cycle layout',
    description: 'Switch between Dagre and ELK layout',
    category: 'view',
    context: 'lineage',
  },
  {
    id: 'focus-search',
    key: '/',
    label: 'Search',
    description: 'Focus the search input',
    category: 'view',
    context: 'lineage',
  },

  // View - Hierarchy controls
  {
    id: 'hierarchy-search',
    key: '/',
    label: 'Search',
    description: 'Focus the filter input',
    category: 'view',
    context: 'hierarchy',
  },

  // View - Matrix controls
  {
    id: 'matrix-search',
    key: '/',
    label: 'Search',
    description: 'Focus the search input',
    category: 'view',
    context: 'matrix',
  },
  {
    id: 'toggle-heatmap',
    key: 'h',
    label: 'Heatmap',
    description: 'Toggle heatmap mode',
    category: 'view',
    context: 'matrix',
  },
  {
    id: 'toggle-xray',
    key: 'x',
    label: 'X-ray mode',
    description: 'Toggle X-ray mode',
    category: 'view',
    context: 'matrix',
  },

  // Actions
  {
    id: 'run-analysis',
    key: 'Enter',
    cmdOrCtrl: true,
    label: 'Run analysis',
    description: 'Run SQL analysis',
    category: 'actions',
    context: 'global',
  },
  {
    id: 'run-active-file',
    key: 'Enter',
    cmdOrCtrl: true,
    shift: true,
    label: 'Run active file',
    description: 'Run analysis on active file only',
    category: 'actions',
    context: 'global',
  },
  {
    id: 'export',
    key: 'e',
    cmdOrCtrl: true,
    shift: true,
    label: 'Export',
    description: 'Open export dialog',
    category: 'actions',
    context: 'global',
  },
  {
    id: 'share',
    key: 's',
    cmdOrCtrl: true,
    shift: true,
    label: 'Share',
    description: 'Open share dialog',
    category: 'actions',
    context: 'global',
  },

  // Settings
  {
    id: 'edit-schema',
    key: 'k',
    cmdOrCtrl: true,
    shift: true,
    label: 'Edit schema',
    description: 'Open schema editor',
    category: 'settings',
    context: 'global',
  },
  {
    id: 'toggle-theme',
    key: '\\',
    cmdOrCtrl: true,
    label: 'Toggle theme',
    description: 'Cycle through light, dark, and system theme',
    category: 'settings',
    context: 'global',
  },
];

/**
 * Get shortcuts filtered by category.
 */
export function getShortcutsByCategory(category: ShortcutCategory): ShortcutDefinition[] {
  return SHORTCUTS.filter(s => s.category === category);
}

/**
 * Get shortcuts filtered by context.
 */
export function getShortcutsByContext(context: ShortcutContext): ShortcutDefinition[] {
  return SHORTCUTS.filter(s => s.context === context || s.context === 'global');
}

/**
 * Find a shortcut by its ID.
 */
export function getShortcutById(id: string): ShortcutDefinition | undefined {
  return SHORTCUTS.find(s => s.id === id);
}

/**
 * Get the formatted shortcut string for a given ID.
 * Useful for displaying in tooltips.
 */
export function getShortcutDisplay(id: string): string | null {
  const shortcut = getShortcutById(id);
  return shortcut ? formatShortcut(shortcut) : null;
}
