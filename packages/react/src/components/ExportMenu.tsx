import { useCallback } from 'react';
import { toPng } from 'html-to-image';
import {
  Download,
  Image,
  FileJson,
  FileSpreadsheet,
  FileCode,
  FileText,
} from 'lucide-react';
import { useLineage } from '../store';
import { UI_CONSTANTS } from '../constants';
import {
  downloadXlsx,
  downloadJson,
  downloadMermaid,
  downloadHtml,
} from '../utils/exportUtils';
import {
  GraphTooltip,
  GraphTooltipContent,
  GraphTooltipProvider,
  GraphTooltipTrigger,
  GraphTooltipArrow,
  GraphTooltipPortal,
} from './ui/graph-tooltip';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  DropdownMenuSeparator,
  DropdownMenuLabel,
} from './ui/dropdown-menu';

export interface ExportMenuProps {
  graphRef: React.RefObject<HTMLDivElement>;
}

export function ExportMenu({ graphRef }: ExportMenuProps): JSX.Element {
  const { state } = useLineage();
  const { result } = state;

  const handleDownloadPng = useCallback(async () => {
    if (graphRef.current === null) {
      return;
    }

    try {
      const dataUrl = await toPng(graphRef.current, { backgroundColor: '#fff' });
      const link = document.createElement('a');
      link.download = 'lineage-export.png';
      link.href = dataUrl;
      link.click();
    } catch (err) {
      console.error('Failed to export image:', err);
    }
  }, [graphRef]);

  const handleDownloadXlsx = useCallback(() => {
    if (!result) return;
    try {
      downloadXlsx(result);
    } catch (err) {
      console.error('Failed to export Excel:', err);
    }
  }, [result]);

  const handleDownloadJson = useCallback(() => {
    if (!result) return;
    try {
      downloadJson(result);
    } catch (err) {
      console.error('Failed to export JSON:', err);
    }
  }, [result]);

  const handleDownloadMermaid = useCallback(() => {
    if (!result) return;
    try {
      downloadMermaid(result);
    } catch (err) {
      console.error('Failed to export Mermaid:', err);
    }
  }, [result]);

  const handleDownloadHtml = useCallback(() => {
    if (!result) return;
    try {
      downloadHtml(result);
    } catch (err) {
      console.error('Failed to export HTML:', err);
    }
  }, [result]);

  return (
    <GraphTooltipProvider>
      <GraphTooltip delayDuration={UI_CONSTANTS.TOOLTIP_DELAY}>
        <DropdownMenu>
          <GraphTooltipTrigger asChild>
            <DropdownMenuTrigger asChild>
              <button
                className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-slate-500 transition-all duration-200 hover:bg-slate-100 dark:hover:bg-slate-700 hover:text-slate-900 dark:hover:text-slate-100 focus-visible:outline-none"
                aria-label="Export lineage"
              >
                <Download className="size-4" strokeWidth={1.5} />
              </button>
            </DropdownMenuTrigger>
          </GraphTooltipTrigger>
          <GraphTooltipPortal>
            <GraphTooltipContent side="bottom">
              <p>Export lineage</p>
              <GraphTooltipArrow />
            </GraphTooltipContent>
          </GraphTooltipPortal>
          <DropdownMenuContent align="end" className="w-48">
            <DropdownMenuLabel>Data Formats</DropdownMenuLabel>
            <DropdownMenuItem onClick={handleDownloadXlsx}>
              <FileSpreadsheet className="size-4 mr-2" />
              Excel (.xlsx)
            </DropdownMenuItem>
            <DropdownMenuItem onClick={handleDownloadJson}>
              <FileJson className="size-4 mr-2" />
              JSON
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuLabel>Visual Formats</DropdownMenuLabel>
            <DropdownMenuItem onClick={handleDownloadPng}>
              <Image className="size-4 mr-2" />
              PNG Image
            </DropdownMenuItem>
            <DropdownMenuItem onClick={handleDownloadMermaid}>
              <FileCode className="size-4 mr-2" />
              Mermaid (.md)
            </DropdownMenuItem>
            <DropdownMenuItem onClick={handleDownloadHtml}>
              <FileText className="size-4 mr-2" />
              HTML Report
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </GraphTooltip>
    </GraphTooltipProvider>
  );
}
