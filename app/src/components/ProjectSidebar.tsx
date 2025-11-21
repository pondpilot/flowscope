import { useState } from 'react';
import { ChevronsUpDown, Folder, File } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';

interface File {
  name: string;
}

interface Project {
  name: string;
  files: File[];
}

const MOCK_PROJECTS: Project[] = [
  {
    name: 'Project Alpha',
    files: [
      { name: '01_users.sql' },
      { name: '02_orders.sql' },
      { name: '03_products.sql' },
    ],
  },
  {
    name: 'Project Beta',
    files: [
      { name: 'main_dashboard.sql' },
      { name: 'user_segmentation.sql' },
    ],
  },
];

export function ProjectSidebar() {
  const [openProjects, setOpenProjects] = useState<string[]>(
    MOCK_PROJECTS.map((p) => p.name)
  );

  const toggleProject = (projectName: string) => {
    setOpenProjects((prev) =>
      prev.includes(projectName)
        ? prev.filter((p) => p !== projectName)
        : [...prev, projectName]
    );
  };

  return (
    <div className="flex flex-col h-full bg-muted/40">
      <div className="p-4 border-b">
        <h2 className="text-lg font-semibold">Projects</h2>
      </div>
      <div className="flex-1 p-2 space-y-2 overflow-auto">
        {MOCK_PROJECTS.map((project) => (
          <Collapsible
            key={project.name}
            open={openProjects.includes(project.name)}
            onOpenChange={() => toggleProject(project.name)}
          >
            <CollapsibleTrigger asChild>
              <Button variant="ghost" className="w-full justify-start gap-2">
                <Folder className="h-4 w-4" />
                <span className="flex-1 text-left">{project.name}</span>
                <ChevronsUpDown className="h-4 w-4" />
              </Button>
            </CollapsibleTrigger>
            <CollapsibleContent className="pl-6 space-y-1 py-1">
              {project.files.map((file) => (
                <Button
                  key={file.name}
                  variant="ghost"
                  className="w-full justify-start gap-2"
                  size="sm"
                >
                  <File className="h-4 w-4" />
                  {file.name}
                </Button>
              ))}
            </CollapsibleContent>
          </Collapsible>
        ))}
      </div>
    </div>
  );
}