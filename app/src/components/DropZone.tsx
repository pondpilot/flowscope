import { useState, useCallback, useRef } from 'react';
import { Upload, FolderUp } from 'lucide-react';
import { cn } from '@/lib/utils';
import { ACCEPTED_FILE_TYPES } from '@/lib/constants';

interface DropZoneProps {
  onFilesDropped: (files: File[]) => void;
  className?: string;
  disabled?: boolean;
}

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

async function processEntry(
  entry: FileSystemEntry,
  basePath: string = ''
): Promise<File[]> {
  const files: File[] = [];
  const currentPath = basePath ? `${basePath}/${entry.name}` : entry.name;

  if (entry.isFile) {
    const file = await readFileEntry(entry);
    if (file) {
      // Check if file type is accepted
      const extension = '.' + file.name.split('.').pop()?.toLowerCase();
      if (ACCEPTED_FILE_TYPES.includes(extension)) {
        // Create a new file with the path stored
        const fileWithPath = new File([file], file.name, { type: file.type });
        Object.defineProperty(fileWithPath, 'webkitRelativePath', {
          value: currentPath,
          writable: false,
        });
        files.push(fileWithPath);
      }
    }
  } else if (entry.isDirectory && entry.createReader) {
    const reader = entry.createReader();
    let entries: FileSystemEntry[] = [];

    // readEntries may not return all entries at once, so we need to call it repeatedly
    let batch: FileSystemEntry[];
    do {
      batch = await readDirectoryEntries(reader);
      entries = entries.concat(batch);
    } while (batch.length > 0);

    for (const childEntry of entries) {
      const childFiles = await processEntry(childEntry, currentPath);
      files.push(...childFiles);
    }
  }

  return files;
}

export function DropZone({ onFilesDropped, className, disabled }: DropZoneProps) {
  const [isDragOver, setIsDragOver] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const dragCounter = useRef(0);

  const handleDragEnter = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (disabled) return;

      dragCounter.current++;
      if (e.dataTransfer.items && e.dataTransfer.items.length > 0) {
        setIsDragOver(true);
      }
    },
    [disabled]
  );

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();

    dragCounter.current--;
    if (dragCounter.current === 0) {
      setIsDragOver(false);
    }
  }, []);

  const handleDragOver = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (disabled) return;
    },
    [disabled]
  );

  const handleDrop = useCallback(
    async (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setIsDragOver(false);
      dragCounter.current = 0;

      if (disabled || isProcessing) return;

      const items = e.dataTransfer.items;
      if (!items || items.length === 0) return;

      setIsProcessing(true);

      try {
        const allFiles: File[] = [];

        // Try to use webkitGetAsEntry for folder support
        const hasWebkitEntry = items[0] && 'webkitGetAsEntry' in items[0];

        if (hasWebkitEntry) {
          for (let i = 0; i < items.length; i++) {
            const item = items[i] as DataTransferItemWithEntry;
            const entry = item.webkitGetAsEntry?.();
            if (entry) {
              const files = await processEntry(entry);
              allFiles.push(...files);
            }
          }
        } else {
          // Fallback for browsers without webkitGetAsEntry
          const files = e.dataTransfer.files;
          for (let i = 0; i < files.length; i++) {
            const file = files[i];
            const extension = '.' + file.name.split('.').pop()?.toLowerCase();
            if (ACCEPTED_FILE_TYPES.includes(extension)) {
              allFiles.push(file);
            }
          }
        }

        if (allFiles.length > 0) {
          onFilesDropped(allFiles);
        }
      } finally {
        setIsProcessing(false);
      }
    },
    [disabled, isProcessing, onFilesDropped]
  );

  return (
    <div
      className={cn(
        'relative border-2 border-dashed rounded-lg p-6 transition-colors',
        isDragOver && !disabled
          ? 'border-primary bg-primary/5'
          : 'border-muted-foreground/25 hover:border-muted-foreground/50',
        disabled && 'opacity-50 cursor-not-allowed',
        isProcessing && 'opacity-75',
        className
      )}
      onDragEnter={handleDragEnter}
      onDragLeave={handleDragLeave}
      onDragOver={handleDragOver}
      onDrop={handleDrop}
    >
      <div className="flex flex-col items-center justify-center gap-2 text-center">
        {isProcessing ? (
          <>
            <div className="size-8 border-2 border-primary border-t-transparent rounded-full animate-spin" />
            <p className="text-sm text-muted-foreground">Processing files...</p>
          </>
        ) : (
          <>
            <div className="flex gap-2">
              <Upload className={cn('size-6', isDragOver ? 'text-primary' : 'text-muted-foreground')} />
              <FolderUp className={cn('size-6', isDragOver ? 'text-primary' : 'text-muted-foreground')} />
            </div>
            <div>
              <p className={cn('text-sm font-medium', isDragOver && 'text-primary')}>
                {isDragOver ? 'Drop files or folders here' : 'Drag and drop files or folders'}
              </p>
              <p className="text-xs text-muted-foreground mt-1">
                Supports .sql, .json, and .txt files
              </p>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
