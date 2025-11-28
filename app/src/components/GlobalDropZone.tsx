import { useState, useCallback, useEffect, useRef } from 'react';
import { Upload, FolderUp, AlertCircle } from 'lucide-react';
import { cn } from '@/lib/utils';
import { useProject } from '@/lib/project-store';
import { ACCEPTED_FILE_TYPES, FILE_LIMITS } from '@/lib/constants';

interface FileSystemEntry {
  isFile: boolean;
  isDirectory: boolean;
  name: string;
  file?: (callback: (file: File) => void) => void;
  createReader?: () => FileSystemDirectoryReader;
}

interface FileSystemDirectoryReader {
  readEntries: (
    successCallback: (entries: FileSystemEntry[]) => void,
    errorCallback?: (error: Error) => void
  ) => void;
}

type DataTransferItemWithEntry = DataTransferItem & {
  webkitGetAsEntry?: () => FileSystemEntry | null;
};

async function readFileEntry(entry: FileSystemEntry): Promise<File | null> {
  return new Promise((resolve) => {
    if (entry.file) {
      entry.file((file) => resolve(file));
    } else {
      resolve(null);
    }
  });
}

async function readDirectoryEntries(
  reader: FileSystemDirectoryReader
): Promise<FileSystemEntry[]> {
  return new Promise((resolve, reject) => {
    reader.readEntries(
      (entries) => resolve(entries),
      (error) => reject(error)
    );
  });
}

interface RejectedFile {
  name: string;
  reason: 'size' | 'type' | 'count';
}

interface ProcessResult {
  accepted: File[];
  rejected: RejectedFile[];
}

async function processEntry(
  entry: FileSystemEntry,
  basePath: string = ''
): Promise<ProcessResult> {
  const accepted: File[] = [];
  const rejected: RejectedFile[] = [];
  const currentPath = basePath ? `${basePath}/${entry.name}` : entry.name;

  if (entry.isFile) {
    const file = await readFileEntry(entry);
    if (file) {
      const extension = '.' + file.name.split('.').pop()?.toLowerCase();
      if (!ACCEPTED_FILE_TYPES.includes(extension)) {
        rejected.push({ name: file.name, reason: 'type' });
      } else if (file.size > FILE_LIMITS.MAX_SIZE) {
        rejected.push({ name: file.name, reason: 'size' });
      } else {
        const fileWithPath = new File([file], file.name, { type: file.type });
        Object.defineProperty(fileWithPath, 'webkitRelativePath', {
          value: currentPath,
          writable: false,
        });
        accepted.push(fileWithPath);
      }
    }
  } else if (entry.isDirectory && entry.createReader) {
    const reader = entry.createReader();
    let entries: FileSystemEntry[] = [];

    let batch: FileSystemEntry[];
    do {
      batch = await readDirectoryEntries(reader);
      entries = entries.concat(batch);
    } while (batch.length > 0);

    for (const childEntry of entries) {
      const result = await processEntry(childEntry, currentPath);
      accepted.push(...result.accepted);
      rejected.push(...result.rejected);
    }
  }

  return { accepted, rejected };
}

