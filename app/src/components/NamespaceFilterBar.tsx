import { useMemo, useCallback } from 'react';
import { X, Plus, Database, Layers } from 'lucide-react';
import { cn } from '@/lib/utils';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Button } from '@/components/ui/button';
import {
  useViewStateStore,
  getNamespaceFilterStateWithDefaults,
  type NamespaceFilterState,
} from '@/lib/view-state-store';
import { getNamespaceColor } from '@pondpilot/flowscope-react';
import { useThemeStore, resolveTheme } from '@/lib/theme-store';

interface NamespaceFilterBarProps {
  projectId: string;
  /** Available schemas extracted from the analysis result */
  availableSchemas: string[];
  /** Available databases extracted from the analysis result */
  availableDatabases: string[];
  className?: string;
}

/**
 * Filter bar for filtering nodes by database/schema namespace.
 * Displays active filters as chips and provides a dropdown to add new filters.
 */
export function NamespaceFilterBar({
  projectId,
  availableSchemas,
  availableDatabases,
  className,
}: NamespaceFilterBarProps) {
  const theme = useThemeStore((state) => state.theme);
  const isDark = resolveTheme(theme) === 'dark';

  const storedState = useViewStateStore((state) => state.viewStates[projectId]?.namespaceFilter);
  const updateViewState = useViewStateStore((state) => state.updateViewState);

  const filterState = useMemo(
    () => getNamespaceFilterStateWithDefaults(storedState),
    [storedState]
  );

  const updateFilter = useCallback(
    (updates: Partial<NamespaceFilterState>) => {
      updateViewState(projectId, 'namespaceFilter', updates);
    },
    [projectId, updateViewState]
  );

  const addSchema = useCallback(
    (schema: string) => {
      if (!filterState.schemas.includes(schema)) {
        updateFilter({ schemas: [...filterState.schemas, schema] });
      }
    },
    [filterState.schemas, updateFilter]
  );

  const removeSchema = useCallback(
    (schema: string) => {
      updateFilter({ schemas: filterState.schemas.filter((s) => s !== schema) });
    },
    [filterState.schemas, updateFilter]
  );

  const addDatabase = useCallback(
    (database: string) => {
      if (!filterState.databases.includes(database)) {
        updateFilter({ databases: [...filterState.databases, database] });
      }
    },
    [filterState.databases, updateFilter]
  );

  const removeDatabase = useCallback(
    (database: string) => {
      updateFilter({ databases: filterState.databases.filter((d) => d !== database) });
    },
    [filterState.databases, updateFilter]
  );

  const clearAll = useCallback(() => {
    updateFilter({ schemas: [], databases: [] });
  }, [updateFilter]);

  const hasFilters = filterState.schemas.length > 0 || filterState.databases.length > 0;

  // Filter out already-selected items from available options
  const unselectedSchemas = availableSchemas.filter((s) => !filterState.schemas.includes(s));
  const unselectedDatabases = availableDatabases.filter((d) => !filterState.databases.includes(d));

  const hasAvailableOptions = unselectedSchemas.length > 0 || unselectedDatabases.length > 0;

  // Don't render if there's nothing to filter
  if (availableSchemas.length === 0 && availableDatabases.length === 0) {
    return null;
  }

  return (
    <div
      className={cn(
        'flex items-center gap-2 px-4 py-1.5 border-b bg-muted/5 min-h-[36px]',
        className
      )}
    >
      {/* Schema chips */}
      {filterState.schemas.map((schema) => (
        <FilterChip
          key={`schema-${schema}`}
          label={schema}
          icon={<Layers className="h-3 w-3" />}
          color={getNamespaceColor(schema, isDark)}
          onRemove={() => removeSchema(schema)}
        />
      ))}

      {/* Database chips */}
      {filterState.databases.map((database) => (
        <FilterChip
          key={`db-${database}`}
          label={database}
          icon={<Database className="h-3 w-3" />}
          color={getNamespaceColor(database, isDark)}
          onRemove={() => removeDatabase(database)}
        />
      ))}

      {/* Add filter dropdown */}
      {hasAvailableOptions && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="sm"
              className="h-6 px-2 text-xs text-muted-foreground hover:text-foreground"
            >
              <Plus className="h-3 w-3 mr-1" />
              {hasFilters ? 'Add' : 'Filter by namespace'}
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-48">
            {unselectedSchemas.length > 0 && (
              <>
                <DropdownMenuLabel className="text-xs text-muted-foreground flex items-center gap-1.5">
                  <Layers className="h-3 w-3" />
                  Schemas
                </DropdownMenuLabel>
                {unselectedSchemas.map((schema) => (
                  <DropdownMenuItem
                    key={schema}
                    onClick={() => addSchema(schema)}
                    className="text-sm"
                  >
                    <span
                      className="w-2 h-2 rounded-full mr-2 shrink-0"
                      style={{ backgroundColor: getNamespaceColor(schema, isDark) }}
                    />
                    {schema}
                  </DropdownMenuItem>
                ))}
              </>
            )}
            {unselectedSchemas.length > 0 && unselectedDatabases.length > 0 && (
              <DropdownMenuSeparator />
            )}
            {unselectedDatabases.length > 0 && (
              <>
                <DropdownMenuLabel className="text-xs text-muted-foreground flex items-center gap-1.5">
                  <Database className="h-3 w-3" />
                  Databases
                </DropdownMenuLabel>
                {unselectedDatabases.map((database) => (
                  <DropdownMenuItem
                    key={database}
                    onClick={() => addDatabase(database)}
                    className="text-sm"
                  >
                    <span
                      className="w-2 h-2 rounded-full mr-2 shrink-0"
                      style={{ backgroundColor: getNamespaceColor(database, isDark) }}
                    />
                    {database}
                  </DropdownMenuItem>
                ))}
              </>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      )}

      {/* Clear all button */}
      {hasFilters && (
        <Button
          variant="ghost"
          size="sm"
          onClick={clearAll}
          className="h-6 px-2 text-xs text-muted-foreground hover:text-foreground ml-auto"
        >
          Clear all
        </Button>
      )}
    </div>
  );
}

interface FilterChipProps {
  label: string;
  icon: React.ReactNode;
  color?: string;
  onRemove: () => void;
}

function FilterChip({ label, icon, color, onRemove }: FilterChipProps) {
  return (
    <div
      className={cn(
        'inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs font-medium',
        'bg-background hover:bg-muted/50 transition-colors'
      )}
      style={{
        borderLeftWidth: '3px',
        borderLeftColor: color,
      }}
    >
      <span className="text-muted-foreground">{icon}</span>
      <span className="max-w-[120px] truncate">{label}</span>
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
