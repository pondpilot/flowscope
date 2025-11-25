import { useState, useRef, useEffect } from 'react';
import { FolderOpen, Pencil, Trash2 } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { DropdownMenuItem } from '@/components/ui/dropdown-menu';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import type { Project } from '@/lib/project-store';

const KEYBOARD_SHORTCUTS = {
  RENAME: ['r', 'R'],
  DELETE: ['d', 'D'],
  CONFIRM_DELETE: ['y', 'Y', 'd', 'D', 'Enter'],
  CANCEL: ['n', 'N', 'Escape'],
} as const;

interface ProjectMenuItemProps {
  project: Project;
  isActive: boolean;
  canDelete: boolean;
  onSelect: () => void;
  onRename: (newName: string) => void;
  onDelete: () => void;
}

export function ProjectMenuItem({
  project,
  isActive,
  canDelete,
  onSelect,
  onRename,
  onDelete,
}: ProjectMenuItemProps) {
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState('');
  const [isConfirmingDelete, setIsConfirmingDelete] = useState(false);
  const renameInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isRenaming && renameInputRef.current) {
      setTimeout(() => renameInputRef.current?.focus(), 0);
    }
  }, [isRenaming]);

  const handleStartRename = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsRenaming(true);
    setRenameValue(project.name);
  };

  const handleConfirmRename = () => {
    const trimmedName = renameValue.trim();
    if (trimmedName && trimmedName !== project.name) {
      onRename(trimmedName);
    }
    setIsRenaming(false);
    setRenameValue('');
  };

  const handleCancelRename = () => {
    setIsRenaming(false);
    setRenameValue('');
  };

  const handleDeleteClick = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (isConfirmingDelete) {
      onDelete();
      setIsConfirmingDelete(false);
    } else {
      setIsConfirmingDelete(true);
    }
  };

  const handleCancelDelete = () => {
    setIsConfirmingDelete(false);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (isConfirmingDelete) {
      e.preventDefault();
      e.stopPropagation();
      if (KEYBOARD_SHORTCUTS.CONFIRM_DELETE.includes(e.key as typeof KEYBOARD_SHORTCUTS.CONFIRM_DELETE[number])) {
        onDelete();
        setIsConfirmingDelete(false);
        return;
      }
      if (KEYBOARD_SHORTCUTS.CANCEL.includes(e.key as typeof KEYBOARD_SHORTCUTS.CANCEL[number])) {
        handleCancelDelete();
        return;
      }
      return;
    }

    if (KEYBOARD_SHORTCUTS.RENAME.includes(e.key as typeof KEYBOARD_SHORTCUTS.RENAME[number])) {
      e.preventDefault();
      e.stopPropagation();
      setIsRenaming(true);
      setRenameValue(project.name);
    }
    if (KEYBOARD_SHORTCUTS.DELETE.includes(e.key as typeof KEYBOARD_SHORTCUTS.DELETE[number]) && canDelete) {
      e.preventDefault();
      e.stopPropagation();
      setIsConfirmingDelete(true);
    }
  };

  if (isRenaming) {
    return (
      <div
        className="flex items-center gap-2 p-2"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.stopPropagation()}
      >
        <FolderOpen className="size-4 shrink-0 text-muted-foreground" />
        <Input
          ref={renameInputRef}
          value={renameValue}
          onChange={(e) => setRenameValue(e.target.value)}
          className="h-7 flex-1 text-sm"
          maxLength={50}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === 'Enter') {
              e.preventDefault();
              handleConfirmRename();
            }
            if (e.key === 'Escape') {
              e.preventDefault();
              handleCancelRename();
            }
          }}
          onBlur={handleConfirmRename}
          data-testid={`rename-input-${project.id}`}
        />
      </div>
    );
  }

  return (
    <DropdownMenuItem
      onClick={onSelect}
      onKeyDown={handleKeyDown}
      onMouseLeave={handleCancelDelete}
      className="gap-2 p-2 group"
      data-testid={`project-option-${project.id}`}
    >
      <div className="flex size-6 items-center justify-center rounded-md border">
        <FolderOpen className="size-3.5 shrink-0" />
      </div>
      <span className="flex-1">{project.name}</span>
      {isConfirmingDelete ? (
        <div className="flex items-center gap-1" onClick={(e) => e.stopPropagation()}>
          <span className="text-xs text-destructive">Delete?</span>
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6 text-destructive hover:bg-destructive/10"
            onClick={handleDeleteClick}
            aria-label={`Confirm delete ${project.name}`}
            data-testid={`confirm-delete-${project.id}`}
          >
            <Trash2 className="h-3 w-3" />
          </Button>
        </div>
      ) : (
        <>
          {isActive && (
            <span className="text-xs text-muted-foreground">Active</span>
          )}
          <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 group-data-[highlighted]:opacity-100">
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-6 w-6 hover:bg-accent"
                  onClick={handleStartRename}
                  aria-label={`Rename project ${project.name}`}
                  data-testid={`rename-project-${project.id}`}
                >
                  <Pencil className="h-3 w-3" />
                </Button>
              </TooltipTrigger>
              <TooltipContent side="bottom">
                <p>Rename <kbd className="ml-1 rounded bg-muted px-1 font-mono text-xs">R</kbd></p>
              </TooltipContent>
            </Tooltip>
            {canDelete && (
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-6 w-6 hover:bg-destructive/10 hover:text-destructive"
                    onClick={handleDeleteClick}
                    aria-label={`Delete project ${project.name}`}
                    data-testid={`delete-project-${project.id}`}
                  >
                    <Trash2 className="h-3 w-3" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent side="bottom">
                  <p>Delete <kbd className="ml-1 rounded bg-muted px-1 font-mono text-xs">D</kbd></p>
                </TooltipContent>
              </Tooltip>
            )}
          </div>
        </>
      )}
    </DropdownMenuItem>
  );
}
