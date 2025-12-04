import { useState, useEffect, useRef, useCallback, useId } from 'react';
import { Search, X, Table2, FileCode, ScanLine, Eye, Focus } from 'lucide-react';
import { cn } from './ui/button';
import { Input } from './ui/input';
import {
  useSearchSuggestions,
  type SearchSuggestion,
  type SearchableType,
} from '../hooks/useSearchSuggestions';
import {
  GraphTooltip,
  GraphTooltipContent,
  GraphTooltipProvider,
  GraphTooltipTrigger,
  GraphTooltipArrow,
  GraphTooltipPortal,
} from './ui/graph-tooltip';

export interface SearchAutocompleteProps {
  /** Controlled value for the search input */
  value?: string;
  /** Initial search value */
  initialValue?: string;
  /** Called when value changes (debounced for performance) */
  onSearch: (value: string) => void;
  /** Which entity types to include in autocomplete */
  searchableTypes: SearchableType[];
  /** Placeholder text */
  placeholder?: string;
  /** Optional CSS class */
  className?: string;
  /** Called when a suggestion is selected */
  onSuggestionSelect?: (suggestion: SearchSuggestion) => void;
  /** Maximum suggestions to show */
  maxSuggestions?: number;
  /** Whether to stop keyboard event propagation (for ReactFlow) */
  stopKeyboardPropagation?: boolean;
  /** Debounce delay in ms */
  debounceMs?: number;
  /** Whether to show the focus mode toggle */
  showFocusToggle?: boolean;
  /** Called when focus mode changes */
  onFocusModeChange?: (enabled: boolean) => void;
  /** Initial focus mode state */
  initialFocusMode?: boolean;
}

function getTypeIcon(type: SearchableType) {
  switch (type) {
    case 'script':
      return <FileCode className="size-3 text-slate-400" />;
    case 'column':
      return <ScanLine className="size-3 text-slate-400" />;
    case 'view':
      return <Eye className="size-3 text-slate-400" />;
    case 'table':
    case 'cte':
    default:
      return <Table2 className="size-3 text-slate-400" />;
  }
}

