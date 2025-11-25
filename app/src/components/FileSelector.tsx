import { useState, useRef, useEffect, useMemo } from 'react';
import {
  ChevronDown,
  Plus,
  Upload,
  Trash2,
  FileCode,
  Search,
  Pencil,
} from 'lucide-react';
import { useProject } from '@/lib/project-store';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
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
  } = useProject();

  const [internalOpen, setInternalOpen] = useState(false);
  const [search, setSearch] = useState('');
  const [renamingFileId, setRenamingFileId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState('');
  const [deletingFileId, setDeletingFileId] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const renameInputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const open = controlledOpen ?? internalOpen;
  const setOpen = onOpenChange ?? setInternalOpen;

  const activeFile = currentProject?.files.find(f => f.id === currentProject.activeFileId);

  const filteredFiles = useMemo(() => {
    if (!currentProject?.files) return [];
    if (!search.trim()) return currentProject.files;
    const searchLower = search.toLowerCase();
    return currentProject.files.filter(f =>
      f.name.toLowerCase().includes(searchLower)
    );
  }, [currentProject?.files, search]);

  useEffect(() => {
    if (open && searchInputRef.current && !renamingFileId) {
      // Use requestAnimationFrame to ensure DOM is ready after Radix animation
      requestAnimationFrame(() => {
        searchInputRef.current?.focus();
      });
    }
    if (!open) {
      setSearch('');
      setRenamingFileId(null);
      setRenameValue('');
      setDeletingFileId(null);
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

  const handleDeleteClick = (e: React.MouseEvent, fileId: string) => {
    e.preventDefault();
    e.stopPropagation();
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

  const showCheckboxes = true; // Always show checkboxes for file selection

  const handleToggleSelection = (e: React.MouseEvent, fileId: string) => {
    e.preventDefault();
    e.stopPropagation();
    if (currentProject) {
      toggleFileSelection(currentProject.id, fileId);
    }
  };

  const handleStartRename = (fileId: string, currentName: string) => {
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

  return (
    <>
      <DropdownMenu open={open} onOpenChange={setOpen}>
        <DropdownMenuTrigger asChild>
          <button
            className="flex h-8 items-center justify-between gap-2 rounded-md border border-input bg-background px-3 text-xs ring-offset-background placeholder:text-muted-foreground focus:outline-none disabled:cursor-not-allowed disabled:opacity-50 [&>span]:line-clamp-1 hover:bg-accent hover:text-accent-foreground min-w-0 shrink"
            data-testid="file-selector-trigger"
          >
            <span className="truncate">
              {activeFile?.name || 'No file selected'}
            </span>
            <kbd className="hidden sm:inline-flex h-5 select-none items-center gap-1 rounded border bg-muted px-1.5 font-mono text-[10px] font-medium text-muted-foreground">
              <span className="text-xs">⌘</span>O
            </kbd>
            <ChevronDown className="size-4 opacity-50 shrink-0" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          className="w-80 p-0"
          align="start"
          sideOffset={8}
          onCloseAutoFocus={(e) => e.preventDefault()}
          onKeyDown={(e) => {
            if ((e.key === 'n' || e.key === 'N') && !renamingFileId) {
              e.preventDefault();
              handleCreateFile();
            }
            if ((e.key === 'u' || e.key === 'U') && !renamingFileId) {
              e.preventDefault();
              fileInputRef.current?.click();
            }
          }}
        >
          {/* Search Input */}
          <div className="flex items-center border-b px-3 py-2">
            <Search className="size-4 text-muted-foreground mr-2" />
            <Input
              ref={searchInputRef}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search files..."
              className="h-8 border-0 p-0 focus-visible:ring-0 focus-visible:ring-offset-0"
              onKeyDown={(e) => {
                if (e.key === 'Escape') {
                  setOpen(false);
                }
                if (e.key === 'Enter' && filteredFiles.length > 0) {
                  handleSelectFile(filteredFiles[0].id);
                }
                if (e.key === 'ArrowDown') {
                  e.preventDefault();
                  const firstItem = listRef.current?.querySelector('[role="menuitem"]') as HTMLElement;
                  firstItem?.focus();
                }
              }}
              data-testid="file-search-input"
            />
          </div>

          {/* File List */}
          <ScrollArea className="max-h-[300px]">
            <div className="p-1" ref={listRef}>
              {filteredFiles.length > 0 ? (
                filteredFiles.map((file) => {
                  const isActive = currentProject?.activeFileId === file.id;
                  const isIncluded = isFileIncludedInAnalysis(file.id);
                  const isSelected = currentProject?.selectedFileIds?.includes(file.id) ?? false;
                  const isRenaming = renamingFileId === file.id;

                  if (isRenaming) {
                    return (
                      <div
                        key={file.id}
                        className="flex items-center gap-2 p-2"
                        onClick={(e) => e.stopPropagation()}
                      >
                        <FileCode className="size-4 shrink-0 text-muted-foreground" />
                        <Input
                          ref={renameInputRef}
                          value={renameValue}
                          onChange={(e) => setRenameValue(e.target.value)}
                          className="h-7 flex-1 text-sm"
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
                          data-testid={`rename-input-${file.id}`}
                        />
                      </div>
                    );
                  }

                  return (
                    <DropdownMenuItem
                      key={file.id}
                      onClick={() => handleSelectFile(file.id)}
                      onKeyDown={(e) => {
                        // When in delete confirmation mode
                        if (deletingFileId === file.id) {
                          if (e.key === 'Enter' || e.key === 'y' || e.key === 'Y' || e.key === 'd' || e.key === 'D') {
                            e.preventDefault();
                            e.stopPropagation();
                            deleteFile(file.id);
                            setDeletingFileId(null);
                            return;
                          }
                          if (e.key === 'Escape' || e.key === 'n' || e.key === 'N') {
                            e.preventDefault();
                            e.stopPropagation();
                            handleCancelDelete();
                            return;
                          }
                        }

                        if (e.key === ' ' && showCheckboxes) {
                          e.preventDefault();
                          e.stopPropagation();
                          if (currentProject) {
                            toggleFileSelection(currentProject.id, file.id);
                          }
                        }
                        if (e.key === 'r' || e.key === 'R') {
                          e.preventDefault();
                          e.stopPropagation();
                          handleStartRename(file.id, file.name);
                        }
                        if ((e.key === 'd' || e.key === 'D') && currentProject && currentProject.files.length > 1) {
                          e.preventDefault();
                          e.stopPropagation();
                          setDeletingFileId(file.id);
                        }
                      }}
                      className="flex items-center gap-2 p-2 group cursor-pointer"
                      data-testid={`file-option-${file.id}`}
                    >
                      {showCheckboxes && (
                        <Checkbox
                          checked={isSelected}
                          onClick={(e) => handleToggleSelection(e, file.id)}
                          className="shrink-0 border-muted-foreground group-data-[highlighted]:border-accent-foreground"
                          data-testid={`file-checkbox-${file.id}`}
                        />
                      )}
                      <FileCode className={cn(
                        "size-4 shrink-0 group-data-[highlighted]:text-accent-foreground",
                        isIncluded ? "text-primary" : "text-muted-foreground"
                      )} />
                      <span className={cn(
                        "flex-1 truncate",
                        isActive && "font-semibold italic"
                      )}>{file.name}</span>
                      {deletingFileId === file.id ? (
                        <div className="flex items-center gap-1" onClick={(e) => e.stopPropagation()}>
                          <span className="text-xs text-destructive">Delete?</span>
                          <Button
                            variant="ghost"
                            size="icon"
                            className="h-6 w-6 text-destructive hover:bg-destructive/10"
                            onClick={(e) => handleDeleteClick(e, file.id)}
                            data-testid={`confirm-delete-${file.id}`}
                          >
                            <Trash2 className="h-3 w-3" />
                          </Button>
                        </div>
                      ) : (
                        <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 group-data-[highlighted]:opacity-100">
                          <TooltipProvider delayDuration={300}>
                            <Tooltip>
                              <TooltipTrigger asChild>
                                <Button
                                  variant="ghost"
                                  size="icon"
                                  className="h-6 w-6 hover:bg-accent"
                                  onClick={(e) => {
                                    e.preventDefault();
                                    e.stopPropagation();
                                    handleStartRename(file.id, file.name);
                                  }}
                                  data-testid={`rename-file-${file.id}`}
                                >
                                  <Pencil className="h-3 w-3" />
                                </Button>
                              </TooltipTrigger>
                              <TooltipContent side="bottom">
                                <p>Rename <kbd className="ml-1 rounded bg-muted px-1 font-mono text-xs">R</kbd></p>
                              </TooltipContent>
                            </Tooltip>
                          </TooltipProvider>
                          {currentProject && currentProject.files.length > 1 && (
                            <TooltipProvider delayDuration={300}>
                              <Tooltip>
                                <TooltipTrigger asChild>
                                  <Button
                                    variant="ghost"
                                    size="icon"
                                    className="h-6 w-6 hover:bg-destructive/10 hover:text-destructive"
                                    onClick={(e) => handleDeleteClick(e, file.id)}
                                    data-testid={`delete-file-${file.id}`}
                                  >
                                    <Trash2 className="h-3 w-3" />
                                  </Button>
                                </TooltipTrigger>
                                <TooltipContent side="bottom">
                                  <p>Delete <kbd className="ml-1 rounded bg-muted px-1 font-mono text-xs">D</kbd></p>
                                </TooltipContent>
                              </Tooltip>
                            </TooltipProvider>
                          )}
                        </div>
                      )}
                    </DropdownMenuItem>
                  );
                })
              ) : (
                <div className="py-6 text-center text-sm text-muted-foreground">
                  {search ? 'No files found' : 'No files in project'}
                </div>
              )}
            </div>
          </ScrollArea>

          {/* Actions */}
          <DropdownMenuSeparator className="my-0" />
          <div className="p-1 flex gap-1">
            <DropdownMenuItem
              onClick={handleCreateFile}
              className="flex-1 gap-2 p-2 cursor-pointer justify-between"
              data-testid="new-file-btn"
            >
              <div className="flex items-center gap-2">
                <Plus className="size-4" />
                <span>New File</span>
              </div>
              <kbd className="rounded bg-muted px-1.5 font-mono text-[10px] text-muted-foreground">N</kbd>
            </DropdownMenuItem>
            <DropdownMenuItem
              onClick={() => fileInputRef.current?.click()}
              className="flex-1 gap-2 p-2 cursor-pointer justify-between"
              data-testid="upload-files-btn"
            >
              <div className="flex items-center gap-2">
                <Upload className="size-4" />
                <span>Upload</span>
              </div>
              <kbd className="rounded bg-muted px-1.5 font-mono text-[10px] text-muted-foreground">U</kbd>
            </DropdownMenuItem>
          </div>
        </DropdownMenuContent>
      </DropdownMenu>

      <input
        type="file"
        multiple
        ref={fileInputRef}
        className="hidden"
        accept={ACCEPTED_FILE_TYPES}
        onChange={handleFileUpload}
        data-testid="file-upload-input"
      />
    </>
  );
}
