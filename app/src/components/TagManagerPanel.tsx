import { useEffect, useMemo, useState } from 'react';
import type { ColumnTag, SchemaTable } from '@pondpilot/flowscope-core';
import { Search, X, Plus, Filter, Database, Check } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Checkbox } from '@/components/ui/checkbox';
import { cn } from '@/lib/utils';

interface TagManagerPanelProps {
  schema: SchemaTable[];
  overrides: Record<string, Record<string, ColumnTag[]>>;
  onUpdateTags: (tableCanonical: string, columnName: string, tags: ColumnTag[]) => void;
  onClearTags: (tableCanonical: string, columnName: string) => void;
}

interface TableSummary {
  key: string;
  label: string;
  canonicalName: string;
  columns: ColumnEntry[];
  hasOverrides: boolean;
}

interface ColumnEntry {
  name: string;
  baseTags: ColumnTag[];
  overrideTags: ColumnTag[];
}

const SENSITIVE_KEYWORDS = ['pii', 'secret', 'password', 'key', 'token', 'gdpr', 'hipaa', 'confidential', 'ssn', 'email', 'credit'];

function getTagVariant(tagName: string, isOverride: boolean): "default" | "secondary" | "destructive" | "outline" {
  if (SENSITIVE_KEYWORDS.some(k => tagName.toLowerCase().includes(k))) {
    return "destructive";
  }
  return isOverride ? "default" : "secondary";
}

