import { useEffect, useRef } from 'react';
import { User } from 'lucide-react';

import type { ChatMessage } from '../types';
import {
  detectIdentifiers,
  EMPTY_SCHEMA_IDENTIFIERS,
  extractSummary,
  resolveAllReferences,
  type ChatReference,
  type SchemaIdentifiers,
} from '../utils/schema-identifiers';

interface ChatMessagesProps {
  messages: ChatMessage[];
  isLoading: boolean;
  schemaIdentifiers?: SchemaIdentifiers;
  /**
   * Called when the user clicks an assistant message that contains at least
   * one resolvable schema identifier. Receives every parsed reference so the
   * host can highlight all of them in the lineage view.
   */
  onNavigateToReferences?: (refs: ChatReference[]) => void;
}

const IDENTIFIER_CLASS = 'font-mono text-primary font-medium';

/**
 * Wrap known schema identifiers in a styled span. Used only for plain-text
 * portions of assistant messages (inline code / code blocks already styled).
 */
function renderWithIdentifiers(
  text: string,
  schema: SchemaIdentifiers,
  keyPrefix: string
): React.ReactNode[] {
  const segments = detectIdentifiers(text, schema);
  return segments.map((seg, i) => {
    if (seg.type === 'identifier') {
      return (
        <span
          key={`${keyPrefix}-id-${i}`}
          className={IDENTIFIER_CLASS}
          data-identifier={seg.value}
          data-identifier-kind={seg.kind}
        >
          {seg.value}
        </span>
      );
    }
    return <span key={`${keyPrefix}-t-${i}`}>{seg.value}</span>;
  });
}

/**
 * Render inline markdown: **bold**, `code`, and plain text. For assistant
 * messages, plain-text portions are further tokenized to highlight schema
 * identifiers.
 */
