import { useState, useRef } from 'react';
import { Plus, FileCode, Trash2, MoreVertical, FolderOpen, Upload, Check, Beaker } from 'lucide-react';
import { useProject } from '../lib/project-store';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { cn } from '@/lib/utils';


export function FileExplorer() {
  const { 
    projects, 
    activeProjectId, 
    currentProject, 
    selectProject, 
    createProject,
    createFile, 
    deleteFile, 
    selectFile,
    importFiles,
    toggleFileSelection,
    setRunMode
  } = useProject();

  const [isCreatingProject, setIsCreatingProject] = useState(false);
  const [newProjectName, setNewProjectName] = useState('');
  const fileInputRef = useRef<HTMLInputElement>(null);

  const handleCreateProject = () => {
    if (newProjectName.trim()) {
      createProject(newProjectName);
      setIsCreatingProject(false);
      setNewProjectName('');
    }
  };

  const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    if (e.target.files && e.target.files.length > 0) {
      importFiles(e.target.files);
    }
    if (fileInputRef.current) {
      fileInputRef.current.value = '';
    }
  };

  const handleFileCheck = (e: React.MouseEvent, fileId: string) => {
    e.stopPropagation();
    if (currentProject) {
      // If we click check, we implicitly want custom mode, unless we uncheck everything?
      // Let's just toggle and let the user set mode via run button or logic
      toggleFileSelection(currentProject.id, fileId);
      
      // Optional: Auto-switch to custom mode if not already?
      // Let's keep it manual control for now to avoid jumping state,
      // OR switch to 'custom' if they start checking things.
      if (currentProject.runMode !== 'custom') {
        setRunMode(currentProject.id, 'custom');
      }
    }
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

  return (
    <div className="flex flex-col h-full bg-muted/10 border-r">
      {/* Project Selector */}
      <div className="p-4 border-b space-y-4 bg-background/50 backdrop-blur-sm sticky top-0 z-10">
        <div className="flex items-center justify-between">
          <h2 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Project</h2>
          <Button variant="ghost" size="icon" className="h-6 w-6" onClick={() => setIsCreatingProject(true)} title="New Project">
            <Plus className="h-4 w-4" />
          </Button>
        </div>
        
        {isCreatingProject ? (
          <div className="flex gap-2 animate-in slide-in-from-top-2 fade-in">
            <Input 
              value={newProjectName} 
              onChange={(e) => setNewProjectName(e.target.value)}
              placeholder="Project Name"
              className="h-8 text-xs"
              autoFocus
              onKeyDown={(e) => e.key === 'Enter' && handleCreateProject()}
            />
            <Button size="sm" className="h-8" onClick={handleCreateProject}>Add</Button>
          </div>
        ) : (
          <Select value={activeProjectId || ''} onValueChange={selectProject}>
            <SelectTrigger className="w-full h-9 text-sm">
              <SelectValue placeholder="Select Project" />
            </SelectTrigger>
            <SelectContent>
              {projects.map(p => (
                <SelectItem key={p.id} value={p.id}>{p.name}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
      </div>

      {/* File Actions */}
      <div className="px-3 py-2 flex items-center gap-2 border-b bg-muted/30">
        <Button 
          variant="secondary" 
          size="sm" 
          className="flex-1 h-7 text-xs bg-background shadow-sm hover:bg-accent" 
          onClick={() => createFile('new_query.sql')}
        >
          <Plus className="h-3 w-3 mr-1" /> New File
        </Button>
        <Button 
          variant="outline" 
          size="icon" 
          className="h-7 w-7"
          onClick={() => fileInputRef.current?.click()}
          title="Upload Files"
        >
          <Upload className="h-3 w-3" />
        </Button>
        <input 
          type="file" 
          multiple 
          ref={fileInputRef} 
          className="hidden" 
          accept=".sql,.json,.txt"
          onChange={handleFileUpload}
        />
      </div>

      {/* File List */}
      <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
        {currentProject?.files.map((file) => {
          const isIncluded = isFileIncludedInAnalysis(file.id);
          const isSelected = currentProject.selectedFileIds?.includes(file.id);
          const isActive = currentProject.activeFileId === file.id;

          return (
            <div 
              key={file.id} 
              className={cn(
                "group flex items-center gap-2 px-2 py-1.5 rounded-md cursor-pointer transition-all text-sm relative",
                isActive 
                  ? "bg-primary/10 text-foreground font-medium" 
                  : "hover:bg-muted/80 text-muted-foreground hover:text-foreground"
              )}
              onClick={() => selectFile(file.id)}
            >
              {/* Inclusion Marker (Left Border) */}
              {isIncluded && (
                <div className="absolute left-0 top-1 bottom-1 w-0.5 bg-brand-blue-500 rounded-r-full" />
              )}

              {/* Checkbox for Custom Selection */}
              <div 
                className={cn(
                  "shrink-0 flex items-center justify-center w-4 h-4 rounded border transition-colors",
                  isSelected 
                    ? "bg-primary border-primary text-primary-foreground" 
                    : "border-input hover:border-foreground/50 bg-background",
                  // Hide checkbox if not checked and not hovering, unless in custom mode
                  !isSelected && currentProject.runMode !== 'custom' && "opacity-0 group-hover:opacity-100"
                )}
                onClick={(e) => handleFileCheck(e, file.id)}
              >
                {isSelected && <Check className="h-3 w-3" />}
              </div>

              <FileCode className={cn(
                "h-4 w-4 shrink-0 transition-colors",
                isIncluded ? "text-brand-blue-500" : "opacity-50"
              )} />
              
              <span className="truncate flex-1">{file.name}</span>
              
              {/* Right-side Marker for clarity */}
              {isIncluded && (
                <Beaker className="h-3 w-3 text-brand-blue-500 opacity-50 shrink-0" />
              )}

              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button 
                    variant="ghost" 
                    size="icon" 
                    className="h-6 w-6 opacity-0 group-hover:opacity-100 transition-opacity ml-auto shrink-0 hover:bg-background/50"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <MoreVertical className="h-3 w-3" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuItem onClick={() => deleteFile(file.id)} className="text-destructive focus:text-destructive">
                    <Trash2 className="h-4 w-4 mr-2" /> Delete
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          );
        })}
        
        {(!currentProject || currentProject.files.length === 0) && (
          <div className="flex flex-col items-center justify-center h-32 text-muted-foreground text-xs text-center px-4 select-none">
            <FolderOpen className="h-8 w-8 mb-2 opacity-20" />
            <p>No files in this project.</p>
            <p className="opacity-50 mt-1">Create or upload one to get started.</p>
          </div>
        )}
      </div>
    </div>
  );
}