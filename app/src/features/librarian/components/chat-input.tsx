import { type KeyboardEvent, useCallback, useRef, useState } from 'react';
import { Send } from 'lucide-react';

import { Button } from '@/components/ui/button';

import { MAX_MESSAGE_LENGTH } from '../constants';
import { useLibrarianStore } from '../store';

interface ChatInputProps {
  onSend: (message: string) => void;
  disabled: boolean;
  noActiveProject?: boolean;
}

export function ChatInput({ onSend, disabled, noActiveProject = false }: ChatInputProps) {
  const [value, setValue] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const hasConfig = useLibrarianStore((s) => s.hasConfig);
  const inputDisabled = disabled || !hasConfig || noActiveProject;

  const handleSend = useCallback(() => {
    const trimmed = value.trim();
    if (!trimmed || disabled || noActiveProject) return;
    onSend(trimmed);
    setValue('');
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
    }
  }, [value, disabled, noActiveProject, onSend]);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend]
  );

  const handleInput = useCallback(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    textarea.style.height = 'auto';
    const lineHeight = 20;
    const maxHeight = lineHeight * 4;
    textarea.style.height = `${Math.min(textarea.scrollHeight, maxHeight)}px`;
  }, []);

  return (
    <div className="border-t p-3">
      {noActiveProject ? (
        <p className="mb-2 text-xs text-muted-foreground" data-testid="no-project-hint">
          Open or create a project to use Librarian.
        </p>
      ) : !hasConfig ? (
        <p className="mb-2 text-xs text-muted-foreground" data-testid="config-hint">
          Configure AI settings to start chatting.
        </p>
      ) : null}
      <div className="flex items-end gap-2">
        <textarea
          ref={textareaRef}
          value={value}
          onChange={(e) => setValue(e.target.value.slice(0, MAX_MESSAGE_LENGTH))}
          onKeyDown={handleKeyDown}
          onInput={handleInput}
          disabled={inputDisabled}
          placeholder="Ask about your data..."
          rows={1}
          className="flex-1 resize-none rounded-lg border border-border-primary-light bg-background px-3 py-2 text-sm placeholder:text-muted-foreground focus-visible:outline-hidden focus-visible:border-accent-light dark:border-border-primary-dark dark:focus-visible:border-accent-dark disabled:cursor-not-allowed disabled:opacity-60"
          data-testid="chat-textarea"
        />
        <Button
          variant="default"
          size="icon"
          className="h-9 w-9 shrink-0"
          onClick={handleSend}
          disabled={inputDisabled || !value.trim()}
          aria-label="Send message"
        >
          <Send className="h-4 w-4" />
        </Button>
      </div>
    </div>
  );
}
