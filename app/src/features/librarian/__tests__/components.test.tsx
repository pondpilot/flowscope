/// <reference types="@testing-library/jest-dom" />
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { MAX_MESSAGE_LENGTH, MAX_PDF_SIZE_MB } from '../constants';
import { useLibrarianStore } from '../store';
import type { ChatMessage, PdfFile } from '../types';

// ---------- Mocks ----------

vi.mock('../services/ai-service', () => ({
  loadAIConfig: vi.fn(() => ({
    provider: 'openai',
    apiKey: 'sk-test',
    model: 'gpt-4o',
  })),
  saveAIConfig: vi.fn(),
  sendChatMessage: vi.fn(() => Promise.resolve('ok')),
  getDefaultModel: vi.fn((p: string) => (p === 'openai' ? 'gpt-4o' : 'claude-sonnet-4-20250514')),
}));

import {
  getDefaultModel,
  loadAIConfig,
  saveAIConfig,
  sendChatMessage,
} from '../services/ai-service';
import { DEFAULT_LIBRARIAN_SYSTEM_PROMPT } from '../services/context-builder';

// ---------- Helpers ----------

function makeMessage(overrides: Partial<ChatMessage> = {}): ChatMessage {
  return {
    id: crypto.randomUUID(),
    role: 'user',
    content: 'hello',
    timestamp: Date.now(),
    ...overrides,
  };
}

function makePdfFile(overrides: Partial<PdfFile> = {}): PdfFile {
  return {
    id: 'file-1',
    name: 'test.pdf',
    size: 1024,
    status: 'ready',
    uploadedAt: Date.now(),
    ...overrides,
  };
}

// ---------- Components under test ----------

import { AISettingsDialog } from '../components/ai-settings-dialog';
import { ChatInput } from '../components/chat-input';
import { ChatMessages } from '../components/chat-messages';
import { PdfUpload } from '../components/pdf-upload';
import type { SchemaIdentifiers } from '../utils/schema-identifiers';

function makeSchema(tables: string[], columns: string[]): SchemaIdentifiers {
  return {
    tables: new Set(tables),
    columns: new Set(columns),
    columnOwners: new Map(),
  };
}

// ---------- Shared setup ----------

beforeEach(() => {
  localStorage.clear();
  useLibrarianStore.setState({
    byProject: { 'proj-1': { messages: [], pdfFiles: [], pdfChunks: [] } },
    activeProjectId: 'proj-1',
    isLoading: false,
    hasConfig: true,
  });
  vi.clearAllMocks();
  // Restore default mock return values after clearAllMocks
  vi.mocked(loadAIConfig).mockReturnValue({
    provider: 'openai',
    apiKey: 'sk-test',
    model: 'gpt-4o',
  });
  vi.mocked(sendChatMessage).mockResolvedValue('ok');
  vi.mocked(getDefaultModel).mockImplementation((p: string) =>
    p === 'openai' ? 'gpt-4o' : 'claude-sonnet-4-20250514'
  );
});

afterEach(() => {
  cleanup();
});

// ============================================================================
// AISettingsDialog
// ============================================================================

