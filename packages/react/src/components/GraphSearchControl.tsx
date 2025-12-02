import { useState, useEffect, useRef } from 'react';
import { Search, ChevronUp, ChevronDown, X } from 'lucide-react';
import { useLineageActions, useLineageStore } from '../store';
import { Input } from './ui/input';
import { useGraphSearch } from '../hooks/useGraphSearch';

interface GraphSearchControlProps {
  className?: string;
  searchTerm: string;
  onSearchTermChange: (term: string) => void;
}

export function GraphSearchControl({ className, searchTerm, onSearchTermChange }: GraphSearchControlProps) {
  const { selectNode } = useLineageActions();
  const result = useLineageStore(state => state.result);
  
  const [currentIndex, setCurrentIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const results = useGraphSearch(searchTerm);
  const hasResults = results.length > 0;

  // Reset index when query changes
  useEffect(() => {
    setCurrentIndex(0);
  }, [searchTerm]);

  // Sync selection when navigating
  useEffect(() => {
    if (hasResults && results[currentIndex]) {
      selectNode(results[currentIndex].nodeId);
    } else if (!searchTerm) {
      selectNode(null);
    }
  }, [currentIndex, hasResults, results, selectNode, searchTerm]);

  const handleNext = () => {
    if (!hasResults) return;
    setCurrentIndex((prev) => (prev + 1) % results.length);
  };

  const handlePrev = () => {
    if (!hasResults) return;
    setCurrentIndex((prev) => (prev - 1 + results.length) % results.length);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      if (e.shiftKey) {
        handlePrev();
      } else {
        handleNext();
      }
    } else if (e.key === 'Escape') {
      e.preventDefault();
      handleClear();
      inputRef.current?.blur();
    }
  };

  const handleClear = () => {
    onSearchTermChange('');
    selectNode(null);
  };

  if (!result) return null;

  return (
    <div
      className={`relative flex items-center rounded-full border border-slate-200/60 dark:border-slate-700/60 bg-white/95 dark:bg-slate-900/95 px-2 py-1 shadow-sm backdrop-blur-sm transition-all duration-200 ${className}`}
      style={{ minWidth: hasResults && searchTerm ? '300px' : '200px' }}
      data-graph-panel
    >
      <Search
        className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 size-3.5 text-slate-400"
        strokeWidth={1.5}
      />
      <Input
        ref={inputRef}
        placeholder="Search..."
        value={searchTerm}
        onChange={(e) => onSearchTermChange(e.target.value)}
        onKeyDown={handleKeyDown}
        className="h-7 border-0 bg-transparent pl-7 pr-2 text-sm shadow-none placeholder:text-slate-400 focus-visible:ring-0 rounded-full"
      />

      {searchTerm && (
        <div className="flex items-center gap-1 border-l border-slate-200 dark:border-slate-700 pl-1 ml-1">
          {hasResults ? (
            <span className="text-[10px] text-slate-400 font-mono whitespace-nowrap px-1.5 min-w-[3ch] text-center">
              {currentIndex + 1}/{results.length}
            </span>
          ) : (
            <span className="text-[10px] text-slate-400 px-1.5 whitespace-nowrap">
              0/0
            </span>
          )}

          <div className="flex gap-0.5">
            <button
              onClick={handlePrev}
              disabled={!hasResults}
              className="flex size-6 items-center justify-center rounded-full hover:bg-slate-100 dark:hover:bg-slate-700 disabled:opacity-30 text-slate-600 dark:text-slate-400 transition-colors duration-200"
              title="Previous (Shift+Enter)"
            >
              <ChevronUp className="size-3.5" />
            </button>
            <button
              onClick={handleNext}
              disabled={!hasResults}
              className="flex size-6 items-center justify-center rounded-full hover:bg-slate-100 dark:hover:bg-slate-700 disabled:opacity-30 text-slate-600 dark:text-slate-400 transition-colors duration-200"
              title="Next (Enter)"
            >
              <ChevronDown className="size-3.5" />
            </button>
            <button
              onClick={handleClear}
              className="flex size-6 items-center justify-center rounded-full hover:bg-slate-100 dark:hover:bg-slate-700 text-slate-400 hover:text-slate-600 dark:hover:text-slate-300 transition-colors duration-200"
              title="Clear"
            >
              <X className="size-3.5" />
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
