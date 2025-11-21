import { useState } from 'react';
import { Button } from './ui/button';
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from './ui/resizable';
import { PanelLeftClose, PanelLeftOpen } from 'lucide-react';

import { useProject } from '@/lib/project-store';
import { FileExplorer } from './FileExplorer';
import { EditorArea } from './EditorArea';
import { AnalysisView } from './AnalysisView';

interface WorkspaceProps {
  wasmReady: boolean;
  error: string | null;
  onRetry?: () => void;
  isRetrying?: boolean;
}

/**
 * Main workspace component containing the three-panel layout:
 * - Left: Project file explorer
 * - Middle: SQL editor
 * - Right: Lineage visualization
 */
export function Workspace({ wasmReady, error, onRetry, isRetrying }: WorkspaceProps) {
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const { currentProject } = useProject();

  return (
    <div className="flex flex-col h-full">
      {/* App Header - Slim & Professional */}
      <header className="flex items-center justify-between px-4 h-12 border-b border-border bg-background z-10 shrink-0">
        <div className="flex items-center gap-3">
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 -ml-2 text-muted-foreground"
            onClick={() => setSidebarCollapsed(!sidebarCollapsed)}
            title={sidebarCollapsed ? "Open Sidebar" : "Collapse Sidebar"}
            aria-label={sidebarCollapsed ? "Open Sidebar" : "Collapse Sidebar"}
          >
             {sidebarCollapsed ? <PanelLeftOpen className="h-4 w-4" /> : <PanelLeftClose className="h-4 w-4" />}
          </Button>

          <div className="flex items-center gap-2">
            <div className="h-6 w-6 bg-brand-blue-500 rounded flex items-center justify-center text-white font-bold text-xs">
              FS
            </div>
            <span className="text-sm font-semibold text-foreground hidden sm:inline-block">
              FlowScope
            </span>
            {currentProject && (
               <span className="text-sm text-muted-foreground border-l pl-3 ml-1">
                 {currentProject.name}
               </span>
            )}
          </div>
        </div>

      </header>

      {/* Global Error Banner */}
      {error && (
        <div className="px-4 py-2 bg-destructive/10 text-destructive text-xs font-medium border-b border-destructive/20 flex items-center justify-center gap-3">
          <span>System Error: {error}</span>
          {onRetry && (
            <Button
              variant="outline"
              size="sm"
              onClick={onRetry}
              disabled={isRetrying}
              className="h-6 text-xs"
            >
              {isRetrying ? 'Retrying...' : 'Retry'}
            </Button>
          )}
        </div>
      )}

      {/* Main Split View */}
      <div className="flex-1 overflow-hidden">
        <ResizablePanelGroup direction="horizontal">
          {/* Left: Project Sidebar */}
          <ResizablePanel
            defaultSize={20}
            minSize={15}
            maxSize={30}
            collapsible={true}
            collapsedSize={0}
            onCollapse={() => setSidebarCollapsed(true)}
            onExpand={() => setSidebarCollapsed(false)}
            className={`bg-muted/5 transition-all duration-300 ${sidebarCollapsed ? '!flex-none !w-0 border-none' : ''}`}
          >
            {!sidebarCollapsed && <FileExplorer />}
          </ResizablePanel>

          <ResizableHandle withHandle className={sidebarCollapsed ? 'hidden' : ''} />

          {/* Middle: Editor */}
          <ResizablePanel defaultSize={40} minSize={20}>
             <EditorArea wasmReady={wasmReady} className="border-r" />
          </ResizablePanel>

          <ResizableHandle withHandle />

          {/* Right: Visualization */}
          <ResizablePanel defaultSize={40} minSize={20}>
            <AnalysisView />
          </ResizablePanel>
        </ResizablePanelGroup>
      </div>
    </div>
  );
}
