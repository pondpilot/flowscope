import { useState, useCallback, useEffect, useMemo } from 'react';
import { Copy, Check, AlertTriangle, FileText } from 'lucide-react';
import { toast } from 'sonner';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Checkbox } from './ui/checkbox';
import { Label } from './ui/label';
import { ScrollArea } from './ui/scroll-area';
import { encodeProject, buildShareUrl, formatBytes } from '@/lib/share';
import type { Project } from '@/lib/project-store';

interface ShareDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  project: Project;
}

export function ShareDialog({ open, onOpenChange, project }: ShareDialogProps) {
  const [selectedFileIds, setSelectedFileIds] = useState<Set<string>>(new Set());
  const [includeSchema, setIncludeSchema] = useState(true);
  const [copied, setCopied] = useState(false);

  // Initialize all files as selected when dialog opens
  useEffect(() => {
    if (open) {
      setSelectedFileIds(new Set(project.files.map(f => f.id)));
      setIncludeSchema(true);
      setCopied(false);
    }
  }, [open, project.files]);

  const toggleFile = useCallback((fileId: string) => {
    setSelectedFileIds(prev => {
      const next = new Set(prev);
      if (next.has(fileId)) {
        next.delete(fileId);
      } else {
        next.add(fileId);
      }
      return next;
    });
  }, []);

  const toggleAll = useCallback(() => {
    if (selectedFileIds.size === project.files.length) {
      setSelectedFileIds(new Set());
    } else {
      setSelectedFileIds(new Set(project.files.map(f => f.id)));
    }
  }, [selectedFileIds.size, project.files]);

  // Compute encoding result
  const encodeResult = useMemo(() => {
    if (!open || selectedFileIds.size === 0) {
      return {
        encoded: '',
        originalSize: 0,
        compressedSize: 0,
        status: 'error' as const,
        message: 'No files selected to share.',
      };
    }
    return encodeProject(project, {
      fileIds: Array.from(selectedFileIds),
      includeSchema,
    });
  }, [open, project, selectedFileIds, includeSchema]);

  const shareUrl = encodeResult.status !== 'error' ? buildShareUrl(encodeResult.encoded) : '';

  const handleCopy = useCallback(async () => {
    if (!shareUrl) return;

    try {
      await navigator.clipboard.writeText(shareUrl);
      setCopied(true);
      toast.success('Link copied to clipboard');
      setTimeout(() => setCopied(false), 2000);
    } catch {
      toast.error('Failed to copy link');
    }
  }, [shareUrl]);

  const dialectDisplay = project.dialect === 'generic' ? 'Generic SQL' : project.dialect;
  const allSelected = selectedFileIds.size === project.files.length;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Share Project</DialogTitle>
          <DialogDescription>
            Select which files to include. No data is uploaded.
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-4">
          {/* File Selection */}
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <Label className="text-sm font-medium">Files to share</Label>
              <Button
                variant="ghost"
                size="sm"
                className="h-6 text-xs px-2"
                onClick={toggleAll}
              >
                {allSelected ? 'Deselect all' : 'Select all'}
              </Button>
            </div>
            <ScrollArea className="h-[140px] rounded-md border p-2">
              <div className="space-y-1">
                {project.files.map(file => (
                  <label
                    key={file.id}
                    className="flex items-center gap-2 py-1 px-1 rounded hover:bg-muted cursor-pointer"
                  >
                    <Checkbox
                      checked={selectedFileIds.has(file.id)}
                      onCheckedChange={() => toggleFile(file.id)}
                    />
                    <FileText className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
                    <span className="text-sm truncate">{file.name}</span>
                  </label>
                ))}
              </div>
            </ScrollArea>
          </div>

          {/* Include Schema Checkbox */}
          {project.schemaSQL && (
            <label className="flex items-center gap-2 cursor-pointer">
              <Checkbox
                checked={includeSchema}
                onCheckedChange={(checked) => setIncludeSchema(checked === true)}
              />
              <span className="text-sm">Include schema definition</span>
            </label>
          )}

          {/* Result Section */}
          {encodeResult.status === 'error' ? (
            <div className="flex flex-col items-center gap-2 py-3 text-center rounded-md border border-dashed">
              <AlertTriangle className="h-8 w-8 text-muted-foreground" />
              <p className="text-sm text-muted-foreground">{encodeResult.message}</p>
            </div>
          ) : (
            <>
              <div className="flex items-center gap-2">
                <Input
                  readOnly
                  value={shareUrl}
                  className="flex-1 text-xs font-mono"
                  onClick={e => (e.target as HTMLInputElement).select()}
                />
                <Button
                  type="button"
                  size="sm"
                  onClick={handleCopy}
                  className="shrink-0"
                >
                  {copied ? (
                    <Check className="h-4 w-4" />
                  ) : (
                    <Copy className="h-4 w-4" />
                  )}
                </Button>
              </div>

              <div className="text-xs text-muted-foreground">
                {selectedFileIds.size} file{selectedFileIds.size !== 1 ? 's' : ''} · {dialectDisplay} · {formatBytes(encodeResult.compressedSize)}
              </div>

              {encodeResult.status === 'warning' && (
                <div className="flex items-start gap-2 rounded-md border border-amber-500/50 bg-amber-500/10 p-3">
                  <AlertTriangle className="h-4 w-4 text-amber-600 dark:text-amber-400 shrink-0 mt-0.5" />
                  <p className="text-xs text-amber-700 dark:text-amber-400">{encodeResult.message}</p>
                </div>
              )}
            </>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