describe('AISettingsDialog', () => {
  it('renders when open', () => {
    render(<AISettingsDialog open={true} onOpenChange={vi.fn()} />);
    expect(screen.getByText('AI Settings')).toBeInTheDocument();
  });

  it('does not render when closed', () => {
    render(<AISettingsDialog open={false} onOpenChange={vi.fn()} />);
    expect(screen.queryByText('AI Settings')).not.toBeInTheDocument();
  });

  it('shows provider selector', () => {
    render(<AISettingsDialog open={true} onOpenChange={vi.fn()} />);
    expect(screen.getByText('Provider')).toBeInTheDocument();
  });

  it('shows API key input', () => {
    render(<AISettingsDialog open={true} onOpenChange={vi.fn()} />);
    expect(screen.getByLabelText('API Key')).toBeInTheDocument();
  });

  it('shows model input', () => {
    render(<AISettingsDialog open={true} onOpenChange={vi.fn()} />);
    expect(screen.getByLabelText('Model')).toBeInTheDocument();
  });

  it('shows editable system prompt with size', () => {
    render(<AISettingsDialog open={true} onOpenChange={vi.fn()} />);
    const promptInput = screen.getByTestId('system-prompt-textarea') as HTMLTextAreaElement;
    expect(promptInput.value).toContain('expert on SQL lineage and data flow');
    expect(screen.getByTestId('prompt-size')).toHaveTextContent(/chars/);
  });

  it('loads existing config on open', () => {
    render(<AISettingsDialog open={true} onOpenChange={vi.fn()} />);
    const apiKeyInput = screen.getByLabelText('API Key') as HTMLInputElement;
    expect(apiKeyInput.value).toBe('sk-test');
  });

  it('calls saveAIConfig on save', () => {
    render(<AISettingsDialog open={true} onOpenChange={vi.fn()} />);
    fireEvent.click(screen.getByText('Save'));
    expect(saveAIConfig).toHaveBeenCalled();
  });

  it('saves a custom system prompt override', () => {
    render(<AISettingsDialog open={true} onOpenChange={vi.fn()} />);
    fireEvent.change(screen.getByTestId('system-prompt-textarea'), {
      target: { value: 'Custom Librarian instructions' },
    });
    fireEvent.click(screen.getByText('Save'));
    expect(saveAIConfig).toHaveBeenCalledWith(
      expect.objectContaining({ systemPrompt: 'Custom Librarian instructions' })
    );
  });

  it('resets the system prompt to the default', () => {
    render(<AISettingsDialog open={true} onOpenChange={vi.fn()} />);
    const promptInput = screen.getByTestId('system-prompt-textarea') as HTMLTextAreaElement;
    fireEvent.change(promptInput, { target: { value: 'Custom Librarian instructions' } });
    fireEvent.click(screen.getByText('Reset to default'));
    expect(promptInput.value).toBe(DEFAULT_LIBRARIAN_SYSTEM_PROMPT);
  });

  it('calls refreshConfig after save to update store', () => {
    vi.mocked(loadAIConfig).mockReturnValue({
      provider: 'openai',
      apiKey: 'sk-new-key',
      model: 'gpt-4o',
    });
    useLibrarianStore.setState({ hasConfig: false });
    render(<AISettingsDialog open={true} onOpenChange={vi.fn()} />);
    fireEvent.click(screen.getByText('Save'));
    expect(useLibrarianStore.getState().hasConfig).toBe(true);
  });

  it('disables save when API key is empty', () => {
    vi.mocked(loadAIConfig).mockReturnValue(null);
    render(<AISettingsDialog open={true} onOpenChange={vi.fn()} />);
    expect(screen.getByText('Save')).toBeDisabled();
  });

  it('tests connection', async () => {
    render(<AISettingsDialog open={true} onOpenChange={vi.fn()} />);
    // Wait for useEffect to load config and enable button
    await waitFor(() => {
      expect(screen.getByText('Test Connection')).not.toBeDisabled();
    });
    fireEvent.click(screen.getByText('Test Connection'));
    await waitFor(() => {
      expect(screen.getByTestId('test-result')).toHaveTextContent('Connection successful');
    });
  });

  it('shows error on failed connection test', async () => {
    vi.mocked(sendChatMessage).mockRejectedValueOnce(new Error('Invalid API key'));
    render(<AISettingsDialog open={true} onOpenChange={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByText('Test Connection')).not.toBeDisabled();
    });
    fireEvent.click(screen.getByText('Test Connection'));
    await waitFor(() => {
      expect(screen.getByTestId('test-result')).toHaveTextContent('Invalid API key');
    });
  });
});

// ============================================================================
// ChatMessages
// ============================================================================

