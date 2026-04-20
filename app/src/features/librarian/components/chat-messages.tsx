import { useEffect, useRef } from 'react';
import { Bot, User } from 'lucide-react';

import { ScrollArea } from '@/components/ui/scroll-area';

import type { ChatMessage } from '../types';
import {
  detectIdentifiers,
  EMPTY_SCHEMA_IDENTIFIERS,
  resolveFirstTableReference,
  type SchemaIdentifiers,
} from '../utils/schema-identifiers';

interface ChatMessagesProps {
  messages: ChatMessage[];
  isLoading: boolean;
  schemaIdentifiers?: SchemaIdentifiers;
  /**
   * Called when the user clicks an assistant message that contains at least
   * one resolvable schema identifier. Receives the first referenced table.
   */
  onNavigateToTable?: (tableName: string) => void;
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
          className="rounded bg-background/50 px-1 py-0.5 text-xs font-mono"
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
  onNavigateToTable,
}: ChatMessagesProps) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, isLoading]);

  if (messages.length === 0 && !isLoading) {
    return (
      <div className="flex flex-1 items-center justify-center p-6" data-testid="empty-state">
        <div className="text-center">
          <Bot className="mx-auto h-10 w-10 text-muted-foreground/50" />
          <p className="mt-2 text-sm text-muted-foreground">
            Ask questions about your data based on data lineage and uploaded documents
          </p>
        </div>
      </div>
    );
  }

  const schema = schemaIdentifiers ?? EMPTY_SCHEMA_IDENTIFIERS;

  return (
    <ScrollArea className="flex-1">
      <div className="flex flex-col gap-4 p-4">
        {messages.map((msg) => {
          const reference =
            msg.role === 'assistant' && onNavigateToTable
              ? resolveFirstTableReference(msg.content, schema)
              : null;
          const isClickable = reference != null;
          const bubbleClass = `min-w-0 rounded-lg px-3 py-2 text-sm whitespace-pre-wrap ${
            msg.role === 'user' ? 'bg-primary text-primary-foreground' : 'bg-muted'
          }${isClickable ? ' cursor-pointer hover:bg-muted/80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring' : ''}`;
          const handleActivate = () => {
            // Skip navigation when the user is selecting text inside the bubble
            // (text selection ends with a click). Without this, copying text or
            // SQL out of an answer would also navigate to the schema view.
            if ((window.getSelection?.()?.toString().length ?? 0) > 0) return;
            if (reference && onNavigateToTable) {
              onNavigateToTable(reference.tableName);
            }
          };
          const clickableProps = isClickable
            ? {
                role: 'button',
                tabIndex: 0,
                onClick: handleActivate,
                onKeyDown: (e: React.KeyboardEvent<HTMLDivElement>) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    handleActivate();
                  }
                },
                'aria-label': `Open ${reference!.tableName} in schema view`,
                'data-reference-table': reference!.tableName,
                ...(reference!.columnName
                  ? { 'data-reference-column': reference!.columnName }
                  : {}),
              }
            : {};
          return (
            <div
              key={msg.id}
              className={`flex gap-3 ${msg.role === 'user' ? 'justify-end' : 'justify-start'}`}
              data-testid={`message-${msg.role}`}
            >
              {msg.role === 'assistant' && (
                <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-accent/10">
                  <Bot className="h-4 w-4 text-accent-light dark:text-accent-dark" />
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
              <Bot className="h-4 w-4 text-accent-light dark:text-accent-dark" />
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
    </ScrollArea>
  );
}
