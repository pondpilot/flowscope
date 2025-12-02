import { useState, useMemo, useEffect, useRef } from 'react';
import {
  ChevronRight,
  ChevronDown,
  Folder,
  FolderOpen,
  FileCode,
  Pencil,
  Trash2,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import type { ProjectFile } from '@/lib/project-store';

interface FileTreeProps {
  files: ProjectFile[];
  activeFileId: string | null;
  selectedFileIds: string[];
  showCheckboxes: boolean;
  deletingFileId: string | null;
  renamingFileId: string | null;
  renameValue: string;
  focusedFileId?: string | null;
  onSelectFile: (fileId: string) => void;
  onToggleSelection: (e: React.MouseEvent, fileId: string) => void;
  onStartRename: (fileId: string, currentName: string) => void;
  onConfirmRename: () => void;
  onCancelRename: () => void;
  onRenameValueChange: (value: string) => void;
  onDeleteClick: (e: React.MouseEvent, fileId: string) => void;
  onCancelDelete: () => void;
  isFileIncludedInAnalysis: (fileId: string) => boolean;
  canDeleteFiles: boolean;
  renameInputRef: React.RefObject<HTMLInputElement>;
}

interface TreeNode {
  name: string;
  path: string;
  file?: ProjectFile;
  children: Map<string, TreeNode>;
}

function buildFileTree(files: ProjectFile[]): TreeNode {
  const root: TreeNode = { name: '', path: '', children: new Map() };

  for (const file of files) {
    const parts = file.path.split('/').filter(Boolean);
    let current = root;

    for (let i = 0; i < parts.length; i++) {
      const part = parts[i];
      const isFile = i === parts.length - 1;
      const pathSoFar = parts.slice(0, i + 1).join('/');

      if (!current.children.has(part)) {
        current.children.set(part, {
          name: part,
          path: pathSoFar,
          file: isFile ? file : undefined,
          children: new Map(),
        });
      } else if (isFile) {
        // Update the existing node with file info
        const node = current.children.get(part)!;
        node.file = file;
      }

      current = current.children.get(part)!;
    }
  }

  return root;
}

function sortTreeNodes(nodes: TreeNode[]): TreeNode[] {
  return nodes.sort((a, b) => {
    // Folders first, then files
    const aIsFolder = a.children.size > 0 && !a.file;
    const bIsFolder = b.children.size > 0 && !b.file;
    if (aIsFolder && !bIsFolder) return -1;
    if (!aIsFolder && bIsFolder) return 1;
    // Alphabetical within same type
    return a.name.localeCompare(b.name);
  });
}

// Returns files in the order they appear visually in the tree (depth-first traversal)
export function getFilesInTreeOrder(files: ProjectFile[]): ProjectFile[] {
  const tree = buildFileTree(files);
  const result: ProjectFile[] = [];

  function traverse(node: TreeNode) {
    const sortedChildren = sortTreeNodes(Array.from(node.children.values()));
    for (const child of sortedChildren) {
      if (child.file) {
        result.push(child.file);
      }
      if (child.children.size > 0) {
        traverse(child);
      }
    }
  }

  traverse(tree);
  return result;
}

interface FolderNodeProps {
  node: TreeNode;
  depth: number;
  props: FileTreeProps;
  expandedFolders: Set<string>;
  onToggleFolder: (path: string) => void;
}

function FolderNode({ node, depth, props, expandedFolders, onToggleFolder }: FolderNodeProps) {
  const isExpanded = expandedFolders.has(node.path);
  const sortedChildren = sortTreeNodes(Array.from(node.children.values()));

  return (
    <div role="treeitem" aria-expanded={isExpanded}>
      <div
        className={cn(
          'flex items-center gap-1 py-1 px-2 rounded-md cursor-pointer hover:bg-muted/50 group',
          'text-sm text-muted-foreground'
        )}
        style={{ paddingLeft: `${depth * 12 + 8}px` }}
        onClick={() => onToggleFolder(node.path)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            onToggleFolder(node.path);
          }
        }}
        tabIndex={-1}
        role="button"
        aria-label={`${isExpanded ? 'Collapse' : 'Expand'} folder ${node.name}`}
      >
        {isExpanded ? (
          <ChevronDown className="size-4 shrink-0" />
        ) : (
          <ChevronRight className="size-4 shrink-0" />
        )}
        {isExpanded ? (
          <FolderOpen className="size-4 shrink-0 text-amber-500" />
        ) : (
          <Folder className="size-4 shrink-0 text-amber-500" />
        )}
        <span className="truncate">{node.name}</span>
      </div>
      {isExpanded && (
        <div role="group">
          {sortedChildren.map(child =>
            child.file ? (
              <FileNode
                key={child.file.id}
                node={child}
                depth={depth + 1}
                props={props}
              />
            ) : (
              <FolderNode
                key={child.path}
                node={child}
                depth={depth + 1}
                props={props}
                expandedFolders={expandedFolders}
                onToggleFolder={onToggleFolder}
              />
            )
          )}
        </div>
      )}
    </div>
  );
}

interface FileNodeProps {
  node: TreeNode;
  depth: number;
  props: FileTreeProps;
}

