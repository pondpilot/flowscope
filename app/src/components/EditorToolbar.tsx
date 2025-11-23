import { FileText, Play, Loader2, ChevronDown } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
} from '@/components/ui/dropdown-menu';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { Dialect, RunMode } from '@/lib/project-store';

interface EditorToolbarProps {
  fileName: string;
  isRenaming: boolean;
  onFileNameChange: (name: string) => void;
  onRenameStart: () => void;
  onRenameEnd: () => void;
  dialect: Dialect;
  onDialectChange: (dialect: Dialect) => void;
  runMode: RunMode;
  onRunModeChange: (mode: RunMode) => void;
  isAnalyzing: boolean;
  wasmReady: boolean;
  onAnalyze: () => void;
  allFileCount: number;
  selectedCount: number;
}

export function EditorToolbar({
  fileName,
  isRenaming,
  onFileNameChange,
  onRenameStart,
  onRenameEnd,
  dialect,
  onDialectChange,
  runMode,
  onRunModeChange,
  isAnalyzing,
  wasmReady,
  onAnalyze,
  allFileCount,
  selectedCount,
}: EditorToolbarProps) {
  return (
    <div className="flex items-center justify-between px-4 py-2 border-b h-[50px] shrink-0 bg-background">
      <div className="flex items-center gap-2 overflow-hidden">
        <FileText className="h-4 w-4 text-muted-foreground shrink-0" />
        {isRenaming ? (
          <Input
            value={fileName}
            onChange={e => onFileNameChange(e.target.value)}
            onBlur={onRenameEnd}
            onKeyDown={e => e.key === 'Enter' && onRenameEnd()}
            className="h-7 w-48 text-sm"
            autoFocus
          />
        ) : (
          <span
            className="text-sm font-medium cursor-pointer hover:text-foreground text-muted-foreground truncate max-w-[200px]"
            onDoubleClick={onRenameStart}
            title="Double click to rename"
          >
            {fileName}
          </span>
        )}
      </div>

      <div className="flex items-center gap-2">
        <Select value={dialect} onValueChange={v => onDialectChange(v as Dialect)}>
          <SelectTrigger className="h-8 w-[130px] text-xs">
            <SelectValue placeholder="Dialect" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="generic">Generic SQL</SelectItem>
            <SelectItem value="postgres">PostgreSQL</SelectItem>
            <SelectItem value="snowflake">Snowflake</SelectItem>
            <SelectItem value="bigquery">BigQuery</SelectItem>
          </SelectContent>
        </Select>

        <div className="flex items-center rounded-md border shadow-sm">
          <Button
            onClick={onAnalyze}
            disabled={!wasmReady || isAnalyzing}
            size="sm"
            className="h-8 gap-2 bg-brand-blue-500 hover:bg-brand-blue-600 text-white font-medium rounded-r-none border-r border-white/20"
          >
            {isAnalyzing ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Play className="h-3.5 w-3.5 fill-current" />
            )}
            <span className="hidden sm:inline">Run</span>
            {runMode === 'all' && (
              <span className="text-xs opacity-80 ml-[-2px] hidden sm:inline">(All)</span>
            )}
            {runMode === 'custom' && (
              <span className="text-xs opacity-80 ml-[-2px] hidden sm:inline">
                ({selectedCount})
              </span>
            )}
            {runMode === 'current' && (
              <span className="text-xs opacity-80 ml-[-2px] hidden sm:inline">(Active)</span>
            )}
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                size="sm"
                className="h-8 px-2 bg-brand-blue-500 hover:bg-brand-blue-600 text-white rounded-l-none border-l border-black/10"
                disabled={!wasmReady || isAnalyzing}
              >
                <ChevronDown className="h-3 w-3" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuLabel>Run Configuration</DropdownMenuLabel>
              <DropdownMenuSeparator />
              <DropdownMenuRadioGroup value={runMode} onValueChange={v => onRunModeChange(v as RunMode)}>
                <DropdownMenuRadioItem value="all" className="text-xs">
                  Run Project ({allFileCount} files)
                </DropdownMenuRadioItem>
                <DropdownMenuRadioItem value="custom" className="text-xs">
                  Run Selected ({selectedCount} files)
                </DropdownMenuRadioItem>
                <DropdownMenuRadioItem value="current" className="text-xs">
                  Run Active File Only
                </DropdownMenuRadioItem>
              </DropdownMenuRadioGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>
    </div>
  );
}
