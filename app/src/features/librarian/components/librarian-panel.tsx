import { useCallback, useMemo, useState } from 'react';
import { ChevronDown, ChevronRight, HelpCircle, Settings, X } from 'lucide-react';
import { useLineageState } from '@pondpilot/flowscope-react';

import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';

import { useLibrarianChat } from '../hooks/use-librarian-chat';
import { processPdf } from '../services/pdf-processor';
import { embedTexts } from '../services/embedding-service';
import { useLibrarianMessages, useLibrarianPromptStats, useLibrarianStore } from '../store';
import { buildSchemaIdentifiers, type ChatReference } from '../utils/schema-identifiers';

import { AISettingsDialog } from './ai-settings-dialog';
import { ChatInput } from './chat-input';
import { ChatMessages } from './chat-messages';
import { PdfUpload } from './pdf-upload';

function formatPromptBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes.toLocaleString()} B`;
  return `${(bytes / 1024).toFixed(1)} KB`;
}

interface LibrarianPanelProps {
  onClose: () => void;
  /**
   * Called when the user clicks an assistant message that contains at least
   * one resolvable schema identifier. Receives every parsed reference so the
   * host can highlight all of them in the lineage view.
   */
  onNavigateToReferences?: (refs: ChatReference[]) => void;
}

export function LibrarianPanel({ onClose, onNavigateToReferences }: LibrarianPanelProps) {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [docsExpanded, setDocsExpanded] = useState(false);

  const messages = useLibrarianMessages();
  const promptStats = useLibrarianPromptStats();
  const isLoading = useLibrarianStore((s) => s.isLoading);
  const activeProjectId = useLibrarianStore((s) => s.activeProjectId);

  const { sendMessage } = useLibrarianChat();
  const { result } = useLineageState();
  const schemaIdentifiers = useMemo(() => buildSchemaIdentifiers(result ?? null), [result]);

  const handlePdfUpload = useCallback(async (file: File) => {
    // Capture the active project id at upload time so all three writes
    // (file, chunks, status) route to the originating project even if
    // the user switches projects while the PDF is being processed.
    const {
      activeProjectId: projectId,
      addPdfFileToProject,
      addPdfChunksToProject,
      setPdfStatusForProject,
    } = useLibrarianStore.getState();
    if (!projectId) return;

    const fileId = crypto.randomUUID();
    addPdfFileToProject(projectId, {
      id: fileId,
      name: file.name,
      size: file.size,
      status: 'processing',
      uploadedAt: Date.now(),
    });

    try {
      const chunks = await processPdf(file, fileId, embedTexts);
      addPdfChunksToProject(projectId, chunks);
      setPdfStatusForProject(projectId, fileId, 'ready');
    } catch (err) {
      console.error('[Librarian] PDF processing failed:', err);
      const message = err instanceof Error ? err.message : 'Failed to process PDF';
      setPdfStatusForProject(projectId, fileId, 'error', message);
    }
  }, []);

  return (
    <div className="flex h-full min-w-0 flex-col overflow-hidden" data-testid="librarian-panel">
      {/* Header */}
      <div className="flex items-center justify-between border-b px-3 h-[44px] shrink-0">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium">Librarian</span>
        </div>
        <div className="flex items-center gap-1">
          <Popover>
            <PopoverTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                aria-label="About Librarian"
                data-testid="help-button"
              >
                <HelpCircle className="h-4 w-4" />
              </Button>
            </PopoverTrigger>
            <PopoverContent align="end" className="w-80 text-sm" data-testid="help-popover">
              <p>
                Hi, I&apos;m Librarian! I answer questions about your data structure using your
                database schema and uploaded technical documentation.
              </p>
              <p className="mt-2 font-medium">How to use:</p>
              <ul className="mt-1 list-disc space-y-1 pl-5 text-muted-foreground">
                <li>Configure your AI provider in Settings (⚙)</li>
                <li>Upload relevant PDF docs (optional)</li>
                <li>Ask questions about your data</li>
              </ul>
            </PopoverContent>
          </Popover>
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7"
                  onClick={() => setSettingsOpen(true)}
                  aria-label="AI Settings"
                  data-testid="settings-button"
                >
                  <Settings className="h-4 w-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>AI Settings</TooltipContent>
            </Tooltip>
          </TooltipProvider>
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7"
                  onClick={onClose}
                  aria-label="Close Librarian"
                  data-testid="close-button"
                >
                  <X className="h-4 w-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>Close</TooltipContent>
            </Tooltip>
          </TooltipProvider>
        </div>
      </div>

      {/* Chat messages */}
      <ChatMessages
        messages={messages}
        isLoading={isLoading}
        schemaIdentifiers={schemaIdentifiers}
        onNavigateToReferences={onNavigateToReferences}
      />

      {/* Collapsible docs section */}
      <div className="border-t">
        <button
          className="flex w-full items-center gap-2 px-3 py-2 text-xs text-muted-foreground hover:bg-muted/50"
          onClick={() => setDocsExpanded((prev) => !prev)}
          data-testid="docs-toggle"
        >
          {docsExpanded ? (
            <ChevronDown className="h-3 w-3" />
          ) : (
            <ChevronRight className="h-3 w-3" />
          )}
          Documentation
        </button>
        {docsExpanded && (
          <div className="min-w-0 overflow-hidden px-3 pb-2" data-testid="docs-section">
            <PdfUpload onUpload={handlePdfUpload} />
          </div>
        )}
      </div>

      {promptStats && (
        <div
          className="border-t px-3 py-1.5 text-xs text-muted-foreground"
          data-testid="last-prompt-size"
        >
          Last prompt: {promptStats.characters.toLocaleString()} chars /{' '}
          {formatPromptBytes(promptStats.bytes)}
        </div>
      )}

      {/* Chat input */}
      <ChatInput onSend={sendMessage} disabled={isLoading} noActiveProject={!activeProjectId} />

      {/* Settings dialog */}
      <AISettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
    </div>
  );
}
