import { Play, Loader2, ChevronDown } from 'lucide-react';
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
import { FileSelector } from './FileSelector';
import type { RunMode } from '@/lib/project-store';

interface EditorToolbarProps {
  runMode: RunMode;
  onRunModeChange: (mode: RunMode) => void;
  isAnalyzing: boolean;
  wasmReady: boolean;
  onAnalyze: () => void;
  allFileCount: number;
  selectedCount: number;
  fileSelectorOpen: boolean;
  onFileSelectorOpenChange: (open: boolean) => void;
}

export function EditorToolbar({
  runMode,
  onRunModeChange,
  isAnalyzing,
  wasmReady,
  onAnalyze,
  allFileCount,
  selectedCount,
  fileSelectorOpen,
  onFileSelectorOpenChange,
}: EditorToolbarProps) {
  return (
    <div className="flex items-center justify-between px-3 py-2 border-b h-[44px] shrink-0 bg-muted/30 overflow-hidden gap-2">
      <div className="flex items-center gap-2 min-w-0 flex-1">
        <FileSelector open={fileSelectorOpen} onOpenChange={onFileSelectorOpenChange} />
      </div>

      <div className="flex items-center gap-2 shrink-0">
        <div className="flex items-center rounded-full overflow-hidden shadow-xs">
          <Button
            onClick={onAnalyze}
            disabled={!wasmReady || isAnalyzing}
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
                disabled={!wasmReady || isAnalyzing}
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
