/// <reference types="@testing-library/jest-dom" />
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useLibrarianStore } from '../store';

// ---------- Mocks ----------

const mockSendMessage = vi.fn();
const mockCancel = vi.fn();
vi.mock('../hooks/use-librarian-chat', () => ({
  useLibrarianChat: () => ({
    sendMessage: mockSendMessage,
    cancel: mockCancel,
  }),
}));

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

vi.mock('../services/pdf-processor', () => ({
  processPdf: vi.fn(() => Promise.resolve([])),
}));

vi.mock('../services/embedding-service', () => ({
  embedTexts: vi.fn(() => Promise.resolve([[0.1, 0.2]])),
}));

vi.mock('@pondpilot/flowscope-react', () => ({
  useLineageState: () => ({ result: null }),
}));

vi.mock('@/lib/project-store', () => ({
  useProject: () => ({ currentProject: null }),
}));

import { LibrarianPanel } from '../components/librarian-panel';

// ---------- Setup ----------

beforeEach(() => {
  useLibrarianStore.setState({
    byProject: { 'proj-1': { messages: [], pdfFiles: [], pdfChunks: [] } },
    activeProjectId: 'proj-1',
    isLoading: false,
  });
  vi.clearAllMocks();
});

afterEach(() => {
  cleanup();
});

// ---------- Tests ----------

describe('LibrarianPanel', () => {
  it('renders the panel with header', () => {
    render(<LibrarianPanel onClose={vi.fn()} />);
    expect(screen.getByTestId('librarian-panel')).toBeInTheDocument();
    expect(screen.getByText('Librarian')).toBeInTheDocument();
  });

  it('renders settings button', () => {
    render(<LibrarianPanel onClose={vi.fn()} />);
    expect(screen.getByTestId('settings-button')).toBeInTheDocument();
  });

  it('renders close button', () => {
    render(<LibrarianPanel onClose={vi.fn()} />);
    expect(screen.getByTestId('close-button')).toBeInTheDocument();
  });

  it('calls onClose when close button is clicked', () => {
    const onClose = vi.fn();
    render(<LibrarianPanel onClose={onClose} />);
    fireEvent.click(screen.getByTestId('close-button'));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('opens settings dialog when settings button is clicked', () => {
    render(<LibrarianPanel onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('settings-button'));
    expect(screen.getByText('AI Settings')).toBeInTheDocument();
  });

  it('renders empty chat state initially', () => {
    render(<LibrarianPanel onClose={vi.fn()} />);
    expect(screen.getByTestId('empty-state')).toBeInTheDocument();
  });

  it('renders chat input', () => {
    render(<LibrarianPanel onClose={vi.fn()} />);
    expect(screen.getByTestId('chat-textarea')).toBeInTheDocument();
  });

  it('renders the last prompt size when available', () => {
    useLibrarianStore.setState({
      byProject: {
        'proj-1': {
          messages: [],
          pdfFiles: [],
          pdfChunks: [],
          lastPromptStats: { characters: 12430, bytes: 13100 },
        },
      },
      activeProjectId: 'proj-1',
    });

    render(<LibrarianPanel onClose={vi.fn()} />);
    expect(screen.getByTestId('last-prompt-size')).toHaveTextContent(
      'Last prompt: 12,430 chars / 12.8 KB'
    );
  });

  it('renders documentation toggle', () => {
    render(<LibrarianPanel onClose={vi.fn()} />);
    expect(screen.getByTestId('docs-toggle')).toBeInTheDocument();
    expect(screen.getByText('Documentation')).toBeInTheDocument();
  });

  it('expands documentation section when toggle is clicked', () => {
    render(<LibrarianPanel onClose={vi.fn()} />);
    expect(screen.queryByTestId('docs-section')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('docs-toggle'));
    expect(screen.getByTestId('docs-section')).toBeInTheDocument();
    expect(screen.getByTestId('drop-zone')).toBeInTheDocument();
  });

  it('collapses documentation section when toggle is clicked again', () => {
    render(<LibrarianPanel onClose={vi.fn()} />);

    fireEvent.click(screen.getByTestId('docs-toggle'));
    expect(screen.getByTestId('docs-section')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('docs-toggle'));
    expect(screen.queryByTestId('docs-section')).not.toBeInTheDocument();
  });

  it('renders messages from active project bucket', () => {
    useLibrarianStore.setState({
      byProject: {
        'proj-1': {
          messages: [
            {
              id: '1',
              role: 'user',
              content: 'What tables exist?',
              timestamp: Date.now(),
            },
            {
              id: '2',
              role: 'assistant',
              content: 'There are 3 tables.',
              timestamp: Date.now(),
            },
          ],
          pdfFiles: [],
          pdfChunks: [],
        },
      },
      activeProjectId: 'proj-1',
    });

    render(<LibrarianPanel onClose={vi.fn()} />);
    expect(screen.getByText('What tables exist?')).toBeInTheDocument();
    expect(screen.getByText('There are 3 tables.')).toBeInTheDocument();
  });

  it('does not render messages from a non-active project bucket', () => {
    useLibrarianStore.setState({
      byProject: {
        'proj-1': { messages: [], pdfFiles: [], pdfChunks: [] },
        'proj-2': {
          messages: [
            {
              id: '1',
              role: 'user',
              content: 'Hidden in project 2',
              timestamp: Date.now(),
            },
          ],
          pdfFiles: [],
          pdfChunks: [],
        },
      },
      activeProjectId: 'proj-1',
    });

    render(<LibrarianPanel onClose={vi.fn()} />);
    expect(screen.queryByText('Hidden in project 2')).not.toBeInTheDocument();
    expect(screen.getByTestId('empty-state')).toBeInTheDocument();
  });

  it('disables chat input and shows hint when no active project', () => {
    useLibrarianStore.setState({
      byProject: {},
      activeProjectId: null,
    });

    render(<LibrarianPanel onClose={vi.fn()} />);
    expect(screen.getByTestId('no-project-hint')).toHaveTextContent(
      /Open or create a project to use Librarian/
    );
    expect(screen.getByTestId('chat-textarea')).toBeDisabled();
  });

  it('renders loading indicator when loading', () => {
    useLibrarianStore.setState({ isLoading: true });
    render(<LibrarianPanel onClose={vi.fn()} />);
    expect(screen.getByTestId('loading-indicator')).toBeInTheDocument();
  });

  it('renders help button in header', () => {
    render(<LibrarianPanel onClose={vi.fn()} />);
    const helpButton = screen.getByTestId('help-button');
    expect(helpButton).toBeInTheDocument();
    expect(helpButton).toHaveAttribute('aria-label', 'About Librarian');
  });

  it('opens help popover with full help text when help button is clicked', () => {
    render(<LibrarianPanel onClose={vi.fn()} />);

    expect(screen.queryByTestId('help-popover')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('help-button'));

    const popover = screen.getByTestId('help-popover');
    expect(popover).toBeInTheDocument();
    expect(popover).toHaveTextContent(/Hi, I'm Librarian!/);
    expect(popover).toHaveTextContent(
      /I answer questions about your data structure using your database schema and uploaded technical documentation\./
    );
    expect(popover).toHaveTextContent(/How to use:/);
    expect(popover).toHaveTextContent(/Configure your AI provider in Settings/);
    expect(popover).toHaveTextContent(/Upload relevant PDF docs \(optional\)/);
    expect(popover).toHaveTextContent(/Ask questions about your data/);
  });
});
