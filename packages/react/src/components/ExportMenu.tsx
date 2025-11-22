import { useCallback } from 'react';
import { toPng } from 'html-to-image';
import { useLineage } from '../context';
import { Button } from './ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './ui/dropdown-menu';

export interface ExportMenuProps {
  graphRef: React.RefObject<HTMLDivElement>;
}

export function ExportMenu({ graphRef }: ExportMenuProps): JSX.Element {
  const { state } = useLineage();
  const { result } = state;

  const downloadImage = useCallback(async (format: 'png' | 'svg') => {
    if (graphRef.current === null) {
      return;
    }

    // Filter out the controls and minimap for cleaner export if desired
    // But html-to-image captures what's visible.
    // ReactFlow has a specific class 'react-flow__viewport' that contains the graph.
    // However, graphRef usually points to the wrapper div.
    
    // To capture just the viewport (zoom independent), it's trickier with html-to-image
    // on the container because it captures the current view.
    // For a quick win, capturing the current view is usually what users expect ("screenshot").
    
    try {
      const dataUrl = await toPng(graphRef.current, { backgroundColor: '#fff' });
      const link = document.createElement('a');
      link.download = `flowscope-lineage.${format}`;
      link.href = dataUrl;
      link.click();
    } catch (err) {
      console.error('Failed to export image:', err);
    }
  }, [graphRef]);

  const downloadJson = useCallback(() => {
    if (!result) return;
    const jsonString = JSON.stringify(result, null, 2);
    const blob = new Blob([jsonString], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.download = 'flowscope-lineage.json';
    link.href = url;
    link.click();
    URL.revokeObjectURL(url);
  }, [result]);

  const downloadCsv = useCallback(() => {
    if (!result) return;
    // Simple CSV export: Nodes list and Edges list
    // We can create a zip or just export nodes for now.
    // Let's export a single CSV for nodes and one for edges? Or just nodes.
    // Request said "export as json, csv".
    // Let's do nodes.csv and edges.csv? Or just one combined?
    // Usually people want lists.
    
    // Let's create a simple CSV string for Nodes
    const nodesHeader = 'id,type,label,schema,table\n';
    const nodesRows = result.statements.flatMap(stmt => 
      stmt.nodes.map(n => {
        const type = n.node_type;
        const label = n.label;
        const qualified = n.qualified_name || '';
        return `${n.id},${type},${label},,${qualified}`;
      })
    ).join('\n');
    
    const nodesCsv = nodesHeader + nodesRows;
    
    const blob = new Blob([nodesCsv], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.download = 'flowscope-nodes.csv';
    link.href = url;
    link.click();
    URL.revokeObjectURL(url);

  }, [result]);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" size="sm" className="shadow-sm bg-white">
          Export
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onClick={() => downloadImage('png')}>
          Export as PNG
        </DropdownMenuItem>
        <DropdownMenuItem onClick={downloadJson}>
          Export as JSON
        </DropdownMenuItem>
        <DropdownMenuItem onClick={downloadCsv}>
          Export Nodes as CSV
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