describe('ChatMessages', () => {
  it('renders empty state when no messages', () => {
    render(<ChatMessages messages={[]} isLoading={false} />);
    expect(screen.getByTestId('empty-state')).toBeInTheDocument();
    expect(screen.getByText(/Ask questions about your data/)).toBeInTheDocument();
  });

  it('does not show empty state when loading', () => {
    render(<ChatMessages messages={[]} isLoading={true} />);
    expect(screen.queryByTestId('empty-state')).not.toBeInTheDocument();
  });

  it('renders user messages', () => {
    const messages = [makeMessage({ role: 'user', content: 'Hello there' })];
    render(<ChatMessages messages={messages} isLoading={false} />);
    expect(screen.getByText('Hello there')).toBeInTheDocument();
    expect(screen.getByTestId('message-user')).toBeInTheDocument();
  });

  it('renders assistant messages', () => {
    const messages = [makeMessage({ role: 'assistant', content: 'Hi back' })];
    render(<ChatMessages messages={messages} isLoading={false} />);
    expect(screen.getByText('Hi back')).toBeInTheDocument();
    expect(screen.getByTestId('message-assistant')).toBeInTheDocument();
  });

  it('renders loading indicator', () => {
    render(<ChatMessages messages={[]} isLoading={true} />);
    expect(screen.getByTestId('loading-indicator')).toBeInTheDocument();
  });

  it('renders code blocks', () => {
    const messages = [
      makeMessage({
        role: 'assistant',
        content: 'Here is code:\n```sql\nSELECT * FROM t\n```',
      }),
    ];
    render(<ChatMessages messages={messages} isLoading={false} />);
    expect(screen.getByText('SELECT * FROM t')).toBeInTheDocument();
  });

  it('renders multiple messages in order', () => {
    const messages = [
      makeMessage({ id: '1', role: 'user', content: 'Question' }),
      makeMessage({ id: '2', role: 'assistant', content: 'Answer' }),
    ];
    render(<ChatMessages messages={messages} isLoading={false} />);
    expect(screen.getByText('Question')).toBeInTheDocument();
    expect(screen.getByText('Answer')).toBeInTheDocument();
  });

  it('wraps schema identifiers in assistant messages with the identifier class', () => {
    const schema = makeSchema([], ['MANDT']);
    const messages = [
      makeMessage({ role: 'assistant', content: 'The client column is MANDT in this table.' }),
    ];
    const { container } = render(
      <ChatMessages messages={messages} isLoading={false} schemaIdentifiers={schema} />
    );

    const idSpan = container.querySelector('[data-identifier="MANDT"]');
    expect(idSpan).not.toBeNull();
    expect(idSpan).toHaveTextContent('MANDT');
    expect(idSpan?.className).toContain('font-mono');
    expect(idSpan?.className).toContain('text-primary');
    expect(idSpan?.className).toContain('font-medium');
  });

  it('does not style surrounding text as an identifier', () => {
    const schema = makeSchema([], ['MANDT']);
    const messages = [makeMessage({ role: 'assistant', content: 'The client column is MANDT.' })];
    const { container } = render(
      <ChatMessages messages={messages} isLoading={false} schemaIdentifiers={schema} />
    );

    const all = container.querySelectorAll('[data-identifier]');
    expect(all).toHaveLength(1);
    expect(all[0]).toHaveTextContent('MANDT');
  });

  it('does not style identifiers in user messages', () => {
    const schema = makeSchema([], ['MANDT']);
    const messages = [makeMessage({ role: 'user', content: 'What is MANDT used for?' })];
    const { container } = render(
      <ChatMessages messages={messages} isLoading={false} schemaIdentifiers={schema} />
    );

    expect(container.querySelector('[data-identifier]')).toBeNull();
  });

  it('makes assistant messages clickable when they reference a table and fires callback', () => {
    const schema = makeSchema(['MARA'], []);
    const onNavigateToReferences = vi.fn();
    const messages = [makeMessage({ role: 'assistant', content: 'Check MARA for this.' })];
    render(
      <ChatMessages
        messages={messages}
        isLoading={false}
        schemaIdentifiers={schema}
        onNavigateToReferences={onNavigateToReferences}
      />
    );

    const bubble = screen.getByTestId('message-assistant').querySelector('[role="button"]');
    expect(bubble).not.toBeNull();
    expect(bubble).toHaveAttribute('data-reference-table', 'MARA');
    expect(bubble).toHaveAttribute('data-reference-count', '1');
    expect(bubble?.className).toContain('cursor-pointer');

    fireEvent.click(bubble!);
    expect(onNavigateToReferences).toHaveBeenCalledWith([{ tableName: 'MARA' }]);
  });

  it('passes bare column references through unchanged for the host to resolve', () => {
    const schema: SchemaIdentifiers = {
      tables: new Set(['MARA']),
      columns: new Set(['MANDT']),
      columnOwners: new Map([['MANDT', ['MARA']]]),
    };
    const onNavigateToReferences = vi.fn();
    const messages = [makeMessage({ role: 'assistant', content: 'The MANDT column exists.' })];
    render(
      <ChatMessages
        messages={messages}
        isLoading={false}
        schemaIdentifiers={schema}
        onNavigateToReferences={onNavigateToReferences}
      />
    );

    const bubble = screen.getByTestId('message-assistant').querySelector('[role="button"]');
    expect(bubble).toHaveAttribute('data-reference-column', 'MANDT');
    expect(bubble).not.toHaveAttribute('data-reference-table');
    expect(bubble).toHaveAttribute('aria-label', 'Open highlighted nodes in lineage view');

    fireEvent.click(bubble!);
    expect(onNavigateToReferences).toHaveBeenCalledWith([
      { columnName: 'MANDT', bareColumn: true },
    ]);
  });

  it('passes every parsed reference for a multi-identifier message', () => {
    const schema: SchemaIdentifiers = {
      tables: new Set(['BKPF', 'BSEG']),
      columns: new Set(['MANDT', 'BUKRS']),
      columnOwners: new Map([
        ['MANDT', ['BKPF', 'BSEG']],
        ['BUKRS', ['BKPF']],
      ]),
    };
    const onNavigateToReferences = vi.fn();
    const messages = [
      makeMessage({
        role: 'assistant',
        content: 'BKPF.MANDT links to BSEG via BUKRS.',
      }),
    ];
    render(
      <ChatMessages
        messages={messages}
        isLoading={false}
        schemaIdentifiers={schema}
        onNavigateToReferences={onNavigateToReferences}
      />
    );

    const bubble = screen.getByTestId('message-assistant').querySelector('[role="button"]')!;
    expect(bubble).toHaveAttribute('data-reference-count', '3');

    fireEvent.click(bubble);
    expect(onNavigateToReferences).toHaveBeenCalledWith([
      { tableName: 'BKPF', columnName: 'MANDT' },
      { tableName: 'BSEG' },
      { columnName: 'BUKRS', bareColumn: true },
    ]);
  });

  it('does not make assistant messages clickable when there is no resolvable reference', () => {
    const schema = makeSchema(['MARA'], []);
    const onNavigateToReferences = vi.fn();
    const messages = [
      makeMessage({ role: 'assistant', content: 'Just a plain answer with no references.' }),
    ];
    render(
      <ChatMessages
        messages={messages}
        isLoading={false}
        schemaIdentifiers={schema}
        onNavigateToReferences={onNavigateToReferences}
      />
    );

    const bubble = screen.getByTestId('message-assistant').querySelector('[role="button"]');
    expect(bubble).toBeNull();
  });

  it('does not make assistant messages clickable when no callback is provided', () => {
    const schema = makeSchema(['MARA'], []);
    const messages = [makeMessage({ role: 'assistant', content: 'Check MARA.' })];
    render(<ChatMessages messages={messages} isLoading={false} schemaIdentifiers={schema} />);

    const bubble = screen.getByTestId('message-assistant').querySelector('[role="button"]');
    expect(bubble).toBeNull();
  });

  it('activates the callback via keyboard (Enter)', () => {
    const schema = makeSchema(['MARA'], []);
    const onNavigateToReferences = vi.fn();
    const messages = [makeMessage({ role: 'assistant', content: 'MARA row.' })];
    render(
      <ChatMessages
        messages={messages}
        isLoading={false}
        schemaIdentifiers={schema}
        onNavigateToReferences={onNavigateToReferences}
      />
    );

    const bubble = screen.getByTestId('message-assistant').querySelector('[role="button"]');
    fireEvent.keyDown(bubble!, { key: 'Enter' });
    expect(onNavigateToReferences).toHaveBeenCalledWith([{ tableName: 'MARA' }]);
  });

  it('keyboard activation ignores stale page-wide text selection', () => {
    // Regression: the selection guard previously fired for both mouse clicks
    // and keyboard activation. If the user had any text selected anywhere on
    // the page (e.g., in another message) and pressed Enter on a focused
    // bubble, navigation was silently aborted.
    const schema = makeSchema(['MARA'], []);
    const onNavigateToReferences = vi.fn();
    const messages = [makeMessage({ role: 'assistant', content: 'Check MARA for this.' })];
    render(
      <ChatMessages
        messages={messages}
        isLoading={false}
        schemaIdentifiers={schema}
        onNavigateToReferences={onNavigateToReferences}
      />
    );

    const bubble = screen.getByTestId('message-assistant').querySelector('[role="button"]')!;
    const originalGetSelection = window.getSelection;
    window.getSelection = vi.fn(
      () => ({ toString: () => 'stale page-wide selection' }) as unknown as Selection
    );
    try {
      fireEvent.keyDown(bubble, { key: 'Enter' });
      expect(onNavigateToReferences).toHaveBeenCalledWith([{ tableName: 'MARA' }]);
    } finally {
      window.getSelection = originalGetSelection;
    }
  });

  it('does not navigate when text is selected inside the bubble (preserves copy/select)', () => {
    const schema = makeSchema(['MARA'], []);
    const onNavigateToReferences = vi.fn();
    const messages = [makeMessage({ role: 'assistant', content: 'Check MARA for this.' })];
    render(
      <ChatMessages
        messages={messages}
        isLoading={false}
        schemaIdentifiers={schema}
        onNavigateToReferences={onNavigateToReferences}
      />
    );

    const bubble = screen.getByTestId('message-assistant').querySelector('[role="button"]')!;
    const originalGetSelection = window.getSelection;
    window.getSelection = vi.fn(
      () => ({ toString: () => 'some selected text' }) as unknown as Selection
    );
    try {
      fireEvent.click(bubble);
      expect(onNavigateToReferences).not.toHaveBeenCalled();
    } finally {
      window.getSelection = originalGetSelection;
    }
  });

  it('does not navigate when clicking inside a code block (allows code copy)', () => {
    const schema = makeSchema(['MARA'], []);
    const onNavigateToReferences = vi.fn();
    const messages = [
      makeMessage({
        role: 'assistant',
        content: 'See MARA below.\n```sql\nSELECT * FROM MARA;\n```',
      }),
    ];
    const { container } = render(
      <ChatMessages
        messages={messages}
        isLoading={false}
        schemaIdentifiers={schema}
        onNavigateToReferences={onNavigateToReferences}
      />
    );

    const pre = container.querySelector('pre')!;
    expect(pre).not.toBeNull();
    fireEvent.click(pre);
    expect(onNavigateToReferences).not.toHaveBeenCalled();
  });

  it('does not make user messages clickable even when they mention identifiers', () => {
    const schema = makeSchema(['MARA'], []);
    const onNavigateToReferences = vi.fn();
    const messages = [makeMessage({ role: 'user', content: 'Tell me about MARA.' })];
    render(
      <ChatMessages
        messages={messages}
        isLoading={false}
        schemaIdentifiers={schema}
        onNavigateToReferences={onNavigateToReferences}
      />
    );

    const bubble = screen.getByTestId('message-user').querySelector('[role="button"]');
    expect(bubble).toBeNull();
  });
});