export function TagManagerPanel({
  schema,
  overrides,
  onUpdateTags,
  onClearTags,
}: TagManagerPanelProps) {
  const [searchTerm, setSearchTerm] = useState('');
  const [selectedTableKey, setSelectedTableKey] = useState<string | null>(null);
  const [selectedColumns, setSelectedColumns] = useState<Set<string>>(new Set());
  
  // Bulk action state
  const [bulkTagName, setBulkTagName] = useState('');

  const tableSummaries = useMemo(() => buildTableSummaries(schema, overrides), [schema, overrides]);

  const filteredTables = useMemo(() => {
    const lower = searchTerm.trim().toLowerCase();
    if (!lower) return tableSummaries;
    return tableSummaries.filter(
      (t) =>
        t.label.toLowerCase().includes(lower) ||
        t.canonicalName.toLowerCase().includes(lower)
    );
  }, [tableSummaries, searchTerm]);

  const selectedTable = useMemo(
    () => tableSummaries.find((t) => t.key === selectedTableKey) ?? null,
    [tableSummaries, selectedTableKey]
  );

  // Clear selection when table changes
  useEffect(() => {
    setSelectedColumns(new Set());
    setBulkTagName('');
  }, [selectedTableKey]);

  // Collect all unique tags for auto-complete
  const allKnownTags = useMemo(() => {
      const tags = new Set<string>();
      // From Schema
      schema.forEach(t => t.columns?.forEach(c => c.classifications?.forEach(tag => tags.add(tag.name))));
      // From Overrides
      Object.values(overrides).forEach(cols => 
          Object.values(cols).forEach(colTags => 
              colTags.forEach(tag => tags.add(tag.name))
          )
      );
      return Array.from(tags).sort();
  }, [schema, overrides]);

  const handleSelectColumn = (colName: string, checked: boolean) => {
    const next = new Set(selectedColumns);
    if (checked) next.add(colName);
    else next.delete(colName);
    setSelectedColumns(next);
  };

  const handleSelectAll = (checked: boolean) => {
    if (!selectedTable) return;
    if (checked) {
      setSelectedColumns(new Set(selectedTable.columns.map(c => c.name)));
    } else {
      setSelectedColumns(new Set());
    }
  };

  const handleBulkAddTag = () => {
    if (!selectedTable || !bulkTagName.trim()) return;
    const tagToAdd = bulkTagName.trim();
    
    selectedColumns.forEach(colName => {
      const col = selectedTable.columns.find(c => c.name === colName);
      if (!col) return;
      
      // Skip if already has tag
      if (col.overrideTags.some(t => t.name.toLowerCase() === tagToAdd.toLowerCase())) return;

      const nextTags: ColumnTag[] = [
        ...col.overrideTags,
        {
          name: tagToAdd,
          source: 'user',
          updatedAt: new Date().toISOString(),
        },
      ];
      onUpdateTags(selectedTable.key, colName, nextTags);
    });
    setBulkTagName('');
    // Optionally clear selection? keeping it for now allows chaining actions
  };

  const handleBulkClear = () => {
    if (!selectedTable) return;
    selectedColumns.forEach(colName => {
       onClearTags(selectedTable.key, colName);
    });
  };

  const allSelected = Boolean(
    selectedTable && selectedTable.columns.length > 0 && selectedColumns.size === selectedTable.columns.length
  );

  return (
    <div className="flex h-full flex-col overflow-hidden border-t bg-background">
      <div className="grid h-full w-full grid-cols-[300px_minmax(0,1fr)] divide-x">
        {/* Left Sidebar: Table List */}
        <div className="flex h-full flex-col bg-muted/10">
          <div className="p-4 border-b space-y-3">
             <div className="relative">
              <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
              <Input
                placeholder="Search tables..."
                value={searchTerm}
                onChange={(e) => setSearchTerm(e.target.value)}
                className="pl-9 bg-background"
              />
            </div>
            <div className="flex items-center justify-between text-xs text-muted-foreground px-1">
              <span>{filteredTables.length} tables</span>
              {searchTerm && (
                <Button 
                  variant="ghost" 
                  size="sm" 
                  className="h-auto p-0 hover:bg-transparent text-primary"
                  onClick={() => setSearchTerm('')}
                >
                  Clear
                </Button>
              )}
            </div>
          </div>
          
          <ScrollArea className="flex-1">
            <div className="flex flex-col p-2 gap-1">
              {filteredTables.length === 0 ? (
                <div className="p-4 text-center text-sm text-muted-foreground">
                  No tables found.
                </div>
              ) : (
                filteredTables.map((table) => (
                  <button
                    key={table.key}
                    onClick={() => setSelectedTableKey(table.key)}
                    className={cn(
                      'flex items-start gap-3 rounded-md px-3 py-2.5 text-left text-sm transition-colors hover:bg-accent/50',
                      selectedTableKey === table.key
                        ? 'bg-accent text-accent-foreground shadow-sm font-medium'
                        : 'text-muted-foreground'
                    )}
                  >
                    <Database className="mt-0.5 h-4 w-4 shrink-0 opacity-70" />
                    <div className="flex-1 min-w-0">
                      <div className="truncate leading-tight">{table.label}</div>
                      <div className="text-[10px] text-muted-foreground/70 truncate mt-0.5 font-normal">
                        {table.canonicalName}
                      </div>
                    </div>
                    {table.hasOverrides && (
                      <div className="h-1.5 w-1.5 rounded-full bg-primary shrink-0 mt-1.5" title="Has overridden tags" />
                    )}
                  </button>
                ))
              )}
            </div>
          </ScrollArea>
        </div>

        {/* Right Panel: Table Detail */}
        <div className="flex h-full flex-col bg-background min-w-0">
          {!selectedTable ? (
            <div className="flex flex-1 flex-col items-center justify-center gap-4 p-8 text-center">
              <div className="rounded-full bg-muted/20 p-4">
                <Filter className="h-8 w-8 text-muted-foreground/50" />
              </div>
              <div>
                <h3 className="text-lg font-semibold">No Table Selected</h3>
                <p className="text-sm text-muted-foreground max-w-xs mx-auto mt-1">
                  Select a table from the sidebar to view and manage its column classifications.
                </p>
              </div>
            </div>
          ) : (
            <>
              {/* Header */}
              <div className="border-b bg-background/95 backdrop-blur z-10">
                <div className="px-6 py-4 flex items-center justify-between">
                    <div>
                    <h2 className="text-xl font-semibold tracking-tight">{selectedTable.label}</h2>
                    <p className="text-xs text-muted-foreground font-mono mt-1">{selectedTable.canonicalName}</p>
                    </div>
                    <div className="flex items-center gap-2">
                    <Badge variant="outline" className="font-normal">
                        {selectedTable.columns.length} columns
                    </Badge>
                    </div>
                </div>

                {/* Bulk Actions / Selection Header */}
                <div className="px-6 py-2 bg-muted/20 border-t flex items-center gap-3 min-h-[48px]">
                    <div className="flex items-center gap-3">
                         <Checkbox 
                            checked={allSelected} 
                            onCheckedChange={(c) => handleSelectAll(c === true)}
                            id="select-all-cols"
                         />
                         <label htmlFor="select-all-cols" className="text-sm text-muted-foreground select-none cursor-pointer">
                             {selectedColumns.size === 0 ? 'Select all' : `${selectedColumns.size} selected`}
                         </label>
                    </div>
                    
                    {selectedColumns.size > 0 && (
                        <>
                            <div className="h-4 w-px bg-border mx-1" />
                            <div className="flex items-center gap-2 animate-in fade-in slide-in-from-left-2 duration-200">
                                <Input 
                                    className="h-8 w-40 text-xs" 
                                    placeholder="Add tag to selected..." 
                                    value={bulkTagName}
                                    onChange={(e) => setBulkTagName(e.target.value)}
                                    onKeyDown={(e) => {
                                        if (e.key === 'Enter') handleBulkAddTag();
                                    }}
                                />
                                <Button size="sm" onClick={handleBulkAddTag} disabled={!bulkTagName.trim()}>
                                    Add
                                </Button>
                                <Button size="sm" variant="outline" onClick={handleBulkClear}>
                                    Clear Overrides
                                </Button>
                            </div>
                        </>
                    )}
                </div>
              </div>

              <ScrollArea className="flex-1">
                <div className="divide-y">
                  {selectedTable.columns.map((column) => (
                    <ColumnRow
                      key={column.name}
                      tableKey={selectedTable.key}
                      column={column}
                      allKnownTags={allKnownTags}
                      onUpdateTags={onUpdateTags}
                      checked={selectedColumns.has(column.name)}
                      onCheckedChange={(c) => handleSelectColumn(column.name, c)}
                    />
                  ))}
                  {selectedTable.columns.length === 0 && (
                     <div className="p-8 text-center text-sm text-muted-foreground">
                        This table has no columns defined in the schema.
                     </div>
                  )}
                </div>
              </ScrollArea>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function ColumnRow({
  tableKey,
  column,
  allKnownTags,
  onUpdateTags,
  checked,
  onCheckedChange,
}: {
  tableKey: string;
  column: ColumnEntry;
  allKnownTags: string[];
  onUpdateTags: (tableKey: string, colName: string, tags: ColumnTag[]) => void;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}) {
  const [isAdding, setIsAdding] = useState(false);
  const [newTag, setNewTag] = useState('');
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [showSuggestions, setShowSuggestions] = useState(false);
  const [suggestionIndex, setSuggestionIndex] = useState(-1);

  useEffect(() => {
    if (!newTag.trim()) {
        setSuggestions([]);
        setShowSuggestions(false);
        setSuggestionIndex(-1);
        return;
    }
    const lower = newTag.toLowerCase();
    const matches = allKnownTags.filter(t => t.toLowerCase().includes(lower) && !column.overrideTags.some(ot => ot.name === t));
    setSuggestions(matches.slice(0, 5));
    setShowSuggestions(matches.length > 0);
    setSuggestionIndex(-1);
  }, [newTag, allKnownTags, column.overrideTags]);

  const commitTag = (tagToAdd: string) => {
    const trimmed = tagToAdd.trim();
    if (!trimmed) {
      setIsAdding(false);
      return;
    }
    
    if (column.overrideTags.some(t => t.name.toLowerCase() === trimmed.toLowerCase())) {
        setIsAdding(false);
        setNewTag('');
        return;
    }

    const nextTags: ColumnTag[] = [
      ...column.overrideTags,
      {
        name: trimmed,
        source: 'user',
        updatedAt: new Date().toISOString(),
      },
    ];
    onUpdateTags(tableKey, column.name, nextTags);
    setNewTag('');
    setIsAdding(false);
    setShowSuggestions(false);
  };

  const handleRemove = (tagName: string) => {
    const nextTags = column.overrideTags.filter((t) => t.name !== tagName);
    onUpdateTags(tableKey, column.name, nextTags);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      if (showSuggestions && suggestionIndex >= 0 && suggestions[suggestionIndex]) {
        commitTag(suggestions[suggestionIndex]);
      } else {
        commitTag(newTag);
      }
    } else if (e.key === 'Escape') {
        setIsAdding(false);
        setNewTag('');
    } else if (e.key === 'ArrowDown') {
        if (showSuggestions) {
            e.preventDefault();
            setSuggestionIndex(prev => (prev + 1) % suggestions.length);
        }
    } else if (e.key === 'ArrowUp') {
        if (showSuggestions) {
            e.preventDefault();
            setSuggestionIndex(prev => (prev - 1 + suggestions.length) % suggestions.length);
        }
    }
  };

  return (
    <div className={cn(
        "group flex flex-col sm:flex-row sm:items-center gap-3 px-6 py-4 hover:bg-muted/30 transition-colors",
        checked && "bg-muted/30"
    )}>
      <div className="flex items-center gap-3 min-w-[200px] max-w-[250px] shrink-0">
        <Checkbox checked={checked} onCheckedChange={(c) => onCheckedChange(c === true)} />
        <div className="font-medium text-sm truncate" title={column.name}>
            {column.name}
        </div>
      </div>
      
      <div className="flex-1 flex flex-wrap items-center gap-2 min-h-[32px]">
        {/* Imported/Base Tags */}
        {column.baseTags.map((tag) => (
          <Badge 
            key={`base-${tag.name}`} 
            variant={getTagVariant(tag.name, false)} 
            className="opacity-70"
          >
            {tag.name}
            <span className="sr-only">(imported)</span>
          </Badge>
        ))}

        {/* User/Override Tags */}
        {column.overrideTags.map((tag) => (
          <Badge 
            key={`override-${tag.name}`} 
            variant={getTagVariant(tag.name, true)}
            className="pl-2.5 pr-1 gap-1"
          >
            {tag.name}
            <button
              type="button"
              onClick={() => handleRemove(tag.name)}
              className="ml-0.5 rounded-full p-0.5 hover:bg-background/20 focus:outline-none"
            >
              <X className="h-3 w-3" />
              <span className="sr-only">Remove {tag.name}</span>
            </button>
          </Badge>
        ))}

        {/* Add Tag Action */}
        {isAdding ? (
          <div className="relative flex items-center gap-1 animate-in fade-in zoom-in-95 duration-100">
            <Input
              autoFocus
              className="h-7 w-32 text-xs px-2"
              placeholder="Tag name..."
              value={newTag}
              onChange={(e) => setNewTag(e.target.value)}
              onKeyDown={handleKeyDown}
              onBlur={() => {
                  // Small delay to allow click events on suggestions to fire
                  setTimeout(() => {
                      if (!showSuggestions && newTag) commitTag(newTag);
                      else if (!showSuggestions) setIsAdding(false);
                      // If suggestions are showing, we let the click handler deal with it or click outside closes it
                      else setIsAdding(false);
                  }, 200);
              }}
            />
            {showSuggestions && (
                <div className="absolute top-full left-0 mt-1 w-48 rounded-md border bg-popover p-1 text-popover-foreground shadow-md z-50">
                    {suggestions.map((suggestion, idx) => (
                        <button
                            key={suggestion}
                            className={cn(
                                "relative flex w-full cursor-default select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none",
                                idx === suggestionIndex ? "bg-accent text-accent-foreground" : "hover:bg-accent hover:text-accent-foreground"
                            )}
                            onMouseDown={(e) => {
                                e.preventDefault(); // Prevent blur
                                commitTag(suggestion);
                            }}
                            onMouseEnter={() => setSuggestionIndex(idx)}
                        >
                            {suggestion}
                            {idx === suggestionIndex && <Check className="ml-auto h-3 w-3 opacity-50" />}
                        </button>
                    ))}
                </div>
            )}
          </div>
        ) : (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs text-muted-foreground hover:text-foreground opacity-0 group-hover:opacity-100 transition-opacity"
            onClick={() => setIsAdding(true)}
          >
            <Plus className="h-3.5 w-3.5 mr-1" />
            Add Tag
          </Button>
        )}
      </div>
    </div>
  );
}

function buildTableSummaries(
  schema: SchemaTable[],
  overrides: Record<string, Record<string, ColumnTag[]>>
): TableSummary[] {
  const tableMap = new Map<string, TableSummary>();

  // Helper to ensure table entry exists
  const ensureTable = (key: string, label: string, canonical: string) => {
    let summary = tableMap.get(key);
    if (!summary) {
      summary = {
        key,
        label,
        canonicalName: canonical,
        columns: [],
        hasOverrides: false,
      };
      tableMap.set(key, summary);
    }
    return summary;
  };

  // 1. Process Schema Tables
  schema.forEach((table) => {
    const canonical = formatCanonicalName(table);
    const summary = ensureTable(canonical, table.name, canonical);
    
    (table.columns ?? []).forEach((col) => {
      summary.columns.push({
        name: col.name,
        baseTags: col.classifications ?? [],
        overrideTags: [],
      });
    });
  });

  // 2. Process Overrides (merge into existing or create new table placeholders)
  Object.entries(overrides || {}).forEach(([tableKey, colOverrides]) => {
      // We use the key as label if we didn't find it in schema
      const summary = ensureTable(tableKey, tableKey.split('.').pop() ?? tableKey, tableKey);
      
      let tableHasOverride = false;
      Object.entries(colOverrides || {}).forEach(([colName, tags]) => {
          if (!tags || tags.length === 0) return;
          tableHasOverride = true;

          const existingCol = summary.columns.find(c => c.name === colName);
          if (existingCol) {
              existingCol.overrideTags = tags;
          } else {
              summary.columns.push({
                  name: colName,
                  baseTags: [],
                  overrideTags: tags,
              });
          }
      });
      if (tableHasOverride) summary.hasOverrides = true;
  });

  // 3. Sort
  return Array.from(tableMap.values())
    .map(table => ({
        ...table,
        columns: table.columns.sort((a, b) => a.name.localeCompare(b.name))
    }))
    .sort((a, b) => a.label.localeCompare(b.label));
}

function formatCanonicalName(table: SchemaTable): string {
  const parts = [table.catalog, table.schema, table.name].filter(Boolean);
  if (!parts.length) return table.name;
  return parts.join('.');
}
