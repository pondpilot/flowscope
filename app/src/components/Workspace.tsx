import { useState, useMemo, useRef, useCallback, useEffect } from 'react';
import { Share2, Github } from 'lucide-react';
import { toast } from 'sonner';
import { useLineageActions, useLineageState } from '@pondpilot/flowscope-react';
import { Button } from './ui/button';
import { FlowScopeLogo } from './FlowScopeLogo';
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from './ui/resizable';
import type { ImperativePanelHandle } from 'react-resizable-panels';

import { EditorArea } from './EditorArea';
import { AnalysisView } from './AnalysisView';
import { ProjectSelector } from './ProjectSelector';
import { ShareDialog } from './ShareDialog';
import { ExportDialog } from './ExportDialog';
import { ThemeToggle } from './ThemeToggle';
import { KeyboardShortcutsDialog } from './KeyboardShortcutsDialog';
import { CommandPalette } from './CommandPalette';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from './ui/tooltip';
import { useProject } from '@/lib/project-store';
import { NavigationProvider } from '@/lib/navigation-context';
import { FocusRegistryProvider } from '@/lib/focus-registry';
import { useGlobalShortcuts } from '@/hooks';
import type { GlobalShortcut } from '@/hooks';
import { useThemeStore, type Theme } from '@/lib/theme-store';
import { useViewStateStore } from '@/lib/view-state-store';
import { getShortcutDisplay } from '@/lib/shortcuts';

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
const EDITOR_PANEL_DEFAULT_SIZE = 33;