// ============================================================================
// ChatInput
// ============================================================================

describe('ChatInput', () => {
  it('renders textarea', () => {
    render(<ChatInput onSend={vi.fn()} disabled={false} />);
    expect(screen.getByTestId('chat-textarea')).toBeInTheDocument();
  });

  it('renders send button', () => {
    render(<ChatInput onSend={vi.fn()} disabled={false} />);
    expect(screen.getByLabelText('Send message')).toBeInTheDocument();
  });

  it('calls onSend when clicking send button', () => {
    const onSend = vi.fn();
    render(<ChatInput onSend={onSend} disabled={false} />);
    const textarea = screen.getByTestId('chat-textarea');
    fireEvent.change(textarea, { target: { value: 'test message' } });
    fireEvent.click(screen.getByLabelText('Send message'));
    expect(onSend).toHaveBeenCalledWith('test message');
  });

  it('calls onSend on Enter key', () => {
    const onSend = vi.fn();
    render(<ChatInput onSend={onSend} disabled={false} />);
    const textarea = screen.getByTestId('chat-textarea');
    fireEvent.change(textarea, { target: { value: 'enter test' } });
    fireEvent.keyDown(textarea, { key: 'Enter', shiftKey: false });
    expect(onSend).toHaveBeenCalledWith('enter test');
  });

  it('does not send on Shift+Enter', () => {
    const onSend = vi.fn();
    render(<ChatInput onSend={onSend} disabled={false} />);
    const textarea = screen.getByTestId('chat-textarea');
    fireEvent.change(textarea, { target: { value: 'shift enter' } });
    fireEvent.keyDown(textarea, { key: 'Enter', shiftKey: true });
    expect(onSend).not.toHaveBeenCalled();
  });

  it('clears input after sending', () => {
    const onSend = vi.fn();
    render(<ChatInput onSend={onSend} disabled={false} />);
    const textarea = screen.getByTestId('chat-textarea') as HTMLTextAreaElement;
    fireEvent.change(textarea, { target: { value: 'clear me' } });
    fireEvent.click(screen.getByLabelText('Send message'));
    expect(textarea.value).toBe('');
  });

  it('does not send empty messages', () => {
    const onSend = vi.fn();
    render(<ChatInput onSend={onSend} disabled={false} />);
    fireEvent.click(screen.getByLabelText('Send message'));
    expect(onSend).not.toHaveBeenCalled();
  });

  it('does not send whitespace-only messages', () => {
    const onSend = vi.fn();
    render(<ChatInput onSend={onSend} disabled={false} />);
    const textarea = screen.getByTestId('chat-textarea');
    fireEvent.change(textarea, { target: { value: '   ' } });
    fireEvent.click(screen.getByLabelText('Send message'));
    expect(onSend).not.toHaveBeenCalled();
  });

  it('disables textarea when disabled prop is true', () => {
    render(<ChatInput onSend={vi.fn()} disabled={true} />);
    expect(screen.getByTestId('chat-textarea')).toBeDisabled();
  });

  it('shows hint when AI is not configured', () => {
    useLibrarianStore.setState({ hasConfig: false });
    render(<ChatInput onSend={vi.fn()} disabled={false} />);
    expect(screen.getByTestId('config-hint')).toBeInTheDocument();
  });

  it('enables send when hasConfig becomes true in store', () => {
    useLibrarianStore.setState({ hasConfig: false });
    const { rerender } = render(<ChatInput onSend={vi.fn()} disabled={false} />);
    expect(screen.getByTestId('chat-textarea')).toBeDisabled();

    useLibrarianStore.setState({ hasConfig: true });
    rerender(<ChatInput onSend={vi.fn()} disabled={false} />);
    expect(screen.getByTestId('chat-textarea')).not.toBeDisabled();
  });

  it('shows no-project hint and disables input when noActiveProject is true', () => {
    render(<ChatInput onSend={vi.fn()} disabled={false} noActiveProject={true} />);
    expect(screen.getByTestId('no-project-hint')).toHaveTextContent(
      /Open or create a project to use Librarian/
    );
    expect(screen.getByTestId('chat-textarea')).toBeDisabled();
    expect(screen.queryByTestId('config-hint')).not.toBeInTheDocument();
  });

  it('does not call onSend when noActiveProject is true', () => {
    const onSend = vi.fn();
    render(<ChatInput onSend={onSend} disabled={false} noActiveProject={true} />);
    const textarea = screen.getByTestId('chat-textarea');
    fireEvent.change(textarea, { target: { value: 'test' } });
    fireEvent.keyDown(textarea, { key: 'Enter', shiftKey: false });
    expect(onSend).not.toHaveBeenCalled();
  });

  it('truncates input to MAX_MESSAGE_LENGTH', () => {
    render(<ChatInput onSend={vi.fn()} disabled={false} />);
    const textarea = screen.getByTestId('chat-textarea') as HTMLTextAreaElement;
    const longText = 'a'.repeat(MAX_MESSAGE_LENGTH + 100);
    fireEvent.change(textarea, { target: { value: longText } });
    expect(textarea.value.length).toBe(MAX_MESSAGE_LENGTH);
  });
});