export function GlobalDropZone() {
  const { importFiles, currentProject } = useProject();
  const [isDragOver, setIsDragOver] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [rejectedFiles, setRejectedFiles] = useState<RejectedFile[]>([]);
  const dragCounter = useRef(0);

  const handleDragEnter = useCallback((e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();

    dragCounter.current++;

    // Check if dragging files
    if (e.dataTransfer?.types.includes('Files')) {
      setIsDragOver(true);
    }
  }, []);

  const handleDragLeave = useCallback((e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();

    dragCounter.current--;
    if (dragCounter.current === 0) {
      setIsDragOver(false);
    }
  }, []);

  const handleDragOver = useCallback((e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
  }, []);

  const handleDrop = useCallback(
    async (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setIsDragOver(false);
      dragCounter.current = 0;
      setRejectedFiles([]);

      if (!currentProject || isProcessing) return;

      const items = e.dataTransfer?.items;
      if (!items || items.length === 0) return;

      setIsProcessing(true);

      try {
        const allAccepted: File[] = [];
        const allRejected: RejectedFile[] = [];

        const hasWebkitEntry = items[0] && 'webkitGetAsEntry' in items[0];

        if (hasWebkitEntry) {
          for (let i = 0; i < items.length; i++) {
            const item = items[i] as DataTransferItemWithEntry;
            const entry = item.webkitGetAsEntry?.();
            if (entry) {
              const result = await processEntry(entry);
              allAccepted.push(...result.accepted);
              allRejected.push(...result.rejected);
            }
          }
        } else {
          const files = e.dataTransfer?.files;
          if (files) {
            for (let i = 0; i < files.length; i++) {
              const file = files[i];
              const extension = '.' + file.name.split('.').pop()?.toLowerCase();
              if (!ACCEPTED_FILE_TYPES.includes(extension)) {
                allRejected.push({ name: file.name, reason: 'type' });
              } else if (file.size > FILE_LIMITS.MAX_SIZE) {
                allRejected.push({ name: file.name, reason: 'size' });
              } else {
                allAccepted.push(file);
              }
            }
          }
        }

        // Enforce file count limit
        const filesToImport = allAccepted.slice(0, FILE_LIMITS.MAX_COUNT);
        const excessFiles = allAccepted.slice(FILE_LIMITS.MAX_COUNT);
        for (const file of excessFiles) {
          allRejected.push({ name: file.name, reason: 'count' });
        }

        if (filesToImport.length > 0) {
          await importFiles(filesToImport);
        }

        if (allRejected.length > 0) {
          setRejectedFiles(allRejected);
        }
      } finally {
        setIsProcessing(false);
      }
    },
    [currentProject, isProcessing, importFiles]
  );

  useEffect(() => {
    window.addEventListener('dragenter', handleDragEnter);
    window.addEventListener('dragleave', handleDragLeave);
    window.addEventListener('dragover', handleDragOver);
    window.addEventListener('drop', handleDrop);

    return () => {
      window.removeEventListener('dragenter', handleDragEnter);
      window.removeEventListener('dragleave', handleDragLeave);
      window.removeEventListener('dragover', handleDragOver);
      window.removeEventListener('drop', handleDrop);
    };
  }, [handleDragEnter, handleDragLeave, handleDragOver, handleDrop]);

  const dismissRejectedFiles = useCallback(() => {
    setRejectedFiles([]);
  }, []);

  const hasRejectedFiles = rejectedFiles.length > 0;

  if (!isDragOver && !isProcessing && !hasRejectedFiles) {
    return null;
  }

  const getReasonText = (reason: RejectedFile['reason']) => {
    switch (reason) {
      case 'size':
        return 'exceeds 10MB limit';
      case 'type':
        return 'unsupported file type';
      case 'count':
        return 'exceeds 100 file limit';
    }
  };

  // Show rejected files notification
  if (hasRejectedFiles && !isDragOver && !isProcessing) {
    return (
      <div
        role="alert"
        aria-live="polite"
        className="fixed bottom-4 right-4 z-50 max-w-md bg-background border border-destructive/50 rounded-lg shadow-lg p-4"
      >
        <div className="flex items-start gap-3">
          <AlertCircle className="size-5 text-destructive shrink-0 mt-0.5" />
          <div className="flex-1 min-w-0">
            <p className="font-medium text-destructive">
              {rejectedFiles.length} file{rejectedFiles.length > 1 ? 's' : ''} could not be imported
            </p>
            <ul className="mt-2 text-sm text-muted-foreground space-y-1 max-h-32 overflow-y-auto">
              {rejectedFiles.slice(0, 5).map((file, i) => (
                <li key={i} className="truncate">
                  <span className="font-medium">{file.name}</span>
                  <span className="text-muted-foreground/70"> — {getReasonText(file.reason)}</span>
                </li>
              ))}
              {rejectedFiles.length > 5 && (
                <li className="text-muted-foreground/70">
                  ...and {rejectedFiles.length - 5} more
                </li>
              )}
            </ul>
          </div>
          <button
            onClick={dismissRejectedFiles}
            className="text-muted-foreground hover:text-foreground"
            aria-label="Dismiss"
          >
            ×
          </button>
        </div>
      </div>
    );
  }

  return (
    <div
      role="region"
      aria-label="File drop zone"
      className={cn(
        'fixed inset-0 z-50 flex items-center justify-center',
        'bg-background/80 backdrop-blur-sm',
        'transition-opacity duration-200',
        isDragOver ? 'opacity-100' : 'opacity-0 pointer-events-none'
      )}
    >
      <div
        className={cn(
          'flex flex-col items-center justify-center gap-4 p-12 rounded-xl',
          'border-2 border-dashed',
          'bg-background shadow-2xl',
          isDragOver ? 'border-primary' : 'border-muted-foreground/25'
        )}
      >
        {isProcessing ? (
          <>
            <div
              className="size-12 border-4 border-primary border-t-transparent rounded-full animate-spin"
              role="status"
              aria-label="Processing files"
            />
            <p className="text-lg font-medium">Processing files...</p>
          </>
        ) : (
          <>
            <div className="flex gap-4">
              <Upload className="size-12 text-primary" aria-hidden="true" />
              <FolderUp className="size-12 text-primary" aria-hidden="true" />
            </div>
            <div className="text-center">
              <p className="text-xl font-medium">Drop files or folders here</p>
              <p className="text-sm text-muted-foreground mt-1">
                Supports .sql, .json, and .txt files (max 10MB each, 100 files)
              </p>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
