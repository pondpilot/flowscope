import { useMemo, useCallback } from 'react';
import { X, ChevronDown, FileCode, Tag, Database } from 'lucide-react';
import { cn } from '@/lib/utils';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuCheckboxItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  useViewStateStore,
  getIssuesStateWithDefaults,
  type IssuesViewState,
} from '@/lib/view-state-store';

type Severity = IssuesViewState['severity'];

interface IssuesFilterBarProps {
  projectId: string;
  /** Available issue codes extracted from the analysis result */
  availableCodes: string[];
  /** Available source files extracted from the analysis result */
  availableSourceFiles: string[];
  /** Issue counts by severity */
  counts: {
    all: number;
    errors: number;
    warnings: number;
    infos: number;
  };
  /** Number of schema-related issues (UNKNOWN_COLUMN, UNKNOWN_TABLE, etc.) */
  schemaIssueCount?: number;
  /** Callback to open the schema editor */
  onOpenSchemaEditor?: () => void;
  className?: string;
}

const SEVERITY_OPTIONS: {
  value: Severity;
  label: string;
  countKey: keyof IssuesFilterBarProps['counts'];
}[] = [
  { value: 'all', label: 'All', countKey: 'all' },
  { value: 'error', label: 'Errors', countKey: 'errors' },
  { value: 'warning', label: 'Warnings', countKey: 'warnings' },
  { value: 'info', label: 'Info', countKey: 'infos' },
];

/**
 * Filter bar for the Issues panel.
 * Styled to match the Lineage tab's filter controls (slate color scheme, rounded-full buttons).
 */
