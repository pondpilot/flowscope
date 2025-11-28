import { useState, useRef, useEffect, useMemo } from 'react';
import {
  ChevronDown,
  Plus,
  Upload,
  FolderUp,
  Search,
} from 'lucide-react';
import { useProject } from '@/lib/project-store';
import { Input } from '@/components/ui/input';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { FileTree } from '@/components/FileTree';
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
  const folderInputRef = useRef<HTMLInputElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const renameInputRef = useRef<HTMLInputElement>(null);

  const open = controlledOpen ?? internalOpen;
  const setOpen = onOpenChange ?? setInternalOpen;

  const activeFile = currentProject?.files.find(f => f.id === currentProject.activeFileId);

  const filteredFiles = useMemo(() => {
    if (!currentProject?.files) return [];
    if (!search.trim()) return currentProject.files;
    const searchLower = search.toLowerCase();
    return currentProject.files.filter(f =>
      f.name.toLowerCase().includes(searchLower) ||
      f.path.toLowerCase().includes(searchLower)
    );
  }, [currentProject?.files, search]);

  useEffect(() => {
    if (open && searchInputRef.current && !renamingFileId) {
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

  const showCheckboxes = true;

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
            if ((e.key === 'f' || e.key === 'F') && !renamingFileId) {
              e.preventDefault();
              folderInputRef.current?.click();
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
              }}
              data-testid="file-search-input"
            />
          </div>

          {/* File Tree */}
          <div className="max-h-[300px] overflow-y-auto">
            {filteredFiles.length > 0 ? (
              <FileTree
                files={filteredFiles}
                activeFileId={currentProject?.activeFileId || null}
                selectedFileIds={currentProject?.selectedFileIds || []}
                showCheckboxes={showCheckboxes}
                deletingFileId={deletingFileId}
                renamingFileId={renamingFileId}
                renameValue={renameValue}
                onSelectFile={handleSelectFile}
                onToggleSelection={handleToggleSelection}
                onStartRename={handleStartRename}
                onConfirmRename={handleConfirmRename}
                onCancelRename={handleCancelRename}
                onRenameValueChange={setRenameValue}
                onDeleteClick={handleDeleteClick}
                onCancelDelete={handleCancelDelete}
                isFileIncludedInAnalysis={isFileIncludedInAnalysis}
                canDeleteFiles={currentProject ? currentProject.files.length > 1 : false}
                renameInputRef={renameInputRef}
              />
            ) : (
              <div className="py-6 text-center text-sm text-muted-foreground">
                {search ? 'No files found' : 'No files in project'}
              </div>
            )}
          </div>

          {/* Actions */}
          <DropdownMenuSeparator className="my-0" />
          <div className="p-1 flex flex-col gap-1">
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
                <kbd className="rounded bg-muted px-1.5 font-mono text-[10px] text-muted-foreground">N</kbd>
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
                <kbd className="rounded bg-muted px-1.5 font-mono text-[10px] text-muted-foreground">U</kbd>
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
                <kbd className="rounded bg-muted px-1.5 font-mono text-[10px] text-muted-foreground">F</kbd>
              </DropdownMenuItem>
            </div>
          </div>
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
        {...{ webkitdirectory: '', directory: '' } as React.InputHTMLAttributes<HTMLInputElement>}
      />
    </>
  );
}
