import { useCallback, useMemo, useState } from 'react';
import { BookOpen, ChevronDown, ChevronRight, Settings, X } from 'lucide-react';
import { useLineageState } from '@pondpilot/flowscope-react';

import { Button } from '@/components/ui/button';
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
}

export function LibrarianPanel({ onClose }: LibrarianPanelProps) {
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
