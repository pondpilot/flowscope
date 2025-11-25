import { useState, useMemo } from 'react';
import { Share2 } from 'lucide-react';
import { Button } from './ui/button';
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from './ui/resizable';

import { EditorArea } from './EditorArea';
import { AnalysisView } from './AnalysisView';
import { ProjectSelector } from './ProjectSelector';
import { ShareDialog } from './ShareDialog';
import { useProject } from '@/lib/project-store';
import { useGlobalShortcuts } from '@/hooks';
import type { GlobalShortcut } from '@/hooks';

interface WorkspaceProps {
  wasmReady: boolean;
  error: string | null;
  onRetry?: () => void;
  isRetrying?: boolean;
}

/**
 * Main workspace component containing the two-panel layout:
 * - Left: SQL editor with file selector
 * - Right: Lineage visualization
 */
export function Workspace({ wasmReady, error, onRetry, isRetrying }: WorkspaceProps) {
  const { currentProject } = useProject();
  const [fileSelectorOpen, setFileSelectorOpen] = useState(false);
  const [projectSelectorOpen, setProjectSelectorOpen] = useState(false);
  const [dialectSelectorOpen, setDialectSelectorOpen] = useState(false);
  const [shareDialogOpen, setShareDialogOpen] = useState(false);

  // Global keyboard shortcuts
  const shortcuts = useMemo<GlobalShortcut[]>(() => [
    {
      key: 'o',
      cmdOrCtrl: true,
      handler: () => setFileSelectorOpen(prev => !prev),
    },
    {
      key: 'p',
      cmdOrCtrl: true,
      handler: () => setProjectSelectorOpen(prev => !prev),
    },
    {
      key: 'd',
      cmdOrCtrl: true,
      handler: () => setDialectSelectorOpen(prev => !prev),
    },
  ], []);

  useGlobalShortcuts(shortcuts);

  return (
    <div className="flex flex-col h-svh">
      {/* App Header */}
      <header
        className="flex items-center justify-between px-4 h-12 border-b border-border bg-background shrink-0"
        data-testid="app-header"
      >
        <div className="flex items-center gap-2">
          {/* Logo */}
          <div className="flex items-center gap-2">
            <div className="h-6 w-6 bg-brand-blue-500 rounded flex items-center justify-center text-white font-bold text-xs">
              FS
            </div>
            <span className="text-xs font-medium italic text-orange-500 hidden sm:inline-block">
              BETA
            </span>
          </div>

          {/* Project Selector */}
          <ProjectSelector
            open={projectSelectorOpen}
            onOpenChange={setProjectSelectorOpen}
          />
        </div>

        {/* Share Button */}
        {currentProject && (
          <Button
            variant="ghost"
            size="sm"
            className="h-8 gap-1.5"
            onClick={() => setShareDialogOpen(true)}
          >
            <Share2 className="h-3.5 w-3.5" />
            <span className="hidden sm:inline">Share</span>
          </Button>
        )}
      </header>

      {/* Share Dialog */}
      {currentProject && (
        <ShareDialog
          open={shareDialogOpen}
          onOpenChange={setShareDialogOpen}
          project={currentProject}
        />
      )}

      {/* Global Error Banner */}
      {error && (
        <div
          className="px-4 py-2 bg-destructive/10 text-destructive text-xs font-medium border-b border-destructive/20 flex items-center justify-center gap-3"
          data-testid="error-banner"
        >
          <span>System Error: {error}</span>
          {onRetry && (
            <Button
              variant="outline"
              size="sm"
              onClick={onRetry}
              disabled={isRetrying}
              className="h-6 text-xs"
              data-testid="retry-btn"
            >
              {isRetrying ? 'Retrying...' : 'Retry'}
            </Button>
          )}
        </div>
      )}

      {/* Main Split View - 2 columns */}
      <div className="flex-1 overflow-hidden">
        <ResizablePanelGroup direction="horizontal">
          {/* Left: Editor */}
          <ResizablePanel defaultSize={33} minSize={20} data-testid="editor-panel">
            <EditorArea
              wasmReady={wasmReady}
              fileSelectorOpen={fileSelectorOpen}
              onFileSelectorOpenChange={setFileSelectorOpen}
              dialectSelectorOpen={dialectSelectorOpen}
              onDialectSelectorOpenChange={setDialectSelectorOpen}
            />
          </ResizablePanel>

          <ResizableHandle withHandle />

          {/* Right: Visualization */}
          <ResizablePanel defaultSize={67} minSize={30} data-testid="analysis-panel">
            <AnalysisView />
          </ResizablePanel>
        </ResizablePanelGroup>
      </div>
    </div>
  );
}
