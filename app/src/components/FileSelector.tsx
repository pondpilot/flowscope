import { useState, useRef, useEffect, useMemo, useCallback } from 'react';
import { ChevronDown, Plus, Upload, FolderUp, Search } from 'lucide-react';
import { useProject } from '@/lib/project-store';
import { Input } from '@/components/ui/input';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { FileTree, getFilesInTreeOrder } from '@/components/FileTree';
import { DEFAULT_FILE_NAMES, ACCEPTED_FILE_TYPES } from '@/lib/constants';

interface FileSelectorProps {
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}

export function FileSelector({ open: controlledOpen, onOpenChange }: FileSelectorProps) {
  const {
    currentProject,
    createFile,
    deleteFile,
    selectFile,
    importFiles,
    toggleFileSelection,
    renameFile,
    isReadOnly,
  } = useProject();

  const [internalOpen, setInternalOpen] = useState(false);
  const [search, setSearch] = useState('');
  const [renamingFileId, setRenamingFileId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState('');
  const [deletingFileId, setDeletingFileId] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const folderInputRef = useRef<HTMLInputElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const renameInputRef = useRef<HTMLInputElement>(null);
  const fileTreeRef = useRef<HTMLDivElement>(null);
  const footerRef = useRef<HTMLDivElement>(null);

  const [focusedFileId, setFocusedFileId] = useState<string | null>(null);
  const [focusZone, setFocusZone] = useState<'search' | 'tree' | 'footer'>('search');

  const open = controlledOpen ?? internalOpen;
  const setOpen = onOpenChange ?? setInternalOpen;

  const activeFile = currentProject?.files.find((f) => f.id === currentProject.activeFileId);

  const filteredFiles = useMemo(() => {
    if (!currentProject?.files) return [];
    if (!search.trim()) return currentProject.files;
    const searchLower = search.toLowerCase();
    return currentProject.files.filter(
      (f) =>
        f.name.toLowerCase().includes(searchLower) || f.path.toLowerCase().includes(searchLower)
    );
  }, [currentProject?.files, search]);

  // Files in visual tree order for keyboard navigation
  const filesInTreeOrder = useMemo(() => {
    return getFilesInTreeOrder(filteredFiles);
  }, [filteredFiles]);

  useEffect(() => {
    if (open && !renamingFileId) {
      // Use setTimeout to ensure focus happens after Radix's internal focus management
      const timer = setTimeout(() => {
        searchInputRef.current?.focus();
        setFocusZone('search');
        setFocusedFileId(null);
      }, 0);
      return () => clearTimeout(timer);
    }
    if (!open) {
      setSearch('');
      setRenamingFileId(null);
      setRenameValue('');
      setDeletingFileId(null);
      setFocusedFileId(null);
      setFocusZone('search');
    }
  }, [open, renamingFileId]);

  useEffect(() => {
    if (renamingFileId && renameInputRef.current) {
      setTimeout(() => renameInputRef.current?.focus(), 0);
    }
  }, [renamingFileId]);

  const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    if (e.target.files && e.target.files.length > 0) {
      importFiles(e.target.files);
    }
    if (fileInputRef.current) {
      fileInputRef.current.value = '';
    }
  };

  const handleFolderUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    if (e.target.files && e.target.files.length > 0) {
      importFiles(e.target.files);
    }
    if (folderInputRef.current) {
      folderInputRef.current.value = '';
    }
  };

  const handleDeleteClick = (e: React.MouseEvent, fileId: string) => {
    e.preventDefault();
    e.stopPropagation();
    if (isReadOnly) return;
    if (deletingFileId === fileId) {
      deleteFile(fileId);
      setDeletingFileId(null);
    } else {
      setDeletingFileId(fileId);
    }
  };

  const handleCancelDelete = () => {
    setDeletingFileId(null);
  };

  const handleCreateFile = () => {
    if (isReadOnly) return;
    createFile(DEFAULT_FILE_NAMES.NEW_QUERY);
    setOpen(false);
  };

  const handleSelectFile = (fileId: string) => {
    selectFile(fileId);
    setOpen(false);
  };

  const isFileIncludedInAnalysis = (fileId: string) => {
    if (!currentProject) return false;
    switch (currentProject.runMode) {
      case 'all':
        return true;
      case 'current':
        return currentProject.activeFileId === fileId;
      case 'custom':
        return currentProject.selectedFileIds?.includes(fileId) ?? false;
      default:
        return false;
    }
  };

  const showCheckboxes = true;

  const handleToggleSelection = (e: React.MouseEvent, fileId: string) => {
    e.preventDefault();
    e.stopPropagation();
    if (currentProject) {
      toggleFileSelection(currentProject.id, fileId);
    }
  };

  const handleToggleSelectionKeyboard = (fileId: string) => {
    if (currentProject) {
      toggleFileSelection(currentProject.id, fileId);
    }
  };

  const handleStartRename = (fileId: string, currentName: string) => {
    if (isReadOnly) return;
    setRenamingFileId(fileId);
    setRenameValue(currentName);
  };

  const handleConfirmRename = () => {
    if (renamingFileId && renameValue.trim()) {
      renameFile(renamingFileId, renameValue.trim());
    }
    setRenamingFileId(null);
    setRenameValue('');
  };

  const handleCancelRename = () => {
    setRenamingFileId(null);
    setRenameValue('');
  };

  const [footerButtonIndex, setFooterButtonIndex] = useState(0);

  const focusFooterButton = useCallback((index: number) => {
    const buttons = footerRef.current?.querySelectorAll(
      '[role="menuitem"]'
    ) as NodeListOf<HTMLElement>;
    if (buttons && buttons[index]) {
      buttons[index].focus();
      setFooterButtonIndex(index);
    }
  }, []);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (renamingFileId) return;

      const fileCount = filesInTreeOrder.length;

      // Find current index of focused file in tree order
      const currentIndex = focusedFileId
        ? filesInTreeOrder.findIndex((f) => f.id === focusedFileId)
        : -1;

      if (e.key === 'Tab') {
        e.preventDefault();
        e.stopPropagation();
        if (e.shiftKey) {
          // Shift+Tab: go backwards
          if (focusZone === 'footer') {
            setFocusZone('tree');
            setFocusedFileId(fileCount > 0 ? filesInTreeOrder[fileCount - 1].id : null);
            fileTreeRef.current?.focus();
          } else if (focusZone === 'tree') {
            setFocusZone('search');
            setFocusedFileId(null);
            searchInputRef.current?.focus();
          } else {
            // From search, go to footer
            setFocusZone('footer');
            setFocusedFileId(null);
            setFooterButtonIndex(0);
            focusFooterButton(0);
          }
        } else {
          // Tab: go forwards
          if (focusZone === 'search') {
            setFocusZone('tree');
            setFocusedFileId(fileCount > 0 ? filesInTreeOrder[0].id : null);
            fileTreeRef.current?.focus();
          } else if (focusZone === 'tree') {
            setFocusZone('footer');
            setFocusedFileId(null);
            setFooterButtonIndex(0);
            focusFooterButton(0);
          } else {
            // From footer, go to search
            setFocusZone('search');
            setFocusedFileId(null);
            searchInputRef.current?.focus();
          }
        }
        return;
      }

      if (e.key === 'ArrowDown') {
        e.preventDefault();
        e.stopPropagation();
        if (focusZone === 'search' && fileCount > 0) {
          setFocusZone('tree');
          setFocusedFileId(filesInTreeOrder[0].id);
          fileTreeRef.current?.focus();
        } else if (focusZone === 'tree' && fileCount > 0) {
          const nextIndex = Math.min(currentIndex + 1, fileCount - 1);
          setFocusedFileId(filesInTreeOrder[nextIndex].id);
        }
        return;
      }

      if (e.key === 'ArrowUp') {
        e.preventDefault();
        e.stopPropagation();
        if (focusZone === 'tree') {
          if (currentIndex <= 0) {
            setFocusZone('search');
            setFocusedFileId(null);
            searchInputRef.current?.focus();
          } else {
            setFocusedFileId(filesInTreeOrder[currentIndex - 1].id);
          }
        }
        return;
      }

      if (e.key === 'ArrowLeft' && focusZone === 'footer') {
        e.preventDefault();
        e.stopPropagation();
        const newIndex = Math.max(0, footerButtonIndex - 1);
        focusFooterButton(newIndex);
        return;
      }

      if (e.key === 'ArrowRight' && focusZone === 'footer') {
        e.preventDefault();
        e.stopPropagation();
        const buttons = footerRef.current?.querySelectorAll('[role="menuitem"]');
        const maxIndex = buttons ? buttons.length - 1 : 0;
        const newIndex = Math.min(maxIndex, footerButtonIndex + 1);
        focusFooterButton(newIndex);
        return;
      }

      if (e.key === 'Enter' && focusZone === 'tree' && focusedFileId) {
        e.preventDefault();
        e.stopPropagation();
        handleSelectFile(focusedFileId);
        return;
      }

      // Space toggles file selection when in tree zone
      if (e.key === ' ' && focusZone === 'tree' && focusedFileId && currentProject) {
        e.preventDefault();
        e.stopPropagation();
        toggleFileSelection(currentProject.id, focusedFileId);
        return;
      }

      // Rename shortcut
      if ((e.key === 'r' || e.key === 'R') && focusZone === 'tree' && focusedFileId) {
        e.preventDefault();
        e.stopPropagation();
        const file = filteredFiles.find((f) => f.id === focusedFileId);
        if (file) {
          handleStartRename(focusedFileId, file.name);
        }
        return;
      }

      // Delete shortcut
      if ((e.key === 'd' || e.key === 'D') && focusZone === 'tree' && focusedFileId) {
        e.preventDefault();
        e.stopPropagation();
        const canDelete = currentProject ? currentProject.files.length > 1 : false;
        if (canDelete) {
          if (deletingFileId === focusedFileId) {
            deleteFile(focusedFileId);
            setDeletingFileId(null);
          } else {
            setDeletingFileId(focusedFileId);
          }
        }
        return;
      }

      // Cancel delete or rename with Escape
      if (e.key === 'Escape') {
        if (deletingFileId) {
          e.preventDefault();
          e.stopPropagation();
          setDeletingFileId(null);
          return;
        }
        if (renamingFileId) {
          e.preventDefault();
          e.stopPropagation();
          handleCancelRename();
          return;
        }
      }

      // Cancel delete with N
      if ((e.key === 'n' || e.key === 'N') && deletingFileId) {
        e.preventDefault();
        e.stopPropagation();
        setDeletingFileId(null);
        return;
      }

      // Confirm delete with Y
      if ((e.key === 'y' || e.key === 'Y') && deletingFileId) {
        e.preventDefault();
        e.stopPropagation();
        deleteFile(deletingFileId);
        setDeletingFileId(null);
        return;
      }
    },
    [
      focusZone,
      focusedFileId,
      filesInTreeOrder,
      filteredFiles,
      renamingFileId,
      deletingFileId,
      currentProject,
      handleSelectFile,
      handleStartRename,
      handleCancelRename,
      deleteFile,
      footerButtonIndex,
      focusFooterButton,
    ]
  );

  return (
    <>
      <DropdownMenu open={open} onOpenChange={setOpen}>
        <DropdownMenuTrigger asChild>
          <button
            className="flex h-[30px] items-center justify-between gap-2 rounded-full border border-border-primary-light dark:border-border-primary-dark bg-background px-4 text-xs transition-all duration-200 ease-pondpilot placeholder:text-muted-foreground focus:outline-hidden focus:border-accent-light dark:focus:border-accent-dark disabled:cursor-not-allowed disabled:opacity-60 [&>span]:line-clamp-1 hover:border-accent-light dark:hover:border-accent-dark min-w-0 shrink"
            data-testid="file-selector-trigger"
          >
            <span className="truncate">{activeFile?.name || 'No file selected'}</span>
            <ChevronDown className="size-4 opacity-50 shrink-0" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          className="w-80 p-0"
          align="start"
          sideOffset={8}
          onCloseAutoFocus={(e) => e.preventDefault()}
          onKeyDown={(e) => {
            handleKeyDown(e);
            if (!isReadOnly) {
              if ((e.key === 'n' || e.key === 'N') && !renamingFileId) {
                e.preventDefault();
                handleCreateFile();
              }
              if ((e.key === 'u' || e.key === 'U') && !renamingFileId) {
                e.preventDefault();
                fileInputRef.current?.click();
              }
              if ((e.key === 'f' || e.key === 'F') && !renamingFileId) {
                e.preventDefault();
                folderInputRef.current?.click();
              }
            }
          }}
        >
          {/* Search Input */}
          <div className="px-3 py-2 border-b">
            <div className="relative flex items-center rounded-full border border-border bg-background h-9 px-2 shadow-xs">
              <Search
                className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 size-3.5 text-muted-foreground"
                strokeWidth={1.5}
              />
              <Input
                ref={searchInputRef}
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Search files..."
                className="h-7 border-0 bg-transparent pl-7 pr-2 text-sm shadow-none placeholder:text-muted-foreground focus-visible:ring-0 rounded-full flex-1"
                onKeyDown={(e) => {
                  if (e.key === 'Escape') {
                    setOpen(false);
                  }
                  if (e.key === 'Enter' && filteredFiles.length > 0) {
                    handleSelectFile(filteredFiles[0].id);
                  }
                }}
                data-testid="file-search-input"
              />
              <kbd className="hidden sm:inline-flex h-5 select-none items-center gap-1 rounded-full border border-border bg-muted px-1.5 font-mono text-[10px] font-medium text-muted-foreground shrink-0">
                <span className="text-xs">⌘</span>O
              </kbd>
            </div>
          </div>

          {/* File Tree */}
          <div
            ref={fileTreeRef}
            className="max-h-[300px] overflow-y-auto outline-hidden"
            tabIndex={-1}
          >
            {filteredFiles.length > 0 ? (
              <FileTree
                files={filteredFiles}
                activeFileId={currentProject?.activeFileId || null}
                selectedFileIds={currentProject?.selectedFileIds || []}
                showCheckboxes={showCheckboxes}
                deletingFileId={deletingFileId}
                renamingFileId={renamingFileId}
                renameValue={renameValue}
                focusedFileId={focusZone === 'tree' ? focusedFileId : null}
                onSelectFile={handleSelectFile}
                onToggleSelection={handleToggleSelection}
                onToggleSelectionKeyboard={handleToggleSelectionKeyboard}
                onStartRename={handleStartRename}
                onConfirmRename={handleConfirmRename}
                onCancelRename={handleCancelRename}
                onRenameValueChange={setRenameValue}
                onDeleteClick={handleDeleteClick}
                onCancelDelete={handleCancelDelete}
                isFileIncludedInAnalysis={isFileIncludedInAnalysis}
                canDeleteFiles={currentProject ? currentProject.files.length > 1 : false}
                renameInputRef={renameInputRef}
                isReadOnly={isReadOnly}
              />
            ) : (
              <div className="py-6 text-center text-sm text-muted-foreground">
                {search ? 'No files found' : 'No files in project'}
              </div>
            )}
          </div>

          {/* Actions - hidden in read-only mode */}
          {!isReadOnly && (
            <>
              <DropdownMenuSeparator className="my-0" />
              <div ref={footerRef} className="p-1 flex flex-col gap-1">
                <div className="flex gap-1">
                  <DropdownMenuItem
                    onClick={handleCreateFile}
                    className="flex-1 gap-2 p-2 cursor-pointer justify-between"
                    data-testid="new-file-btn"
                  >
                    <div className="flex items-center gap-2">
                      <Plus className="size-4" />
                      <span>New</span>
                    </div>
                    <kbd className="rounded bg-muted px-1.5 font-mono text-[10px] text-muted-foreground">
                      N
                    </kbd>
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    onClick={() => fileInputRef.current?.click()}
                    className="flex-1 gap-2 p-2 cursor-pointer justify-between"
                    data-testid="upload-files-btn"
                  >
                    <div className="flex items-center gap-2">
                      <Upload className="size-4" />
                      <span>Files</span>
                    </div>
                    <kbd className="rounded bg-muted px-1.5 font-mono text-[10px] text-muted-foreground">
                      U
                    </kbd>
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    onClick={() => folderInputRef.current?.click()}
                    className="flex-1 gap-2 p-2 cursor-pointer justify-between"
                    data-testid="upload-folder-btn"
                  >
                    <div className="flex items-center gap-2">
                      <FolderUp className="size-4" />
                      <span>Folder</span>
                    </div>
                    <kbd className="rounded bg-muted px-1.5 font-mono text-[10px] text-muted-foreground">
                      F
                    </kbd>
                  </DropdownMenuItem>
                </div>
              </div>
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>

      {/* Hidden file inputs */}
      <input
        type="file"
        multiple
        ref={fileInputRef}
        className="hidden"
        accept={ACCEPTED_FILE_TYPES}
        onChange={handleFileUpload}
        data-testid="file-upload-input"
      />
      <input
        type="file"
        ref={folderInputRef}
        className="hidden"
        onChange={handleFolderUpload}
        data-testid="folder-upload-input"
        {...({ webkitdirectory: '', directory: '' } as React.InputHTMLAttributes<HTMLInputElement>)}
      />
    </>
  );
}
