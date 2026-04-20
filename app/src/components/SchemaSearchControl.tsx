import { useCallback, useEffect, useRef, useState } from 'react';
import { Search, X } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/utils';

interface SchemaSearchControlProps {
  tableNames: string[];
  onSelectTable: (tableName: string | undefined) => void;
  className?: string;
}

function findMatch(tableNames: string[], query: string): string | undefined {
  if (!query) return undefined;
  const q = query.toLowerCase();
  return tableNames.find((name) => name.toLowerCase().startsWith(q));
}

export function SchemaSearchControl({
  tableNames,
  onSelectTable,
  className,
}: SchemaSearchControlProps) {
  const [expanded, setExpanded] = useState(false);
  const [value, setValue] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (expanded) {
      inputRef.current?.focus();
    }
  }, [expanded]);

  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const next = e.target.value;
      setValue(next);
      if (!next.trim()) {
        onSelectTable(undefined);
        return;
      }
      const match = findMatch(tableNames, next.trim());
      onSelectTable(match);
    },
    [tableNames, onSelectTable]
  );

  const collapse = useCallback(() => {
    setExpanded(false);
    setValue('');
    onSelectTable(undefined);
  }, [onSelectTable]);

  const handleBlur = useCallback(() => {
    if (!value) {
      setExpanded(false);
    }
  }, [value]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        collapse();
      }
    },
    [collapse]
  );

  if (!expanded) {
    return (
      <Button
        type="button"
        variant="outline"
        size="sm"
        className={cn('h-7 w-7 p-0', className)}
        aria-label="Search schema tables"
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
        placeholder="Search tables…"
        aria-label="Search tables"
        data-testid="schema-search-input"
        className="h-7 w-48 text-xs px-3"
      />
      <Button
        type="button"
        variant="ghost"
        size="sm"
        className="h-7 w-7 p-0"
        aria-label="Close schema search"
        data-testid="schema-search-close"
        onMouseDown={(e) => {
          // Prevent blur on input before click handler fires
          e.preventDefault();
        }}
        onClick={collapse}
      >
        <X className="h-3.5 w-3.5" />
      </Button>
    </div>
  );
}
