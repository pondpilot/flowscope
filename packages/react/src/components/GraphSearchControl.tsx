import { useCallback } from 'react';
import { useLineageStore } from '../store';
import { SearchAutocomplete } from './SearchAutocomplete';
import type { SearchableType, SearchSuggestion } from '../hooks/useSearchSuggestions';

interface GraphSearchControlProps {
  className?: string;
  searchTerm?: string;
  onSearchTermChange: (term: string) => void;
  searchableTypes: SearchableType[];
  /** Whether focus mode is enabled (show only lineage-related nodes) */
  focusMode?: boolean;
  /** Called when focus mode changes */
  onFocusModeChange?: (enabled: boolean) => void;
}

export function GraphSearchControl({
  className,
  searchTerm = '',
  onSearchTermChange,
  searchableTypes,
  focusMode = false,
  onFocusModeChange,
}: GraphSearchControlProps) {
  const result = useLineageStore((state) => state.result);

  const handleSuggestionSelect = useCallback(
    (suggestion: SearchSuggestion) => {
      // When selecting a suggestion, immediately search for it
      onSearchTermChange(suggestion.label);
    },
    [onSearchTermChange]
  );

  if (!result) return null;

  return (
    <SearchAutocomplete
      className={className}
      value={searchTerm}
      onSearch={onSearchTermChange}
      searchableTypes={searchableTypes}
      placeholder="Search..."
      onSuggestionSelect={handleSuggestionSelect}
      stopKeyboardPropagation
      showFocusToggle
      initialFocusMode={focusMode}
      onFocusModeChange={onFocusModeChange}
    />
  );
}
