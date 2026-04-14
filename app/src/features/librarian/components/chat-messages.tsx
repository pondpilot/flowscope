import { useEffect, useRef } from 'react';
import { Bot, User } from 'lucide-react';

import { ScrollArea } from '@/components/ui/scroll-area';

import type { ChatMessage } from '../types';

interface ChatMessagesProps {
  messages: ChatMessage[];
  isLoading: boolean;
}

/**
 * Render inline markdown: **bold**, `code`, and plain text.
 */
function renderInline(text: string, keyPrefix: string): React.ReactNode[] {
  const nodes: React.ReactNode[] = [];
  const inlineRegex = /(\*\*(.+?)\*\*)|(`([^`]+)`)/g;
  let last = 0;
  let m: RegExpExecArray | null;

  while ((m = inlineRegex.exec(text)) !== null) {
    if (m.index > last) {
      nodes.push(<span key={`${keyPrefix}-t-${last}`}>{text.slice(last, m.index)}</span>);
    }
    if (m[2]) {
      nodes.push(<strong key={`${keyPrefix}-b-${m.index}`}>{m[2]}</strong>);
    } else if (m[4]) {
      nodes.push(
        <code
          key={`${keyPrefix}-c-${m.index}`}
          className="rounded bg-background/50 px-1 py-0.5 text-xs font-mono"
        >
          {m[4]}
        </code>
      );
    }
    last = m.index + m[0].length;
  }

  if (last < text.length) {
    nodes.push(<span key={`${keyPrefix}-t-${last}`}>{text.slice(last)}</span>);
  }

  return nodes;
}

function formatContent(content: string) {
  const parts: React.ReactNode[] = [];
  const codeBlockRegex = /```(\w*)\n?([\s\S]*?)```/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = codeBlockRegex.exec(content)) !== null) {
    if (match.index > lastIndex) {
      parts.push(
        <span key={`text-${lastIndex}`}>
          {renderInline(content.slice(lastIndex, match.index), `i-${lastIndex}`)}
        </span>
      );
    }
    parts.push(
      <pre
        key={`code-${match.index}`}
        className="my-2 overflow-x-auto rounded-md bg-muted p-3 text-xs font-mono"
      >
        <code>{match[2]}</code>
      </pre>
    );
    lastIndex = match.index + match[0].length;
  }

  if (lastIndex < content.length) {
    parts.push(
      <span key={`text-${lastIndex}`}>
        {renderInline(content.slice(lastIndex), `i-${lastIndex}`)}
      </span>
    );
  }

  return parts;
}

export function ChatMessages({ messages, isLoading }: ChatMessagesProps) {
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

  return (
    <ScrollArea className="flex-1">
      <div className="flex flex-col gap-4 p-4">
        {messages.map((msg) => (
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
            <div
              className={`min-w-0 rounded-lg px-3 py-2 text-sm whitespace-pre-wrap ${
                msg.role === 'user' ? 'bg-primary text-primary-foreground' : 'bg-muted'
              }`}
            >
              {formatContent(msg.content)}
            </div>
            {msg.role === 'user' && (
              <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-primary/10">
                <User className="h-4 w-4" />
              </div>
            )}
          </div>
        ))}

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