// ============================================================================
// PdfUpload
// ============================================================================

describe('PdfUpload', () => {
  it('renders drop zone', () => {
    render(<PdfUpload onUpload={vi.fn()} />);
    expect(screen.getByTestId('drop-zone')).toBeInTheDocument();
    expect(screen.getByText(/Drop a PDF/)).toBeInTheDocument();
  });

  it('calls onUpload for valid PDF file', () => {
    const onUpload = vi.fn();
    render(<PdfUpload onUpload={onUpload} />);
    const input = screen.getByTestId('file-input');
    const file = new File(['content'], 'test.pdf', { type: 'application/pdf' });
    fireEvent.change(input, { target: { files: [file] } });
    expect(onUpload).toHaveBeenCalledWith(file);
  });

  it('rejects non-PDF files', () => {
    const onUpload = vi.fn();
    render(<PdfUpload onUpload={onUpload} />);
    const input = screen.getByTestId('file-input');
    const file = new File(['content'], 'test.txt', { type: 'text/plain' });
    fireEvent.change(input, { target: { files: [file] } });
    expect(onUpload).not.toHaveBeenCalled();
    expect(screen.getByTestId('upload-error')).toHaveTextContent('Only PDF files');
  });

  it('rejects files exceeding size limit', () => {
    const onUpload = vi.fn();
    render(<PdfUpload onUpload={onUpload} />);
    const input = screen.getByTestId('file-input');
    const bigContent = new ArrayBuffer(MAX_PDF_SIZE_MB * 1024 * 1024 + 1);
    const file = new File([bigContent], 'big.pdf', { type: 'application/pdf' });
    fireEvent.change(input, { target: { files: [file] } });
    expect(onUpload).not.toHaveBeenCalled();
    expect(screen.getByTestId('upload-error')).toHaveTextContent(`${MAX_PDF_SIZE_MB} MB`);
  });

  it('allows uploading many files (no file count limit)', () => {
    const files = Array.from({ length: 20 }, (_, i) =>
      makePdfFile({ id: `f-${i}`, name: `file-${i}.pdf` })
    );
    useLibrarianStore.setState({
      byProject: { 'proj-1': { messages: [], pdfFiles: files, pdfChunks: [] } },
      activeProjectId: 'proj-1',
    });

    const onUpload = vi.fn();
    render(<PdfUpload onUpload={onUpload} />);
    const input = screen.getByTestId('file-input');
    const file = new File(['content'], 'extra.pdf', { type: 'application/pdf' });
    fireEvent.change(input, { target: { files: [file] } });
    expect(onUpload).toHaveBeenCalledWith(file);
  });

  it('rejects duplicate file names', () => {
    useLibrarianStore.setState({
      byProject: {
        'proj-1': {
          messages: [],
          pdfFiles: [makePdfFile({ name: 'dup.pdf' })],
          pdfChunks: [],
        },
      },
      activeProjectId: 'proj-1',
    });

    const onUpload = vi.fn();
    render(<PdfUpload onUpload={onUpload} />);
    const input = screen.getByTestId('file-input');
    const file = new File(['content'], 'dup.pdf', { type: 'application/pdf' });
    fireEvent.change(input, { target: { files: [file] } });
    expect(onUpload).not.toHaveBeenCalled();
    expect(screen.getByTestId('upload-error')).toHaveTextContent('already uploaded');
  });

  it('renders file list with status', () => {
    useLibrarianStore.setState({
      byProject: {
        'proj-1': {
          messages: [],
          pdfFiles: [
            makePdfFile({ id: 'f1', name: 'ready.pdf', status: 'ready' }),
            makePdfFile({ id: 'f2', name: 'processing.pdf', status: 'processing' }),
          ],
          pdfChunks: [],
        },
      },
      activeProjectId: 'proj-1',
    });

    render(<PdfUpload onUpload={vi.fn()} />);
    const items = screen.getAllByTestId('pdf-file-item');
    expect(items).toHaveLength(2);
    expect(screen.getByText('ready.pdf')).toBeInTheDocument();
    expect(screen.getByText('processing.pdf')).toBeInTheDocument();
  });

  it('removes file when clicking remove button', () => {
    useLibrarianStore.setState({
      byProject: {
        'proj-1': {
          messages: [],
          pdfFiles: [makePdfFile({ id: 'f1', name: 'remove-me.pdf' })],
          pdfChunks: [],
        },
      },
      activeProjectId: 'proj-1',
    });

    render(<PdfUpload onUpload={vi.fn()} />);
    fireEvent.click(screen.getByLabelText('Remove remove-me.pdf'));
    expect(useLibrarianStore.getState().byProject['proj-1'].pdfFiles).toHaveLength(0);
  });

  it('only lists PDFs from the active project', () => {
    useLibrarianStore.setState({
      byProject: {
        'proj-1': {
          messages: [],
          pdfFiles: [makePdfFile({ id: 'a1', name: 'project-a.pdf' })],
          pdfChunks: [],
        },
        'proj-2': {
          messages: [],
          pdfFiles: [makePdfFile({ id: 'b1', name: 'project-b.pdf' })],
          pdfChunks: [],
        },
      },
      activeProjectId: 'proj-1',
    });

    render(<PdfUpload onUpload={vi.fn()} />);
    expect(screen.getByText('project-a.pdf')).toBeInTheDocument();
    expect(screen.queryByText('project-b.pdf')).not.toBeInTheDocument();
  });

  it('handles drag and drop', () => {
    const onUpload = vi.fn();
    render(<PdfUpload onUpload={onUpload} />);
    const dropZone = screen.getByTestId('drop-zone');

    const file = new File(['content'], 'dropped.pdf', { type: 'application/pdf' });
    const dataTransfer = { files: [file] };

    fireEvent.dragOver(dropZone, { dataTransfer });
    fireEvent.drop(dropZone, { dataTransfer });

    expect(onUpload).toHaveBeenCalledWith(file);
  });

  it('renders ScrollArea with max-h-[64px] when many files are uploaded', () => {
    const files = Array.from({ length: 6 }, (_, i) =>
      makePdfFile({ id: `scroll-${i}`, name: `doc-${i}.pdf` })
    );
    useLibrarianStore.setState({
      byProject: { 'proj-1': { messages: [], pdfFiles: files, pdfChunks: [] } },
      activeProjectId: 'proj-1',
    });

    const { container } = render(<PdfUpload onUpload={vi.fn()} />);
    const items = screen.getAllByTestId('pdf-file-item');
    expect(items).toHaveLength(6);

    const scrollArea = container.querySelector('.max-h-\\[64px\\]');
    expect(scrollArea).toBeInTheDocument();
  });

  it('keeps size and delete button visible while truncating long file names', () => {
    const longName =
      'a-very-long-pdf-file-name-that-should-be-truncated-with-ellipsis-when-the-panel-is-narrow.pdf';
    useLibrarianStore.setState({
      byProject: {
        'proj-1': {
          messages: [],
          pdfFiles: [makePdfFile({ id: 'long-1', name: longName, size: 2_500_000 })],
          pdfChunks: [],
        },
      },
      activeProjectId: 'proj-1',
    });

    render(<PdfUpload onUpload={vi.fn()} />);

    const item = screen.getByTestId('pdf-file-item');
    const nameSpan = item.querySelector('span.truncate');
    expect(nameSpan).not.toBeNull();
    expect(nameSpan).toHaveTextContent(longName);
    expect(nameSpan?.className).toContain('min-w-0');
    expect(nameSpan?.className).toContain('flex-1');
    expect(nameSpan?.className).toContain('truncate');

    const sizeSpan = screen.getByText('2.4 MB');
    expect(sizeSpan.className).toContain('shrink-0');
    expect(sizeSpan.className).toContain('whitespace-nowrap');

    const removeButton = screen.getByLabelText(`Remove ${longName}`);
    expect(removeButton.className).toContain('shrink-0');
  });

  it('has data-librarian-dropzone attribute on drop zone', () => {
    render(<PdfUpload onUpload={vi.fn()} />);
    const dropZone = screen.getByTestId('drop-zone');
    expect(dropZone).toHaveAttribute('data-librarian-dropzone');
  });

  it('calls stopPropagation on drop to prevent global handler', () => {
    const onUpload = vi.fn();
    render(<PdfUpload onUpload={onUpload} />);
    const dropZone = screen.getByTestId('drop-zone');

    const file = new File(['content'], 'stopped.pdf', { type: 'application/pdf' });
    const stopPropagation = vi.fn();
    const dropEvent = new Event('drop', { bubbles: true });
    Object.defineProperty(dropEvent, 'dataTransfer', { value: { files: [file] } });
    Object.defineProperty(dropEvent, 'stopPropagation', { value: stopPropagation });
    Object.defineProperty(dropEvent, 'preventDefault', { value: vi.fn() });

    fireEvent(dropZone, dropEvent);
    expect(stopPropagation).toHaveBeenCalled();
  });

  it('calls stopPropagation on dragOver to prevent global handler', () => {
    render(<PdfUpload onUpload={vi.fn()} />);
    const dropZone = screen.getByTestId('drop-zone');

    const stopPropagation = vi.fn();
    const dragOverEvent = new Event('dragover', { bubbles: true });
    Object.defineProperty(dragOverEvent, 'stopPropagation', { value: stopPropagation });
    Object.defineProperty(dragOverEvent, 'preventDefault', { value: vi.fn() });

    fireEvent(dropZone, dragOverEvent);
    expect(stopPropagation).toHaveBeenCalled();
  });
});