export function Workspace({ wasmReady, error, onRetry, isRetrying }: WorkspaceProps) {
  const { currentProject, selectFile, activeProjectId } = useProject();
  const lineageActions = useLineageActions();
  const { highlightSpan, setViewMode, toggleColumnEdges, setAllNodesCollapsed, toggleShowScriptTables, setLayoutAlgorithm } = lineageActions;
  const lineageState = useLineageState();
  const { result, viewMode, layoutAlgorithm } = lineageState;
  const [fileSelectorOpen, setFileSelectorOpen] = useState(false);
  const [projectSelectorOpen, setProjectSelectorOpen] = useState(false);
  const [dialectSelectorOpen, setDialectSelectorOpen] = useState(false);
  const [templateSelectorOpen, setTemplateSelectorOpen] = useState(false);
  const [shareDialogOpen, setShareDialogOpen] = useState(false);
  const [shortcutsDialogOpen, setShortcutsDialogOpen] = useState(false);
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);

  // Theme cycling for keyboard shortcut
  const { theme, setTheme } = useThemeStore();
  const cycleTheme = useCallback(() => {
    const themes: Theme[] = ['light', 'dark', 'system'];
    const currentIndex = themes.indexOf(theme);
    const nextTheme = themes[(currentIndex + 1) % themes.length];
    setTheme(nextTheme);
    toast.success(`Theme: ${nextTheme.charAt(0).toUpperCase() + nextTheme.slice(1)}`);
  }, [theme, setTheme]);

  const editorPanelRef = useRef<ImperativePanelHandle>(null);
  const graphContainerRef = useRef<HTMLDivElement>(null);

  // Use ref for currentProject to avoid recreating callback on every project change
  const currentProjectRef = useRef(currentProject);
  useEffect(() => {
    currentProjectRef.current = currentProject;
  }, [currentProject]);

  // Handler for navigating to a file in the editor
  // Uses ref to avoid dependency on currentProject object which changes frequently
  const handleNavigateToEditor = useCallback((sourceName: string, span?: { start: number; end: number }) => {
    if (!sourceName?.trim()) {
      return;
    }

    const project = currentProjectRef.current;
    if (!project?.files) {
      toast.error('Cannot open file', {
        description: 'No project is currently loaded',
      });
      return;
    }

    // Normalize the source name for comparison (handle different path separators)
    const normalizeForComparison = (path: string) => path.replace(/\\/g, '/').toLowerCase();
    const normalizedSource = normalizeForComparison(sourceName);

    // Find the file by name - try exact match first, then by path
    let file = project.files.find(f => f.name === sourceName);
    if (!file) {
      // Try matching by path (sourceName might be a path)
      file = project.files.find(f => f.path === sourceName);
    }
    if (!file) {
      // Try normalized path match (handle different path separators and case)
      file = project.files.find(f =>
        normalizeForComparison(f.name) === normalizedSource ||
        normalizeForComparison(f.path) === normalizedSource
      );
    }
    if (!file) {
      // Try partial match (sourceName might be just filename without path)
      file = project.files.find(f =>
        normalizeForComparison(f.name).endsWith(normalizedSource) ||
        normalizeForComparison(f.path).endsWith(normalizedSource)
      );
    }

    if (!file) {
      if (import.meta.env.DEV) {
        console.warn(
          `[Workspace] Cannot navigate to editor: file "${sourceName}" not found. Available files:`,
          project.files.map(f => ({ name: f.name, path: f.path }))
        );
      }
      toast.error('File not found', {
        description: `Could not locate "${sourceName}" in the project`,
      });
      return;
    }

    selectFile(file.id);
    // Expand the editor panel if collapsed
    if (editorPanelRef.current?.isCollapsed()) {
      editorPanelRef.current.expand();
    }
    // Highlight the span when opening the editor from navigation actions
    if (span) {
      highlightSpan(span);
    }
  }, [selectFile, highlightSpan]);

  const toggleEditorPanel = useCallback(() => {
    const panel = editorPanelRef.current;
    if (!panel) return;

    if (panel.isCollapsed()) {
      panel.expand();
    } else {
      panel.collapse();
    }
  }, []);

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
    {
      key: 'b',
      cmdOrCtrl: true,
      handler: toggleEditorPanel,
    },
    // Help dialog
    {
      key: '?',
      handler: () => setShortcutsDialogOpen(true),
    },
    // Share dialog
    {
      key: 's',
      cmdOrCtrl: true,
      shift: true,
      handler: () => {
        if (currentProject) {
          setShareDialogOpen(true);
        }
      },
    },
    // Theme toggle (Cmd+\ to avoid browser conflict with Cmd+Shift+T)
    {
      key: '\\',
      cmdOrCtrl: true,
      handler: cycleTheme,
    },
    // Command palette
    {
      key: 'k',
      cmdOrCtrl: true,
      handler: () => setCommandPaletteOpen(true),
    },
  ], [toggleEditorPanel, currentProject, cycleTheme]);

  useGlobalShortcuts(shortcuts);

  // View state store for tab switching via command palette
  const setActiveTab = useViewStateStore(s => s.setActiveTab);
  const getActiveTab = useViewStateStore(s => s.getActiveTab);
  const currentActiveTab = activeProjectId ? getActiveTab(activeProjectId) : 'lineage';

  // Command palette handler
  const handleExecuteCommand = useCallback((commandId: string) => {
    switch (commandId) {
      // Navigation
      case 'help':
        setShortcutsDialogOpen(true);
        break;
      case 'command-palette':
        setCommandPaletteOpen(true);
        break;
      case 'open-files':
        setFileSelectorOpen(true);
        break;
      case 'open-projects':
        setProjectSelectorOpen(true);
        break;
      case 'open-dialect':
        setDialectSelectorOpen(true);
        break;
      case 'toggle-editor':
        toggleEditorPanel();
        break;

      // Tab switching
      case 'tab-lineage':
        if (activeProjectId) setActiveTab(activeProjectId, 'lineage');
        break;
      case 'tab-hierarchy':
        if (activeProjectId) setActiveTab(activeProjectId, 'hierarchy');
        break;
      case 'tab-matrix':
        if (activeProjectId) setActiveTab(activeProjectId, 'matrix');
        break;
      case 'tab-schema':
        if (activeProjectId) setActiveTab(activeProjectId, 'schema');
        break;
      case 'tab-issues':
        if (activeProjectId) setActiveTab(activeProjectId, 'issues');
        break;

      // Actions
      case 'share':
        if (currentProject) setShareDialogOpen(true);
        break;

      // Settings
      case 'toggle-theme':
        cycleTheme();
        break;

      // Lineage view commands - execute via lineage actions
      case 'toggle-view-mode':
        setViewMode(viewMode === 'table' ? 'script' : 'table');
        break;
      case 'toggle-column-edges':
        toggleColumnEdges();
        break;
      case 'expand-all':
        setAllNodesCollapsed(false);
        break;
      case 'collapse-all':
        setAllNodesCollapsed(true);
        break;
      case 'toggle-script-tables':
        toggleShowScriptTables();
        break;
      case 'cycle-layout':
        setLayoutAlgorithm(layoutAlgorithm === 'dagre' ? 'elk' : 'dagre');
        break;
      case 'focus-search':
        // Focus search is context-dependent, use keyboard shortcut
        toast.info('Press / to focus search');
        break;

      default:
        console.warn(`Unknown command: ${commandId}`);
    }
  }, [
    toggleEditorPanel,
    activeProjectId,
    setActiveTab,
    currentProject,
    cycleTheme,
    setViewMode,
    viewMode,
    toggleColumnEdges,
    setAllNodesCollapsed,
    toggleShowScriptTables,
    setLayoutAlgorithm,
    layoutAlgorithm,
  ]);

  return (
    <div className="flex flex-col h-svh">
      {/* App Header */}
      <header
        className="flex items-center justify-between px-4 h-12 border-b border-border bg-background shrink-0"
        data-testid="app-header"
      >
        <div className="flex items-center gap-2">
          {/* Logo */}
          <div className="flex items-center gap-3">
            <FlowScopeLogo className="w-8 h-8 text-foreground/30 dark:text-white/30" />
            <div className="flex items-baseline gap-1">
              <span className="text-lg font-semibold text-foreground">
                FlowScope
              </span>
              <span className="text-xs font-mono text-muted-foreground">
                Beta
              </span>
            </div>
          </div>

          {/* Project Selector */}
          <ProjectSelector
            open={projectSelectorOpen}
            onOpenChange={setProjectSelectorOpen}
          />
        </div>

        {/* Header Actions */}
        <div className="flex items-center gap-1">
          {currentProject && (
            <>
              <ExportDialog
                result={result}
                projectName={currentProject.name}
                graphRef={graphContainerRef}
              />
              <TooltipProvider>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-8 w-8"
                      onClick={() => setShareDialogOpen(true)}
                    >
                      <Share2 className="h-4 w-4" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>
                    <p className="flex items-center gap-2">
                      Share project
                      <kbd className="px-1.5 py-0.5 text-xs bg-muted rounded border font-mono">{getShortcutDisplay('share')}</kbd>
                    </p>
                  </TooltipContent>
                </Tooltip>
              </TooltipProvider>
            </>
          )}
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8"
                  asChild
                >
                  <a
                    href="https://github.com/pondpilot/flowscope"
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    <Github className="h-4 w-4" />
                  </a>
                </Button>
              </TooltipTrigger>
              <TooltipContent>
                <p>View on GitHub</p>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
          <ThemeToggle />
        </div>
      </header>

      {/* Share Dialog */}
      {currentProject && (
        <ShareDialog
          open={shareDialogOpen}
          onOpenChange={setShareDialogOpen}
          project={currentProject}
        />
      )}

      {/* Keyboard Shortcuts Help Dialog */}
      <KeyboardShortcutsDialog
        open={shortcutsDialogOpen}
        onOpenChange={setShortcutsDialogOpen}
        activeTab={currentActiveTab}
      />

      {/* Command Palette */}
      <CommandPalette
        open={commandPaletteOpen}
        onOpenChange={setCommandPaletteOpen}
        onExecuteCommand={handleExecuteCommand}
      />

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
      <NavigationProvider projectId={activeProjectId} onNavigateToEditor={handleNavigateToEditor}>
        <FocusRegistryProvider>
        <div className="flex-1 overflow-hidden">
          <ResizablePanelGroup direction="horizontal">
            {/* Left: Editor */}
            <ResizablePanel
              ref={editorPanelRef}
              defaultSize={EDITOR_PANEL_DEFAULT_SIZE}
              minSize={25}
              collapsible
              collapsedSize={0}
              data-testid="editor-panel"
            >
              <EditorArea
                wasmReady={wasmReady}
                fileSelectorOpen={fileSelectorOpen}
                onFileSelectorOpenChange={setFileSelectorOpen}
                dialectSelectorOpen={dialectSelectorOpen}
                onDialectSelectorOpenChange={setDialectSelectorOpen}
                templateSelectorOpen={templateSelectorOpen}
                onTemplateSelectorOpenChange={setTemplateSelectorOpen}
              />
            </ResizablePanel>

            <ResizableHandle withHandle />

            {/* Right: Visualization */}
            <ResizablePanel
              defaultSize={67}
              minSize={30}
              collapsible
              collapsedSize={0}
              data-testid="analysis-panel"
            >
              <AnalysisView graphContainerRef={graphContainerRef} />
            </ResizablePanel>
          </ResizablePanelGroup>
        </div>
        </FocusRegistryProvider>
      </NavigationProvider>
    </div>
  );
}
