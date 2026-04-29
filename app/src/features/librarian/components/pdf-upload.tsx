import { useCallback, useEffect, useRef, useState } from 'react';
import { FileText, Loader2, Trash2, Upload, AlertCircle } from 'lucide-react';

import { Button } from '@/components/ui/button';

import { MAX_PDF_SIZE_BYTES, MAX_PDF_SIZE_MB } from '../constants';
import { useLibrarianPdfFiles, useLibrarianStore } from '../store';
import type { PdfFile } from '../types';

interface PdfUploadProps {
  onUpload: (file: File) => void;
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function FileStatusIcon({ status }: { status: PdfFile['status'] }) {
  if (status === 'processing') {
    return <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />;
  }
  if (status === 'error') {
    return <AlertCircle className="h-4 w-4 text-red-500" />;
  }
  return <FileText className="h-4 w-4 text-muted-foreground" />;
}

export function PdfUpload({ onUpload }: PdfUploadProps) {
  const [dragging, setDragging] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const dropRef = useRef<HTMLDivElement>(null);

  const pdfFiles = useLibrarianPdfFiles();
  const removePdf = useLibrarianStore((s) => s.removePdf);
  const hasPdfFile = useLibrarianStore((s) => s.hasPdfFile);

  const validateAndUpload = useCallback(
    (file: File) => {
      setError(null);

      if (!file.name.toLowerCase().endsWith('.pdf')) {
        setError('Only PDF files are supported.');
        return;
      }

      if (file.size > MAX_PDF_SIZE_BYTES) {
        setError(`File exceeds ${MAX_PDF_SIZE_MB} MB limit.`);
        return;
      }

      if (hasPdfFile(file.name)) {
        setError('A file with this name is already uploaded.');
        return;
      }

      onUpload(file);
    },
    [hasPdfFile, onUpload]
  );

  // Use native event listeners so stopPropagation prevents the event
  // from reaching GlobalDropZone's window-level native listeners.
  // React synthetic stopPropagation only stops React's own bubble chain.
  useEffect(() => {
    const el = dropRef.current;
    if (!el) return;

    const handleDrop = (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setDragging(false);
      const file = e.dataTransfer?.files[0];
      if (file) validateAndUpload(file);
    };

    const handleDragOver = (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setDragging(true);
    };

    const handleDragLeave = (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      setDragging(false);
    };

    const handleDragEnter = (e: DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
    };

    el.addEventListener('drop', handleDrop);
    el.addEventListener('dragover', handleDragOver);
    el.addEventListener('dragleave', handleDragLeave);
    el.addEventListener('dragenter', handleDragEnter);

    return () => {
      el.removeEventListener('drop', handleDrop);
      el.removeEventListener('dragover', handleDragOver);
      el.removeEventListener('dragleave', handleDragLeave);
      el.removeEventListener('dragenter', handleDragEnter);
    };
  }, [validateAndUpload]);

  const handleFileInput = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (file) validateAndUpload(file);
      if (inputRef.current) inputRef.current.value = '';
    },
    [validateAndUpload]
  );

  return (
    <div className="flex min-w-0 flex-col gap-2 overflow-hidden">
      <div
        ref={dropRef}
        className={`flex cursor-pointer flex-col items-center gap-2 rounded-lg border border-dashed p-4 transition-colors ${
          dragging
            ? 'border-accent-light bg-accent/5 dark:border-accent-dark'
            : 'border-border-primary-light dark:border-border-primary-dark hover:border-accent-light dark:hover:border-accent-dark'
        }`}
        onClick={() => inputRef.current?.click()}
        data-testid="drop-zone"
        data-librarian-dropzone
        role="button"
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            inputRef.current?.click();
          }
        }}
      >
        <Upload className="h-6 w-6 text-muted-foreground/50" />
        <p className="text-xs text-muted-foreground">Drop a PDF here or click to upload</p>
        <p className="text-xs text-muted-foreground/60">Max {MAX_PDF_SIZE_MB} MB per file</p>
      </div>

      <input
        ref={inputRef}
        type="file"
        accept=".pdf,application/pdf"
        className="hidden"
        onChange={handleFileInput}
        data-testid="file-input"
      />

      {error && (
        <p className="text-xs text-red-600 dark:text-red-400" data-testid="upload-error">
          {error}
        </p>
      )}

      {pdfFiles.length > 0 && (
        <div className="max-h-[64px] overflow-y-auto">
          <div className="flex flex-col gap-1">
            {pdfFiles.map((file) => (
              <div
                key={file.id}
                className="flex min-w-0 items-center gap-1 rounded-md px-2 py-1 text-sm hover:bg-muted"
                data-testid="pdf-file-item"
              >
                <FileStatusIcon status={file.status} />
                <span className="min-w-0 flex-1 truncate text-xs">{file.name}</span>
                <span className="shrink-0 whitespace-nowrap text-xs text-muted-foreground">
                  {formatFileSize(file.size)}
                </span>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-6 w-6 shrink-0"
                  onClick={(e) => {
                    e.stopPropagation();
                    removePdf(file.id);
                  }}
                  aria-label={`Remove ${file.name}`}
                >
                  <Trash2 className="h-3 w-3" />
                </Button>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
