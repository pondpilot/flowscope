import { useState } from 'react';
import { ChevronDown, Plus, FolderOpen } from 'lucide-react';
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
import { TooltipProvider } from '@/components/ui/tooltip';
import { ProjectMenuItem } from './ProjectMenuItem';

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
    renameProject,
  } = useProject();

  const [isCreating, setIsCreating] = useState(false);
  const [newProjectName, setNewProjectName] = useState('');
  const [internalOpen, setInternalOpen] = useState(false);

  const open = controlledOpen ?? internalOpen;
  const setOpen = onOpenChange ?? setInternalOpen;

  const handleCreate = () => {
    const trimmedName = newProjectName.trim();
    if (trimmedName) {
      createProject(trimmedName);
      setNewProjectName('');
      setIsCreating(false);
      setOpen(false);
    }
  };

  const handleOpenChange = (newOpen: boolean) => {
    setOpen(newOpen);
    if (!newOpen) {
      setIsCreating(false);
      setNewProjectName('');
    }
  };

  const handleSelectProject = (projectId: string) => {
    selectProject(projectId);
    setOpen(false);
  };

  return (
    <DropdownMenu open={open} onOpenChange={handleOpenChange}>
      <DropdownMenuTrigger asChild>
        <button
          className="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm focus:outline-none"
          data-testid="project-selector-trigger"
        >
          <div className="flex size-6 items-center justify-center rounded-md border bg-background">
            <FolderOpen className="size-3.5" />
          </div>
          <span className="font-medium">
            {currentProject?.name || 'Select Project'}
          </span>
          <ChevronDown className="size-4 opacity-50" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        className="min-w-64 rounded-lg"
        align="start"
        sideOffset={8}
      >
        <TooltipProvider delayDuration={300}>
          <DropdownMenuLabel className="text-xs text-muted-foreground flex items-center justify-between">
            <span>Projects</span>
            <kbd className="hidden sm:inline-flex h-5 select-none items-center gap-1 rounded border bg-muted px-1.5 font-mono text-[10px] font-medium">
              <span className="text-xs">⌘</span>P
            </kbd>
          </DropdownMenuLabel>

          {projects.map((project) => (
            <ProjectMenuItem
              key={project.id}
              project={project}
              isActive={activeProjectId === project.id}
              canDelete={projects.length > 1}
              onSelect={() => handleSelectProject(project.id)}
              onRename={(newName) => renameProject(project.id, newName)}
              onDelete={() => deleteProject(project.id)}
            />
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
                  maxLength={50}
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
        </TooltipProvider>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
