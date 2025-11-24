import { useState, useCallback } from 'react';
import { SqlView } from '@pondpilot/flowscope-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';
import { Button } from './ui/button';
import type { Dialect } from '@/lib/project-store';

interface SchemaEditorProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  schemaSQL: string;
  dialect: Dialect;
  onSave: (schemaSQL: string) => void;
}

export function SchemaEditor({
  open,
  onOpenChange,
  schemaSQL,
  onSave,
}: SchemaEditorProps) {
  const [editedSQL, setEditedSQL] = useState(schemaSQL);

  // Reset to prop value when dialog opens
  const handleOpenChange = useCallback((newOpen: boolean) => {
    if (newOpen) {
      setEditedSQL(schemaSQL);
    }
    onOpenChange(newOpen);
  }, [schemaSQL, onOpenChange]);

  const handleSave = useCallback(() => {
    onSave(editedSQL);
    onOpenChange(false);
  }, [editedSQL, onSave, onOpenChange]);

  const handleCancel = useCallback(() => {
    setEditedSQL(schemaSQL); // Reset to original
    onOpenChange(false);
  }, [schemaSQL, onOpenChange]);

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="max-w-4xl max-h-[80vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>Edit Schema</DialogTitle>
          <DialogDescription>
            Define your database schema using CREATE TABLE statements. This schema will be
            used to augment the lineage analysis without appearing in the graph.
          </DialogDescription>
        </DialogHeader>

        <div className="flex-1 min-h-0 border rounded-md overflow-hidden">
          <SqlView
            value={editedSQL}
            onChange={setEditedSQL}
            className="h-full"
            editable={true}
          />
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={handleCancel}>
            Cancel
          </Button>
          <Button onClick={handleSave}>
            Save Schema
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
