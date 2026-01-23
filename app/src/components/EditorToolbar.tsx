import { Play, Loader2, ChevronDown, Braces, Code } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
} from '@/components/ui/dropdown-menu';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { FileSelector } from './FileSelector';
import type { RunMode } from '@/lib/project-store';

export type SqlViewMode = 'template' | 'resolved';

interface EditorToolbarProps {
  runMode: RunMode;
  onRunModeChange: (mode: RunMode) => void;
  isAnalyzing: boolean;
  backendReady: boolean;
  onAnalyze: () => void;
  allFileCount: number;
  selectedCount: number;
  fileSelectorOpen: boolean;
  onFileSelectorOpenChange: (open: boolean) => void;
  sqlViewMode?: SqlViewMode;
  onSqlViewModeChange?: (mode: SqlViewMode) => void;
  showSqlViewToggle?: boolean;
  hasResolvedSql?: boolean;
}

export function EditorToolbar({
  runMode,
  onRunModeChange,
  isAnalyzing,
  backendReady,
  onAnalyze,
  allFileCount,
  selectedCount,
  fileSelectorOpen,
  onFileSelectorOpenChange,
  sqlViewMode = 'template',
  onSqlViewModeChange,
  showSqlViewToggle = false,
  hasResolvedSql = false,
}: EditorToolbarProps) {
  return (
    <div className="flex items-center justify-between px-3 py-2 border-b h-[44px] shrink-0 bg-muted/30 overflow-hidden gap-2">
      <div className="flex items-center gap-2 min-w-0 flex-1">
        <FileSelector open={fileSelectorOpen} onOpenChange={onFileSelectorOpenChange} />

        {showSqlViewToggle && (
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-7 w-7 p-0"
                  disabled={!hasResolvedSql || !onSqlViewModeChange}
                  aria-label={
                    sqlViewMode === 'template'
                      ? 'Switch to resolved SQL view'
                      : 'Switch to template SQL view'
                  }
                  aria-pressed={sqlViewMode === 'resolved'}
                  onClick={() => {
                    onSqlViewModeChange?.(sqlViewMode === 'template' ? 'resolved' : 'template');
                  }}
                >
                  {sqlViewMode === 'template' ? (
                    <Braces className="h-4 w-4" />
                  ) : (
                    <Code className="h-4 w-4" />
                  )}
                </Button>
              </TooltipTrigger>
              <TooltipContent>
                {!hasResolvedSql ? (
                  <p>Run analysis to see resolved SQL</p>
                ) : sqlViewMode === 'template' ? (
                  <p>Viewing template SQL. Click to see resolved.</p>
                ) : (
                  <p>Viewing resolved SQL. Click to see template.</p>
                )}
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        )}
      </div>

      <div className="flex items-center gap-2 shrink-0">
        <div className="flex items-center rounded-full overflow-hidden shadow-xs">
          <Button
            onClick={onAnalyze}
            disabled={!backendReady || isAnalyzing}
            size="sm"
            className="h-[34px] gap-1.5 bg-brand-blue-500 hover:bg-brand-blue-700 text-white font-medium rounded-none rounded-l-full border-r border-brand-blue-400/30 px-3"
          >
            {isAnalyzing ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Play className="h-3.5 w-3.5 fill-current" />
            )}
            <span className="hidden sm:inline">Run</span>
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                size="sm"
                className="h-[34px] px-3 bg-brand-blue-500 hover:bg-brand-blue-700 text-white rounded-none rounded-r-full border-l border-brand-blue-700/30"
                disabled={!backendReady || isAnalyzing}
              >
                <ChevronDown className="size-3.5" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuLabel>Run Configuration</DropdownMenuLabel>
              <DropdownMenuSeparator />
              <DropdownMenuRadioGroup
                value={runMode}
                onValueChange={(v) => onRunModeChange(v as RunMode)}
              >
                <DropdownMenuRadioItem value="current" className="text-xs justify-between">
                  <span>Run Active File Only</span>
                  <kbd className="ml-4 inline-flex h-5 select-none items-center gap-1 rounded border bg-muted px-1.5 font-mono text-[10px] font-medium text-muted-foreground">
                    <span className="text-xs">⌘</span>⇧↵
                  </kbd>
                </DropdownMenuRadioItem>
                <DropdownMenuRadioItem value="all" className="text-xs">
                  Run All Files ({allFileCount})
                </DropdownMenuRadioItem>
                <DropdownMenuRadioItem value="custom" className="text-xs">
                  Run Selected ({selectedCount})
                </DropdownMenuRadioItem>
              </DropdownMenuRadioGroup>
              <DropdownMenuSeparator />
              <div className="px-2 py-1.5 text-xs text-muted-foreground">
                <kbd className="inline-flex h-5 select-none items-center gap-1 rounded border bg-muted px-1.5 font-mono text-[10px] font-medium">
                  <span className="text-xs">⌘</span>↵
                </kbd>
                <span className="ml-2">Run in current mode</span>
              </div>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>
    </div>
  );
}
