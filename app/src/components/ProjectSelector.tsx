import { useState } from 'react';
import { ChevronDown, Plus, Trash2, FolderOpen } from 'lucide-react';
import { useProject } from '@/lib/project-store';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';

interface ProjectSelectorProps {
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}

export function ProjectSelector({ open: controlledOpen, onOpenChange }: ProjectSelectorProps) {
  const {
    projects,
    activeProjectId,
    currentProject,
    selectProject,
    createProject,
    deleteProject,
  } = useProject();

  const [isCreating, setIsCreating] = useState(false);
  const [newProjectName, setNewProjectName] = useState('');
  const [internalOpen, setInternalOpen] = useState(false);

  const open = controlledOpen ?? internalOpen;
  const setOpen = onOpenChange ?? setInternalOpen;

  const handleCreate = () => {
    if (newProjectName.trim()) {
      createProject(newProjectName.trim());
      setNewProjectName('');
      setIsCreating(false);
    }
  };

  const handleDelete = (e: React.MouseEvent, projectId: string) => {
    e.preventDefault();
    e.stopPropagation();
    deleteProject(projectId);
  };

  const handleOpenChange = (newOpen: boolean) => {
    setOpen(newOpen);
    if (!newOpen) {
      setIsCreating(false);
      setNewProjectName('');
    }
  };

  return (
    <DropdownMenu open={open} onOpenChange={handleOpenChange}>
      <DropdownMenuTrigger asChild>
        <button
          className="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-accent focus:outline-none"
          data-testid="project-selector-trigger"
        >
          <div className="flex size-6 items-center justify-center rounded-md border bg-background">
            <FolderOpen className="size-3.5" />
          </div>
          <span className="font-medium">
            {currentProject?.name || 'Select Project'}
          </span>
          <kbd className="hidden sm:inline-flex h-5 select-none items-center gap-1 rounded border bg-muted px-1.5 font-mono text-[10px] font-medium text-muted-foreground">
            <span className="text-xs">⌘</span>P
          </kbd>
          <ChevronDown className="size-4 opacity-50" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        className="min-w-64 rounded-lg"
        align="start"
        sideOffset={8}
      >
        <DropdownMenuLabel className="text-xs text-muted-foreground">
          Projects
        </DropdownMenuLabel>

        {projects.map((project) => (
          <DropdownMenuItem
            key={project.id}
            onClick={() => {
              selectProject(project.id);
              setOpen(false);
            }}
            className="gap-2 p-2 group"
            data-testid={`project-option-${project.id}`}
          >
            <div className="flex size-6 items-center justify-center rounded-md border">
              <FolderOpen className="size-3.5 shrink-0" />
            </div>
            <span className="flex-1">
              {project.name}
            </span>
            {activeProjectId === project.id && (
              <span className="text-xs text-muted-foreground">Active</span>
            )}
            {projects.length > 1 && (
              <Button
                variant="ghost"
                size="icon"
                className="h-6 w-6 opacity-0 group-hover:opacity-100 hover:bg-destructive/10 hover:text-destructive"
                onClick={(e) => handleDelete(e, project.id)}
                data-testid={`delete-project-${project.id}`}
              >
                <Trash2 className="h-3 w-3" />
              </Button>
            )}
          </DropdownMenuItem>
        ))}

        <DropdownMenuSeparator />

        {isCreating ? (
          <div
            className="p-2"
            onClick={(e) => e.stopPropagation()}
            onKeyDown={(e) => e.stopPropagation()}
          >
            <div className="flex gap-2">
              <Input
                value={newProjectName}
                onChange={(e) => setNewProjectName(e.target.value)}
                placeholder="Project name"
                className="h-8 text-sm"
                autoFocus
                onKeyDown={(e) => {
                  e.stopPropagation();
                  if (e.key === 'Enter') {
                    e.preventDefault();
                    handleCreate();
                  }
                  if (e.key === 'Escape') {
                    e.preventDefault();
                    setIsCreating(false);
                    setNewProjectName('');
                  }
                }}
                data-testid="new-project-input"
              />
              <Button
                size="sm"
                className="h-8 px-3"
                onClick={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  handleCreate();
                }}
                disabled={!newProjectName.trim()}
                data-testid="create-project-btn"
              >
                Add
              </Button>
            </div>
          </div>
        ) : (
          <DropdownMenuItem
            onSelect={(e) => {
              e.preventDefault();
              setIsCreating(true);
            }}
            className="gap-2 p-2"
            data-testid="new-project-btn"
          >
            <div className="flex size-6 items-center justify-center rounded-md border bg-transparent">
              <Plus className="size-4" />
            </div>
            <span className="font-medium">Add project</span>
          </DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