function renderInline(
  text: string,
  keyPrefix: string,
  schema: SchemaIdentifiers
): React.ReactNode[] {
  const nodes: React.ReactNode[] = [];
  const inlineRegex = /(\*\*(.+?)\*\*)|(`([^`]+)`)/g;
  let last = 0;
  let m: RegExpExecArray | null;

  while ((m = inlineRegex.exec(text)) !== null) {
    if (m.index > last) {
      nodes.push(
        <span key={`${keyPrefix}-t-${last}`}>
          {renderWithIdentifiers(text.slice(last, m.index), schema, `${keyPrefix}-t-${last}`)}
        </span>
      );
    }
    if (m[2]) {
      nodes.push(<strong key={`${keyPrefix}-b-${m.index}`}>{m[2]}</strong>);
    } else if (m[4]) {
      nodes.push(
        <code
          key={`${keyPrefix}-c-${m.index}`}
          className="rounded bg-accent/10 px-1 py-0.5 text-xs font-mono text-accent-light dark:text-accent-dark"
          onClick={(e) => e.stopPropagation()}
        >
          {m[4]}
        </code>
      );
    }
    last = m.index + m[0].length;
  }

  if (last < text.length) {
    nodes.push(
      <span key={`${keyPrefix}-t-${last}`}>
        {renderWithIdentifiers(text.slice(last), schema, `${keyPrefix}-t-${last}`)}
      </span>
    );
  }

  return nodes;
}

function formatContent(content: string, schema: SchemaIdentifiers) {
  const parts: React.ReactNode[] = [];
  const codeBlockRegex = /```(\w*)\n?([\s\S]*?)```/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = codeBlockRegex.exec(content)) !== null) {
    if (match.index > lastIndex) {
      parts.push(
        <span key={`text-${lastIndex}`}>
          {renderInline(content.slice(lastIndex, match.index), `i-${lastIndex}`, schema)}
        </span>
      );
    }
    parts.push(
      <pre
        key={`code-${match.index}`}
        className="my-2 overflow-x-auto rounded-md bg-muted p-3 text-xs font-mono"
        onClick={(e) => e.stopPropagation()}
      >
        <code>{match[2]}</code>
      </pre>
    );
    lastIndex = match.index + match[0].length;
  }

  if (lastIndex < content.length) {
    parts.push(
      <span key={`text-${lastIndex}`}>
        {renderInline(content.slice(lastIndex), `i-${lastIndex}`, schema)}
      </span>
    );
  }

  return parts;
}

export function ChatMessages({
  messages,
  isLoading,
  schemaIdentifiers,
  onNavigateToReferences,
}: ChatMessagesProps) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, isLoading]);

  if (messages.length === 0 && !isLoading) {
    return (
      <div className="flex flex-1 items-center justify-center p-6" data-testid="empty-state">
        <div className="text-center">
          <img src="/polly-icon.svg" alt="Librarian" className="mx-auto h-14 w-14" />
          <p className="mt-2 text-sm text-muted-foreground">
            Ask questions about your data based on data lineage and uploaded documents
          </p>
        </div>
      </div>
    );
  }

  const schema = schemaIdentifiers ?? EMPTY_SCHEMA_IDENTIFIERS;

  return (
    <div className="flex-1 min-w-0 overflow-y-auto overflow-x-hidden">
      <div className="flex min-w-0 flex-col gap-4 p-4">
        {messages.map((msg) => {
          const refs =
            msg.role === 'assistant' && onNavigateToReferences
              ? resolveAllReferences(extractSummary(msg.content), schema)
              : [];
          const isClickable = refs.length > 0;
          const firstTable = refs.find((r) => r.tableName)?.tableName;
          const firstColumn = refs.find((r) => r.columnName)?.columnName;
          const ariaLabel = firstTable
            ? `Open ${firstTable} in lineage view`
            : 'Open highlighted nodes in lineage view';
          const bubbleClass = `min-w-0 max-w-[calc(100%-2.5rem)] break-words rounded-lg px-3 py-2 text-sm whitespace-pre-wrap ${
            msg.role === 'user' ? 'bg-primary text-primary-foreground' : 'bg-muted'
          }${isClickable ? ' cursor-pointer hover:bg-muted/80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring' : ''}`;
          const handleActivate = (source: 'click' | 'keyboard') => {
            // Skip navigation when the user is selecting text inside the bubble
            // (text selection ends with a click). Without this, copying text or
            // SQL out of an answer would also navigate to the lineage view.
            // Only applies to mouse clicks — pressing Enter/Space on a focused
            // bubble must always activate, regardless of any stale page-wide
            // selection that may exist elsewhere.
            if (source === 'click' && (window.getSelection?.()?.toString().length ?? 0) > 0) {
              return;
            }
            if (refs.length > 0 && onNavigateToReferences) {
              onNavigateToReferences(refs);
            }
          };
          const clickableProps = isClickable
            ? {
                role: 'button',
                tabIndex: 0,
                onClick: () => handleActivate('click'),
                onKeyDown: (e: React.KeyboardEvent<HTMLDivElement>) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    handleActivate('keyboard');
                  }
                },
                'aria-label': ariaLabel,
                'data-reference-count': String(refs.length),
                ...(firstTable ? { 'data-reference-table': firstTable } : {}),
                ...(firstColumn ? { 'data-reference-column': firstColumn } : {}),
              }
            : {};
          return (
            <div
              key={msg.id}
              className={`flex min-w-0 gap-3 ${msg.role === 'user' ? 'justify-end' : 'justify-start'}`}
              data-testid={`message-${msg.role}`}
            >
              {msg.role === 'assistant' && (
                <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-accent/10">
                  <img src="/polly-icon.svg" alt="" className="h-6 w-6" />
                </div>
              )}
              <div className={bubbleClass} {...clickableProps}>
                {msg.role === 'assistant'
                  ? formatContent(msg.content, schema)
                  : formatContent(msg.content, EMPTY_SCHEMA_IDENTIFIERS)}
              </div>
              {msg.role === 'user' && (
                <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-primary/10">
                  <User className="h-4 w-4" />
                </div>
              )}
            </div>
          );
        })}

        {isLoading && (
          <div className="flex gap-3" data-testid="loading-indicator">
            <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-accent/10">
              <img src="/polly-icon.svg" alt="" className="h-6 w-6" />
            </div>
            <div className="flex items-center gap-1 rounded-lg bg-muted px-3 py-2">
              <span className="h-2 w-2 animate-bounce rounded-full bg-muted-foreground/50 [animation-delay:0ms]" />
              <span className="h-2 w-2 animate-bounce rounded-full bg-muted-foreground/50 [animation-delay:150ms]" />
              <span className="h-2 w-2 animate-bounce rounded-full bg-muted-foreground/50 [animation-delay:300ms]" />
            </div>
          </div>
        )}

        <div ref={bottomRef} />
      </div>
    </div>
  );
}