export function IssuesFilterBar({
  projectId,
  availableCodes,
  availableSourceFiles,
  counts,
  schemaIssueCount,
  onOpenSchemaEditor,
  className,
}: IssuesFilterBarProps) {
  const storedState = useViewStateStore((state) => state.viewStates[projectId]?.issues);
  const updateViewState = useViewStateStore((state) => state.updateViewState);

  const filterState = useMemo(() => getIssuesStateWithDefaults(storedState), [storedState]);

  const updateFilter = useCallback(
    (updates: Partial<IssuesViewState>) => {
      updateViewState(projectId, 'issues', updates);
    },
    [projectId, updateViewState]
  );

  const setSeverity = useCallback(
    (severity: Severity) => {
      updateFilter({ severity });
    },
    [updateFilter]
  );

  const toggleCode = useCallback(
    (code: string) => {
      const newCodes = filterState.codes.includes(code)
        ? filterState.codes.filter((c) => c !== code)
        : [...filterState.codes, code];
      updateFilter({ codes: newCodes });
    },
    [filterState.codes, updateFilter]
  );

  const toggleSourceFile = useCallback(
    (file: string) => {
      const newFiles = filterState.sourceFiles.includes(file)
        ? filterState.sourceFiles.filter((f) => f !== file)
        : [...filterState.sourceFiles, file];
      updateFilter({ sourceFiles: newFiles });
    },
    [filterState.sourceFiles, updateFilter]
  );

  const removeCode = useCallback(
    (code: string) => {
      updateFilter({ codes: filterState.codes.filter((c) => c !== code) });
    },
    [filterState.codes, updateFilter]
  );

  const removeSourceFile = useCallback(
    (file: string) => {
      updateFilter({ sourceFiles: filterState.sourceFiles.filter((f) => f !== file) });
    },
    [filterState.sourceFiles, updateFilter]
  );

  const clearAll = useCallback(() => {
    updateFilter({
      severity: 'all',
      codes: [],
      sourceFiles: [],
    });
  }, [updateFilter]);

  const hasActiveFilters =
    filterState.severity !== 'all' ||
    filterState.codes.length > 0 ||
    filterState.sourceFiles.length > 0;

  return (
    <div className={cn('flex flex-col gap-2 px-4 py-2 border-b bg-muted/5', className)}>
      {/* Top row: Severity tabs + Dropdowns */}
      <div className="flex items-center gap-2">
        {/* Severity segmented control - matches ViewModeSelector/LayoutSelector style */}
        <div className="inline-flex h-8 items-center rounded-full border border-slate-200/60 dark:border-slate-700/60 bg-white/95 dark:bg-slate-900/95 shadow-xs backdrop-blur-xs p-0.5">
          {SEVERITY_OPTIONS.map((option) => {
            const count = counts[option.countKey];
            const isActive = filterState.severity === option.value;
            // Skip rendering if count is 0 (except for 'all')
            if (count === 0 && option.value !== 'all') return null;

            return (
              <button
                key={option.value}
                onClick={() => setSeverity(option.value)}
                className={cn(
                  'h-7 px-3 text-xs font-medium rounded-full transition-colors',
                  isActive
                    ? 'bg-slate-100 dark:bg-slate-700 text-slate-900 dark:text-slate-100'
                    : 'text-slate-500 hover:text-slate-700 dark:hover:text-slate-300'
                )}
              >
                {option.label}
                {count > 0 && (
                  <span
                    className={cn('ml-1.5 tabular-nums', isActive ? 'opacity-80' : 'opacity-60')}
                  >
                    {count}
                  </span>
                )}
              </button>
            );
          })}
        </div>

        {/* Code filter dropdown - matches TableFilterDropdown style */}
        {availableCodes.length > 0 && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                className={cn(
                  'flex items-center gap-1.5 h-8 px-3 rounded-full border transition-colors',
                  'border-slate-200/60 dark:border-slate-700/60 bg-white/95 dark:bg-slate-900/95 shadow-xs backdrop-blur-xs',
                  filterState.codes.length > 0
                    ? 'bg-slate-100 dark:bg-slate-700 text-slate-900 dark:text-slate-100'
                    : 'text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-slate-100'
                )}
              >
                <Tag className="h-3.5 w-3.5" />
                <span className="text-xs font-medium">Code</span>
                {filterState.codes.length > 0 && (
                  <span className="text-[10px] font-medium tabular-nums bg-slate-200 dark:bg-slate-600 px-1.5 py-0.5 rounded-full">
                    {filterState.codes.length}
                  </span>
                )}
                <ChevronDown className="h-3 w-3 opacity-50" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-56 max-h-[300px] overflow-auto">
              <DropdownMenuLabel className="text-xs text-muted-foreground">
                Filter by issue code
              </DropdownMenuLabel>
              <DropdownMenuSeparator />
              {availableCodes.map((code) => (
                <DropdownMenuCheckboxItem
                  key={code}
                  checked={filterState.codes.includes(code)}
                  onCheckedChange={() => toggleCode(code)}
                  className="text-xs font-mono"
                >
                  {code}
                </DropdownMenuCheckboxItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        )}

        {/* Source file filter dropdown */}
        {availableSourceFiles.length > 1 && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                className={cn(
                  'flex items-center gap-1.5 h-8 px-3 rounded-full border transition-colors',
                  'border-slate-200/60 dark:border-slate-700/60 bg-white/95 dark:bg-slate-900/95 shadow-xs backdrop-blur-xs',
                  filterState.sourceFiles.length > 0
                    ? 'bg-slate-100 dark:bg-slate-700 text-slate-900 dark:text-slate-100'
                    : 'text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-slate-100'
                )}
              >
                <FileCode className="h-3.5 w-3.5" />
                <span className="text-xs font-medium">File</span>
                {filterState.sourceFiles.length > 0 && (
                  <span className="text-[10px] font-medium tabular-nums bg-slate-200 dark:bg-slate-600 px-1.5 py-0.5 rounded-full">
                    {filterState.sourceFiles.length}
                  </span>
                )}
                <ChevronDown className="h-3 w-3 opacity-50" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-64 max-h-[300px] overflow-auto">
              <DropdownMenuLabel className="text-xs text-muted-foreground">
                Filter by source file
              </DropdownMenuLabel>
              <DropdownMenuSeparator />
              {availableSourceFiles.map((file) => (
                <DropdownMenuCheckboxItem
                  key={file}
                  checked={filterState.sourceFiles.includes(file)}
                  onCheckedChange={() => toggleSourceFile(file)}
                  className="text-xs"
                >
                  <span className="truncate">{file}</span>
                </DropdownMenuCheckboxItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        )}

        {/* Right-aligned section: schema badge + clear all */}
        <div className="flex items-center gap-2 ml-auto">
          {/* Schema issues badge */}
          {schemaIssueCount !== undefined && schemaIssueCount > 0 && onOpenSchemaEditor && (
            <button
              onClick={onOpenSchemaEditor}
              className={cn(
                'flex items-center gap-1.5 h-8 px-3 rounded-full border transition-colors',
                'border-slate-200/60 dark:border-slate-700/60 bg-white/95 dark:bg-slate-900/95 shadow-xs backdrop-blur-xs',
                'text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700 hover:text-slate-900 dark:hover:text-slate-100'
              )}
            >
              <Database className="h-3.5 w-3.5" />
              <span className="text-xs font-medium tabular-nums">{schemaIssueCount}</span>
              <span className="text-xs font-medium">schema</span>
            </button>
          )}

          {/* Clear all button */}
          {hasActiveFilters && (
            <button
              onClick={clearAll}
              className="h-6 px-2 text-xs text-slate-500 hover:text-slate-900 dark:hover:text-slate-100 transition-colors"
            >
              Clear all
            </button>
          )}
        </div>
      </div>

      {/* Active filter chips row */}
      {(filterState.codes.length > 0 || filterState.sourceFiles.length > 0) && (
        <div className="flex items-center gap-1.5 flex-wrap">
          {filterState.codes.map((code) => (
            <FilterChip
              key={`code-${code}`}
              label={code}
              icon={<Tag className="h-3 w-3" />}
              variant="code"
              onRemove={() => removeCode(code)}
            />
          ))}
          {filterState.sourceFiles.map((file) => (
            <FilterChip
              key={`file-${file}`}
              label={file}
              icon={<FileCode className="h-3 w-3" />}
              variant="file"
              onRemove={() => removeSourceFile(file)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

interface FilterChipProps {
  label: string;
  icon: React.ReactNode;
  variant: 'code' | 'file';
  onRemove: () => void;
}

function FilterChip({ label, icon, variant, onRemove }: FilterChipProps) {
  return (
    <div
      className={cn(
        'inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs font-medium',
        'border-slate-200/60 dark:border-slate-700/60 bg-white dark:bg-slate-900 hover:bg-slate-50 dark:hover:bg-slate-800 transition-colors',
        variant === 'code' && 'border-l-[3px] border-l-amber-500 dark:border-l-amber-400',
        variant === 'file' && 'border-l-[3px] border-l-blue-500 dark:border-l-blue-400'
      )}
    >
      <span className="text-slate-400">{icon}</span>
      <span className="max-w-[150px] truncate font-mono text-slate-700 dark:text-slate-300">
        {label}
      </span>
      <button
        onClick={onRemove}
        className="ml-0.5 rounded-full p-0.5 hover:bg-slate-200 dark:hover:bg-slate-700 transition-colors"
        aria-label={`Remove ${label} filter`}
      >
        <X className="h-3 w-3 text-slate-400" />
      </button>
    </div>
  );
}
