import { toPng, toSvg } from 'html-to-image';
import { saveAs } from 'file-saver';
import { Download } from 'lucide-react';

import type { RefObject } from 'react';

import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { useLineageState } from '@pondpilot/flowscope-react';

interface ExportMenuProps {
  graphContainerRef: RefObject<HTMLDivElement>;
}

export function ExportMenu({ graphContainerRef }: ExportMenuProps): JSX.Element {
  const { result } = useLineageState();

  const exportImage = async (format: 'png' | 'svg') => {
    if (!graphContainerRef.current) return;

    const fileName = `flowscope-lineage-${Date.now()}.${format}`;
    let dataUrl: string | undefined;

    if (format === 'png') {
      dataUrl = await toPng(graphContainerRef.current);
    } else if (format === 'svg') {
      dataUrl = await toSvg(graphContainerRef.current);
    }

    if (dataUrl) {
      saveAs(dataUrl, fileName);
    }
  };

  const exportCsv = () => {
    if (!result || !result.statements.length) return;

    const csvRows: string[] = [];
    csvRows.push('Statement Index,Node ID,Node Label,Node Type,Column Name,Column Expression,Upstream Nodes,Downstream Nodes');

    result.statements.forEach((statement, stmtIndex) => {
      statement.nodes.forEach((node) => {
        const upstreamNodes = statement.edges
          .filter((edge) => edge.to === node.id && (edge.type === 'data_flow' || edge.type === 'derivation'))
          .map((edge) => statement.nodes.find((n) => n.id === edge.from)?.label || edge.from)
          .join('; ');

        const downstreamNodes = statement.edges
          .filter((edge) => edge.from === node.id && (edge.type === 'data_flow' || edge.type === 'derivation'))
          .map((edge) => statement.nodes.find((n) => n.id === edge.to)?.label || edge.to)
          .join('; ');

        if (node.type === 'table' || node.type === 'cte') {
          const columns = statement.edges
            .filter((edge) => edge.from === node.id && edge.type === 'ownership')
            .map((edge) => statement.nodes.find((n) => n.id === edge.to));

          if (columns.length > 0) {
            columns.forEach((col) => {
              if (col) {
                csvRows.push(
                  `"${stmtIndex}","${node.id}","${node.label}","${node.type}","${col.label}","${col.expression || ''}","${upstreamNodes}","${downstreamNodes}"`
                );
              }
            });
          } else {
            csvRows.push(
              `"${stmtIndex}","${node.id}","${node.label}","${node.type}","","","${upstreamNodes}","${downstreamNodes}"`
            );
          }
        } else if (node.type === 'column') {
          csvRows.push(
            `"${stmtIndex}","${node.id}","${node.label}","${node.type}","${node.label}","${node.expression || ''}","${upstreamNodes}","${downstreamNodes}"`
          );
        }
      });
    });

    const csvContent = csvRows.join('\n');
    const blob = new Blob([csvContent], { type: 'text/csv;charset=utf-8;' });
    saveAs(blob, `flowscope-lineage-mapping-${Date.now()}.csv`);
  };

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" size="sm" className="gap-1">
          <Download className="h-4 w-4" />
          Export
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onClick={() => exportImage('png')}>Export as PNG</DropdownMenuItem>
        <DropdownMenuItem onClick={() => exportImage('svg')}>Export as SVG</DropdownMenuItem>
        <DropdownMenuItem onClick={exportCsv}>Export as CSV</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