function FileNode({ node, depth, props }: FileNodeProps) {
  const file = node.file!;
  const {
    activeFileId,
    selectedFileIds,
    showCheckboxes,
    deletingFileId,
    renamingFileId,
    renameValue,
    focusedFileId,
    onSelectFile,
    onToggleSelection,
    onStartRename,
    onConfirmRename,
    onCancelRename,
    onRenameValueChange,
    onDeleteClick,
    isFileIncludedInAnalysis,
    canDeleteFiles,
    renameInputRef,
  } = props;

  const isActive = activeFileId === file.id;
  const isIncluded = isFileIncludedInAnalysis(file.id);
  const isSelected = selectedFileIds.includes(file.id);
  const isRenaming = renamingFileId === file.id;
  const isDeleting = deletingFileId === file.id;
  const isFocused = focusedFileId === file.id;

  if (isRenaming) {
    return (
      <div
        className="flex items-center gap-2 py-1 px-2"
        style={{ paddingLeft: `${depth * 12 + 8}px` }}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.stopPropagation()}
      >
        <FileCode className="size-4 shrink-0 text-muted-foreground" />
        <Input
          ref={renameInputRef}
          value={renameValue}
          onChange={(e) => onRenameValueChange(e.target.value)}
          className="h-7 flex-1 text-sm"
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === 'Enter') {
              e.preventDefault();
              onConfirmRename();
            }
            if (e.key === 'Escape') {
              e.preventDefault();
              onCancelRename();
            }
          }}
          onBlur={onConfirmRename}
          data-testid={`rename-input-${file.id}`}
        />
      </div>
    );
  }

  const itemRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (isFocused && itemRef.current) {
      itemRef.current.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    }
  }, [isFocused]);

  return (
    <div
      ref={itemRef}
      role="treeitem"
      aria-selected={isActive}
      className={cn(
        'flex items-center gap-2 py-1 px-2 rounded-md cursor-pointer hover:bg-muted/50 group',
        isFocused && 'bg-muted'
      )}
      style={{ paddingLeft: `${depth * 12 + 8}px` }}
      onClick={() => onSelectFile(file.id)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onSelectFile(file.id);
        }
      }}
      tabIndex={-1}
      data-testid={`file-tree-item-${file.id}`}
    >
      {showCheckboxes && (
        <Checkbox
          checked={isSelected}
          onClick={(e) => onToggleSelection(e, file.id)}
          className="shrink-0 border-muted-foreground"
          data-testid={`file-checkbox-${file.id}`}
        />
      )}
      <FileCode
        className={cn(
          'size-4 shrink-0',
          isIncluded ? 'text-primary' : 'text-muted-foreground'
        )}
      />
      <span
        className={cn(
          'flex-1 truncate text-sm',
          isActive && 'font-semibold italic'
        )}
      >
        {file.name}
      </span>
      {isDeleting ? (
        <div className="flex items-center gap-1" onClick={(e) => e.stopPropagation()}>
          <span className="text-xs text-destructive">Delete?</span>
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6 text-destructive hover:bg-destructive/10"
            onClick={(e) => onDeleteClick(e, file.id)}
            data-testid={`confirm-delete-${file.id}`}
          >
            <Trash2 className="h-3 w-3" />
          </Button>
        </div>
      ) : (
        <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100">
          <TooltipProvider delayDuration={300}>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-6 w-6 hover:bg-background/50"
                  onClick={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    onStartRename(file.id, file.name);
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
          {canDeleteFiles && (
            <TooltipProvider delayDuration={300}>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-6 w-6 hover:bg-background/50 hover:text-destructive"
                    onClick={(e) => onDeleteClick(e, file.id)}
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
    </div>
  );
}

export function FileTree(props: FileTreeProps) {
  const { files } = props;
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set());

  const tree = useMemo(() => buildFileTree(files), [files]);

  // Check if we have any nested structure
  const hasNestedStructure = useMemo(() => {
    return files.some(f => f.path.includes('/'));
  }, [files]);

  // Auto-expand all folders on initial render if there's nested structure
  useEffect(() => {
    if (!hasNestedStructure || expandedFolders.size > 0) {
      return;
    }

    const allFolderPaths = new Set<string>();
    for (const file of files) {
      const parts = file.path.split('/');
      for (let i = 1; i < parts.length; i++) {
        allFolderPaths.add(parts.slice(0, i).join('/'));
      }
    }
    if (allFolderPaths.size > 0) {
      setExpandedFolders(allFolderPaths);
    }
  }, [files, hasNestedStructure, expandedFolders.size]);

  const toggleFolder = (path: string) => {
    setExpandedFolders(prev => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  };

  const sortedChildren = sortTreeNodes(Array.from(tree.children.values()));

  // If no nested structure, render flat list (no need for tree)
  if (!hasNestedStructure) {
    return (
      <div className="p-1" role="tree" aria-label="File list">
        {sortedChildren.map(node => (
          <FileNode key={node.file?.id} node={node} depth={0} props={props} />
        ))}
      </div>
    );
  }

  return (
    <div className="p-1" role="tree" aria-label="File tree">
      {sortedChildren.map(node =>
        node.file ? (
          <FileNode key={node.file.id} node={node} depth={0} props={props} />
        ) : (
          <FolderNode
            key={node.path}
            node={node}
            depth={0}
            props={props}
            expandedFolders={expandedFolders}
            onToggleFolder={toggleFolder}
          />
        )
      )}
    </div>
  );
}
