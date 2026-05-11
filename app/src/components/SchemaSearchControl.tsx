import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ChevronLeft, ChevronRight, Search, X } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/utils';

interface TableWithColumns {
  name: string;
  columns?: { name: string }[];
}

interface SchemaSearchControlProps {
  tableNames: string[];
  tables?: TableWithColumns[];
  onSelectTable: (tableName: string | undefined) => void;
  className?: string;
}

function findAllMatches(
  tableNames: string[],
  query: string,
  tables?: TableWithColumns[]
): string[] {
  if (!query) return [];
  const q = query.toLowerCase();
  const matches = new Set<string>();

  // Match table names
  for (const name of tableNames) {
    if (name.toLowerCase().includes(q)) {
      matches.add(name);
    }
  }

  // Match column names — add owning table
  if (tables) {
    for (const table of tables) {
      if (table.columns?.some((col) => col.name.toLowerCase().includes(q))) {
        matches.add(table.name);
      }
    }
  }

  return Array.from(matches);
}

export function SchemaSearchControl({
  tableNames,
  tables,
  onSelectTable,
  className,
}: SchemaSearchControlProps) {
  const [expanded, setExpanded] = useState(false);
  const [value, setValue] = useState('');
  const [matchIndex, setMatchIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const hasInteractedRef = useRef(false);

  const matches = useMemo(
    () => findAllMatches(tableNames, value.trim(), tables),
    [tableNames, tables, value]
  );
  const activeMatch =
    matches.length > 0 ? matches[Math.min(matchIndex, matches.length - 1)] : undefined;

  useEffect(() => {
    if (expanded) {
      inputRef.current?.focus();
    }
  }, [expanded]);

  useEffect(() => {
    if (matches.length > 0 && matchIndex >= matches.length) {
      setMatchIndex(0);
    }
  }, [matches.length, matchIndex]);

  // Update selection when an active search has matches. Do not clear an
  // existing schema selection just because the control mounted with an empty
  // query; only clear after the user has interacted with the search field.
  // `hasInteractedRef` is scoped to a single search session — it is reset
  // when the control collapses, so a fresh open behaves like a fresh mount
  // and a parent re-render of the collapsed control does not clobber an
  // externally-set selection.
  useEffect(() => {
    if (activeMatch !== undefined) {
      onSelectTable(activeMatch);
    } else if (hasInteractedRef.current) {
      onSelectTable(undefined);
    }
  }, [activeMatch, matches.length, onSelectTable, value]);

  const handleChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    hasInteractedRef.current = true;
    setValue(e.target.value);
    setMatchIndex(0);
  }, []);

  const goNext = useCallback(() => {
    if (matches.length > 0) {
      setMatchIndex((i) => (i + 1) % matches.length);
    }
  }, [matches.length]);

  const goPrev = useCallback(() => {
    if (matches.length > 0) {
      setMatchIndex((i) => (i - 1 + matches.length) % matches.length);
    }
  }, [matches.length]);

  const collapse = useCallback(() => {
    hasInteractedRef.current = false;
    setExpanded(false);
    setValue('');
    setMatchIndex(0);
    onSelectTable(undefined);
  }, [onSelectTable]);

  const handleBlur = useCallback(() => {
    if (!value) {
      hasInteractedRef.current = false;
      setExpanded(false);
    }
  }, [value]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        collapse();
      } else if (e.key === 'Enter') {
        e.preventDefault();
        if (e.shiftKey) {
          goPrev();
        } else {
          goNext();
        }
      }
    },
    [collapse, goNext, goPrev]
  );

  if (!expanded) {
    return (
      <Button
        type="button"
        variant="outline"
        size="sm"
        className={cn('h-7 w-7 p-0', className)}
        aria-label="Search schema"
        data-testid="schema-search-toggle"
        onClick={() => setExpanded(true)}
      >
        <Search className="h-3.5 w-3.5" />
      </Button>
    );
  }

  return (
    <div className={cn('flex items-center gap-1', className)} data-testid="schema-search-field">
      <Input
        ref={inputRef}
        value={value}
        onChange={handleChange}
        onBlur={handleBlur}
        onKeyDown={handleKeyDown}
        placeholder="Search tables or columns…"
        aria-label="Search tables or columns"
        data-testid="schema-search-input"
        className="h-7 w-36 text-xs px-3"
      />
      {matches.length > 0 && (
        <>
          <span className="shrink-0 text-xs text-muted-foreground">
            {matchIndex + 1}/{matches.length}
          </span>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0"
            aria-label="Previous match"
            data-testid="schema-search-prev"
            onMouseDown={(e) => e.preventDefault()}
            onClick={goPrev}
          >
            <ChevronLeft className="h-3.5 w-3.5" />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0"
            aria-label="Next match"
            data-testid="schema-search-next"
            onMouseDown={(e) => e.preventDefault()}
            onClick={goNext}
          >
            <ChevronRight className="h-3.5 w-3.5" />
          </Button>
        </>
      )}
      <Button
        type="button"
        variant="ghost"
        size="sm"
        className="h-7 w-7 p-0"
        aria-label="Close schema search"
        data-testid="schema-search-close"
        onMouseDown={(e) => e.preventDefault()}
        onClick={collapse}
      >
        <X className="h-3.5 w-3.5" />
      </Button>
    </div>
  );
}
