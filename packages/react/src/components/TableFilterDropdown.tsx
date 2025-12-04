import { useState, useMemo, useCallback, useRef, useEffect } from 'react';
import type { ElementType } from 'react';
import { Filter, X, Table2, Eye, Layers, ChevronDown, ChevronUp, ArrowUp, ArrowDown, ArrowLeftRight, Check } from 'lucide-react';
import { cn } from './ui/button';
import { useLineageStore } from '../store';
import { PANEL_STYLES } from '../constants';
import type { TableFilterDirection } from '../types';

interface TableItem {
  id: string;
  label: string;
  type: 'table' | 'view' | 'cte';
  /** Number of references in the lineage */
  refCount: number;
}

const DIRECTION_OPTIONS: Array<{
  value: TableFilterDirection;
  label: string;
  icon: ElementType;
}> = [
  { value: 'both', label: 'Both directions', icon: ArrowLeftRight },
  { value: 'upstream', label: 'Upstream only', icon: ArrowUp },
  { value: 'downstream', label: 'Downstream only', icon: ArrowDown },
];

function getTypeIcon(type: 'table' | 'view' | 'cte') {
  switch (type) {
    case 'view':
      return <Eye className="size-3.5 text-slate-400" />;
    case 'cte':
      return <Layers className="size-3.5 text-slate-400" />;
    case 'table':
    default:
      return <Table2 className="size-3.5 text-slate-400" />;
  }
}