export function SearchAutocomplete({
  value,
  initialValue = '',
  onSearch,
  searchableTypes,
  placeholder = 'Search...',
  className,
  onSuggestionSelect,
  maxSuggestions = 8,
  stopKeyboardPropagation = false,
  debounceMs = 150,
  showFocusToggle = false,
  onFocusModeChange,
  initialFocusMode = false,
}: SearchAutocompleteProps) {
  const [inputValue, setInputValue] = useState(value ?? initialValue);
  const [showSuggestions, setShowSuggestions] = useState(false);
  const [activeSuggestionIndex, setActiveSuggestionIndex] = useState(0);
  const [focusMode, setFocusMode] = useState(initialFocusMode);
  const inputRef = useRef<HTMLInputElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const suggestionsListId = useId();
  const activeOptionId = useId();
  const debounceTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastControlledValueRef = useRef<string | undefined>(value);

  // Keep local state in sync with controlled value changes
  useEffect(() => {
    if (value !== undefined) {
      if (value !== lastControlledValueRef.current) {
        setInputValue(value);
        lastControlledValueRef.current = value;
      }
    } else if (lastControlledValueRef.current !== undefined) {
      lastControlledValueRef.current = undefined;
    }
  }, [value]);


  // Debounced search callback
  const debouncedSearch = useCallback(
    (value: string) => {
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
      }
      debounceTimerRef.current = setTimeout(() => {
        onSearch(value);
      }, debounceMs);
    },
    [onSearch, debounceMs]
  );

  // Cleanup timer on unmount
  useEffect(() => {
    return () => {
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
      }
    };
  }, []);

  const { suggestions } = useSearchSuggestions({
    searchTerm: inputValue,
    searchableTypes,
    maxSuggestions,
  });

  const hasSuggestions = suggestions.length > 0;

  // Reset suggestion index when input value changes
  useEffect(() => {
    setActiveSuggestionIndex(0);
  }, [inputValue]);

  // Clamp suggestion index when suggestions change
  useEffect(() => {
    setActiveSuggestionIndex((prev) => {
      if (suggestions.length === 0) return 0;
      return Math.min(prev, suggestions.length - 1);
    });
  }, [suggestions.length]);

  // Close suggestions when clicking outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
        setShowSuggestions(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const handleClear = useCallback(() => {
    setInputValue('');
    onSearch('');
    setShowSuggestions(false);
    inputRef.current?.focus();
  }, [onSearch]);

  const handleInputChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const nextValue = e.target.value;
      setInputValue(nextValue);
      debouncedSearch(nextValue);
      setShowSuggestions(true);
    },
    [debouncedSearch]
  );

  const handleSuggestionSelect = useCallback(
    (suggestion: SearchSuggestion) => {
      const newValue = suggestion.label;
      setInputValue(newValue);
      onSearch(newValue);
      onSuggestionSelect?.(suggestion);
      setShowSuggestions(false);
    },
    [onSearch, onSuggestionSelect]
  );

  const handleFocusModeToggle = useCallback(() => {
    const newValue = !focusMode;
    setFocusMode(newValue);
    onFocusModeChange?.(newValue);
  }, [focusMode, onFocusModeChange]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (stopKeyboardPropagation) {
        e.stopPropagation();
      }

      if (showSuggestions && hasSuggestions) {
        switch (e.key) {
          case 'ArrowDown':
            e.preventDefault();
            setActiveSuggestionIndex((prev) => Math.min(prev + 1, suggestions.length - 1));
            return;
          case 'ArrowUp':
            e.preventDefault();
            setActiveSuggestionIndex((prev) => Math.max(prev - 1, 0));
            return;
          case 'Enter':
            e.preventDefault();
            if (suggestions[activeSuggestionIndex]) {
              handleSuggestionSelect(suggestions[activeSuggestionIndex]);
            }
            return;
          case 'Escape':
            e.preventDefault();
            setShowSuggestions(false);
            return;
        }
      }

      if (e.key === 'Escape') {
        e.preventDefault();
        handleClear();
        inputRef.current?.blur();
      }
    },
    [
      stopKeyboardPropagation,
      showSuggestions,
      hasSuggestions,
      suggestions,
      activeSuggestionIndex,
      handleSuggestionSelect,
      handleClear,
    ]
  );

  return (
    <div
      ref={containerRef}
      className={cn(
        'relative flex items-center rounded-full border border-slate-200/60 dark:border-slate-700/60 bg-white dark:bg-slate-900 h-9 px-2 shadow-sm transition-all duration-200 min-w-[200px]',
        className
      )}
      data-graph-panel
      onKeyDown={stopKeyboardPropagation ? (e) => e.stopPropagation() : undefined}
    >
      <Search
        className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 size-3.5 text-slate-400"
        strokeWidth={1.5}
      />
      <Input
        ref={inputRef}
        placeholder={placeholder}
        value={inputValue}
        onChange={handleInputChange}
        onFocus={() => setShowSuggestions(true)}
        onKeyDown={handleKeyDown}
        className="h-7 border-0 bg-transparent pl-7 pr-2 text-sm shadow-none placeholder:text-slate-400 focus-visible:ring-0 rounded-full"
        role="combobox"
        aria-expanded={showSuggestions && hasSuggestions}
        aria-haspopup="listbox"
        aria-controls={suggestionsListId}
        aria-activedescendant={
          showSuggestions && hasSuggestions ? `${activeOptionId}-${activeSuggestionIndex}` : undefined
        }
        aria-autocomplete="list"
      />

      {/* Autocomplete Dropdown */}
      {showSuggestions && hasSuggestions && (
        <div
          id={suggestionsListId}
          role="listbox"
          aria-label="Search suggestions"
          className="absolute top-full left-0 right-0 mt-1 bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-lg shadow-lg z-[100] max-h-60 overflow-auto py-1"
        >
          {suggestions.map((suggestion, index) => (
            <button
              key={suggestion.id}
              id={`${activeOptionId}-${index}`}
              role="option"
              aria-selected={index === activeSuggestionIndex}
              className={cn(
                'w-full text-left px-3 py-2 text-sm transition-colors flex items-center gap-2',
                index === activeSuggestionIndex
                  ? 'bg-slate-100 dark:bg-slate-800 text-slate-900 dark:text-slate-100'
                  : 'text-slate-700 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-800/50'
              )}
              onClick={() => handleSuggestionSelect(suggestion)}
              onMouseEnter={() => setActiveSuggestionIndex(index)}
            >
              {getTypeIcon(suggestion.type)}
              <span className="truncate flex-1">{suggestion.label}</span>
              {suggestion.subtitle && (
                <span className="text-xs text-slate-400 ml-auto">{suggestion.subtitle}</span>
              )}
            </button>
          ))}
        </div>
      )}

      {/* Focus mode toggle - only show when there's a search term */}
      {showFocusToggle && inputValue && (
        <>
          <div className="h-5 w-px bg-slate-200 dark:bg-slate-700 mx-1" />
          <GraphTooltipProvider>
            <GraphTooltip delayDuration={300}>
              <GraphTooltipTrigger asChild>
                <button
                  onClick={handleFocusModeToggle}
                  className={cn(
                    'flex size-6 items-center justify-center rounded-full transition-colors duration-200',
                    focusMode
                      ? 'bg-slate-100 dark:bg-slate-700 text-slate-900 dark:text-slate-100'
                      : 'text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-700'
                  )}
                  aria-label={focusMode ? 'Show all nodes' : 'Focus on lineage'}
                  aria-pressed={focusMode}
                  type="button"
                >
                  <Focus className="size-3.5" strokeWidth={focusMode ? 2.5 : 1.5} />
                </button>
              </GraphTooltipTrigger>
              <GraphTooltipPortal>
                <GraphTooltipContent side="bottom">
                  <p>{focusMode ? 'Show all nodes' : 'Focus on lineage'}</p>
                  <GraphTooltipArrow />
                </GraphTooltipContent>
              </GraphTooltipPortal>
            </GraphTooltip>
          </GraphTooltipProvider>
        </>
      )}

      {/* Clear button */}
      {inputValue && (
        <button
          onClick={handleClear}
          className="flex size-6 items-center justify-center rounded-full hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 transition-colors duration-200 ml-1"
          title="Clear (Escape)"
          type="button"
        >
          <X className="size-3.5" />
        </button>
      )}
    </div>
  );
}
