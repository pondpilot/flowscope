import { useMemo, useCallback } from 'react';
import { X, ChevronDown, FileCode, Tag } from 'lucide-react';
import { cn } from '@/lib/utils';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuCheckboxItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Button } from '@/components/ui/button';
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
  className?: string;
}

const SEVERITY_OPTIONS: { value: Severity; label: string; countKey: keyof IssuesFilterBarProps['counts'] }[] = [
  { value: 'all', label: 'All', countKey: 'all' },
  { value: 'error', label: 'Errors', countKey: 'errors' },
  { value: 'warning', label: 'Warnings', countKey: 'warnings' },
  { value: 'info', label: 'Info', countKey: 'infos' },
];

/**
 * Filter bar for the Issues panel.
 * Provides severity tabs, text search, and dropdowns for code/source file filtering.
 */
export function IssuesFilterBar({
  projectId,
  availableCodes,
  availableSourceFiles,
  counts,
  className,
}: IssuesFilterBarProps) {
  const storedState = useViewStateStore(
    (state) => state.viewStates[projectId]?.issues
  );
  const updateViewState = useViewStateStore((state) => state.updateViewState);

  const filterState = useMemo(
    () => getIssuesStateWithDefaults(storedState),
    [storedState]
  );

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
      <div className="flex items-center gap-3">
        {/* Severity segmented control */}
        <div className="inline-flex items-center rounded-full border border-border-primary-light dark:border-border-primary-dark p-0.5 bg-background">
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
                  'px-3 py-1 text-xs font-medium rounded-full transition-all duration-200',
                  isActive
                    ? 'bg-primary text-primary-foreground'
                    : 'text-muted-foreground hover:text-foreground hover:bg-muted/50'
                )}
              >
                {option.label}
                {count > 0 && (
                  <span className={cn('ml-1.5', isActive ? 'opacity-80' : 'opacity-60')}>
                    {count}
                  </span>
                )}
              </button>
            );
          })}
        </div>

        {/* Code filter dropdown */}
        {availableCodes.length > 0 && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="sm"
                className={cn(
                  'h-7 px-2 text-xs gap-1',
                  filterState.codes.length > 0 && 'text-primary'
                )}
              >
                <Tag className="h-3 w-3" />
                Code
                {filterState.codes.length > 0 && (
                  <span className="ml-1 px-1.5 py-0.5 rounded-full bg-primary/10 text-primary text-[10px] font-medium">
                    {filterState.codes.length}
                  </span>
                )}
                <ChevronDown className="h-3 w-3 opacity-50" />
              </Button>
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
              <Button
                variant="ghost"
                size="sm"
                className={cn(
                  'h-7 px-2 text-xs gap-1',
                  filterState.sourceFiles.length > 0 && 'text-primary'
                )}
              >
                <FileCode className="h-3 w-3" />
                File
                {filterState.sourceFiles.length > 0 && (
                  <span className="ml-1 px-1.5 py-0.5 rounded-full bg-primary/10 text-primary text-[10px] font-medium">
                    {filterState.sourceFiles.length}
                  </span>
                )}
                <ChevronDown className="h-3 w-3 opacity-50" />
              </Button>
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

        {/* Clear all button */}
        {hasActiveFilters && (
          <Button
            variant="ghost"
            size="sm"
            onClick={clearAll}
            className="h-7 px-2 text-xs text-muted-foreground hover:text-foreground ml-auto"
          >
            Clear all
          </Button>
        )}
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
        'bg-background hover:bg-muted/50 transition-colors',
        variant === 'code' && 'border-l-[3px] border-l-warning-light dark:border-l-warning-dark',
        variant === 'file' && 'border-l-[3px] border-l-primary'
      )}
    >
      <span className="text-muted-foreground">{icon}</span>
      <span className="max-w-[150px] truncate font-mono">{label}</span>
      <button
        onClick={onRemove}
        className="ml-0.5 rounded-full p-0.5 hover:bg-muted transition-colors"
        aria-label={`Remove ${label} filter`}
      >
        <X className="h-3 w-3 text-muted-foreground" />
      </button>
    </div>
  );
}