export function TableFilterDropdown(): JSX.Element | null {
  const [isOpen, setIsOpen] = useState(false);
  const [searchTerm, setSearchTerm] = useState('');
  const containerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const result = useLineageStore((state) => state.result);
  const tableFilter = useLineageStore((state) => state.tableFilter);
  const toggleTableFilterSelection = useLineageStore((state) => state.toggleTableFilterSelection);
  const setTableFilterDirection = useLineageStore((state) => state.setTableFilterDirection);
  const clearTableFilter = useLineageStore((state) => state.clearTableFilter);

  // Build list of all tables/views/CTEs from global lineage
  const allTables = useMemo((): TableItem[] => {
    if (!result?.globalLineage?.nodes) return [];

    const tableMap = new Map<string, TableItem>();

    for (const node of result.globalLineage.nodes) {
      if (node.type === 'table' || node.type === 'view' || node.type === 'cte') {
        const existing = tableMap.get(node.label.toLowerCase());
        if (existing) {
          existing.refCount++;
        } else {
          tableMap.set(node.label.toLowerCase(), {
            id: node.id,
            label: node.label,
            type: node.type,
            refCount: 1,
          });
        }
      }
    }

    return Array.from(tableMap.values()).sort((a, b) => {
      // Sort by type (tables first, then views, then CTEs), then alphabetically
      const typeOrder = { table: 0, view: 1, cte: 2 };
      const typeCompare = typeOrder[a.type] - typeOrder[b.type];
      if (typeCompare !== 0) return typeCompare;
      return a.label.localeCompare(b.label);
    });
  }, [result]);

  // Filter tables based on search term
  const filteredTables = useMemo(() => {
    if (!searchTerm.trim()) return allTables;
    const lowerSearch = searchTerm.toLowerCase();
    return allTables.filter((table) => table.label.toLowerCase().includes(lowerSearch));
  }, [allTables, searchTerm]);

  // Close dropdown when clicking outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  // Close on Escape key
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && isOpen) {
        setIsOpen(false);
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [isOpen]);

  // Focus input when dropdown opens
  useEffect(() => {
    if (isOpen && inputRef.current) {
      inputRef.current.focus();
    }
  }, [isOpen]);

  const handleToggle = useCallback(() => {
    setIsOpen((prev) => !prev);
    setSearchTerm('');
  }, []);

  const handleTableToggle = useCallback(
    (tableLabel: string) => {
      toggleTableFilterSelection(tableLabel);
    },
    [toggleTableFilterSelection]
  );

  const handleClearAll = useCallback(() => {
    clearTableFilter();
    setIsOpen(false);
  }, [clearTableFilter]);

  const hasActiveFilter = tableFilter.selectedTableLabels.size > 0;
  const selectedCount = tableFilter.selectedTableLabels.size;

  if (!result) return null;

  return (
    <div ref={containerRef} className="relative">
      <div
        className={`${PANEL_STYLES.container} px-1.5 transition-all duration-200`}
        data-graph-panel
      >
        <button
          type="button"
          onClick={handleToggle}
          className={cn(
            'flex items-center gap-2 h-7 px-3 rounded-full transition-all duration-200 text-sm font-medium',
            isOpen || hasActiveFilter
              ? 'bg-slate-100 dark:bg-slate-700 text-slate-900 dark:text-slate-100'
              : 'text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-slate-100'
          )}
          aria-label="Filter by tables"
          aria-expanded={isOpen}
        >
          <Filter className="size-4" strokeWidth={hasActiveFilter ? 2.5 : 1.5} />
          <span>
            {hasActiveFilter ? `Tables (${selectedCount})` : 'Tables'}
          </span>
          {isOpen ? (
            <ChevronUp className="size-4" />
          ) : (
            <ChevronDown className="size-4" />
          )}
        </button>
      </div>

      {/* Dropdown Panel */}
      {isOpen && (
        <div className="absolute top-full left-0 mt-2 w-72 bg-white dark:bg-slate-900 border border-slate-200/60 dark:border-slate-700/60 rounded-xl shadow-lg z-[100] overflow-hidden animate-in fade-in zoom-in-95 duration-200">
          {/* Header */}
          <div className="px-3 py-2 border-b border-slate-200 dark:border-slate-700">
            <div className="flex items-center justify-between mb-2">
              <span className="text-sm font-medium text-slate-900 dark:text-slate-100">
                Filter by Tables
              </span>
              {hasActiveFilter && (
                <button
                  onClick={handleClearAll}
                  className="text-xs text-slate-500 hover:text-slate-700 dark:text-slate-400 dark:hover:text-slate-200 flex items-center gap-1"
                >
                  <X className="size-3" />
                  Clear all
                </button>
              )}
            </div>
            {/* Search input */}
            <input
              ref={inputRef}
              type="text"
              placeholder="Search tables..."
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              className="w-full px-2.5 py-1.5 text-sm border border-slate-200 dark:border-slate-700 rounded-md bg-white dark:bg-slate-800 text-slate-900 dark:text-slate-100 placeholder:text-slate-400 focus:outline-none focus:ring-1 focus:ring-blue-500"
            />
          </div>

          {/* Direction selector */}
          <div className="px-3 py-2 border-b border-slate-200 dark:border-slate-700">
            <div className="text-xs text-slate-500 dark:text-slate-400 mb-1.5">Direction</div>
            <div className="flex gap-1">
              {DIRECTION_OPTIONS.map((option) => {
                const isActive = tableFilter.direction === option.value;
                const Icon = option.icon;
                return (
                  <button
                    key={option.value}
                    type="button"
                    onClick={() => setTableFilterDirection(option.value)}
                    className={cn(
                      'flex-1 flex items-center justify-center gap-1 px-2 py-1.5 text-xs rounded-md transition-colors',
                      isActive
                        ? 'bg-slate-100 dark:bg-slate-700 text-slate-900 dark:text-slate-100'
                        : 'text-slate-600 dark:text-slate-400 hover:bg-slate-50 dark:hover:bg-slate-800'
                    )}
                  >
                    <Icon className="size-3" />
                    <span>{option.label.split(' ')[0]}</span>
                  </button>
                );
              })}
            </div>
          </div>

          {/* Table list */}
          <div className="max-h-60 overflow-y-auto py-1">
            {filteredTables.length === 0 ? (
              <div className="px-3 py-4 text-center text-sm text-slate-500 dark:text-slate-400">
                {searchTerm ? 'No tables match your search' : 'No tables found'}
              </div>
            ) : (
              filteredTables.map((table) => {
                const isSelected = tableFilter.selectedTableLabels.has(table.label);
                return (
                  <button
                    key={table.label}
                    type="button"
                    onClick={() => handleTableToggle(table.label)}
                    className={cn(
                      'w-full text-left px-3 py-2 text-sm transition-colors flex items-center gap-2',
                      isSelected
                        ? 'bg-blue-50 dark:bg-blue-950/50 text-blue-700 dark:text-blue-300'
                        : 'text-slate-700 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-800/50'
                    )}
                  >
                    <div
                      className={cn(
                        'size-4 rounded border flex items-center justify-center flex-shrink-0',
                        isSelected
                          ? 'bg-blue-500 border-blue-500'
                          : 'border-slate-300 dark:border-slate-600'
                      )}
                    >
                      {isSelected && <Check className="size-3 text-white" />}
                    </div>
                    {getTypeIcon(table.type)}
                    <span className="truncate flex-1">{table.label}</span>
                    <span className="text-xs text-slate-400 dark:text-slate-500 ml-auto">
                      {table.refCount > 1 ? `${table.refCount}x` : ''}
                    </span>
                  </button>
                );
              })
            )}
          </div>

          {/* Footer with apply hint */}
          {hasActiveFilter && (
            <div className="px-3 py-2 border-t border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800/50">
              <p className="text-xs text-slate-500 dark:text-slate-400">
                Showing lineage for {selectedCount} selected table{selectedCount > 1 ? 's' : ''}
              </p>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
