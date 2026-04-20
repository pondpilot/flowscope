import { useCallback, useMemo, useState } from 'react';
import { BookOpen, ChevronDown, ChevronRight, HelpCircle, Settings, X } from 'lucide-react';
import { useLineageState } from '@pondpilot/flowscope-react';

import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';

import { useLibrarianChat } from '../hooks/use-librarian-chat';
import { processPdf } from '../services/pdf-processor';
import { embedTexts } from '../services/embedding-service';
import { useLibrarianStore } from '../store';
import { buildSchemaIdentifiers } from '../utils/schema-identifiers';

import { AISettingsDialog } from './ai-settings-dialog';
import { ChatInput } from './chat-input';
import { ChatMessages } from './chat-messages';
import { PdfUpload } from './pdf-upload';

interface LibrarianPanelProps {
  onClose: () => void;
  /**
   * Called when the user clicks an assistant message that references a
   * schema table (directly or via a column). The host wires this to the
   * schema tab navigation.
   */
  onNavigateToTable?: (tableName: string) => void;
}

export function LibrarianPanel({ onClose, onNavigateToTable }: LibrarianPanelProps) {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [docsExpanded, setDocsExpanded] = useState(false);

  const messages = useLibrarianStore((s) => s.messages);
  const isLoading = useLibrarianStore((s) => s.isLoading);
  const addPdfFile = useLibrarianStore((s) => s.addPdfFile);
  const addPdfChunks = useLibrarianStore((s) => s.addPdfChunks);
  const setPdfStatus = useLibrarianStore((s) => s.setPdfStatus);

  const { sendMessage } = useLibrarianChat();
  const { result } = useLineageState();
  const schemaIdentifiers = useMemo(() => buildSchemaIdentifiers(result ?? null), [result]);

  const handlePdfUpload = useCallback(
    async (file: File) => {
      const fileId = crypto.randomUUID();
      addPdfFile({
        id: fileId,
        name: file.name,
        size: file.size,
        status: 'processing',
        uploadedAt: Date.now(),
      });

      try {
        const chunks = await processPdf(file, fileId, embedTexts);
        addPdfChunks(chunks);
        setPdfStatus(fileId, 'ready');
      } catch (err) {
        console.error('[Librarian] PDF processing failed:', err);
        const message = err instanceof Error ? err.message : 'Failed to process PDF';
        setPdfStatus(fileId, 'error', message);
      }
    },
    [addPdfFile, addPdfChunks, setPdfStatus]
  );

  return (
    <div className="flex h-full flex-col" data-testid="librarian-panel">
      {/* Header */}
      <div className="flex items-center justify-between border-b px-3 h-12 shrink-0">
        <div className="flex items-center gap-2">
          <BookOpen className="h-4 w-4 text-muted-foreground" />
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
        onNavigateToTable={onNavigateToTable}
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
          <div className="px-3 pb-2" data-testid="docs-section">
            <PdfUpload onUpload={handlePdfUpload} />
          </div>
        )}
      </div>

      {/* Chat input */}
      <ChatInput onSend={sendMessage} disabled={isLoading} />

      {/* Settings dialog */}
      <AISettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
    </div>
  );
}
