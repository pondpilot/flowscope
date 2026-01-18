/**
 * Export utilities for lineage data.
 * Delegates export generation to flowscope-export via WASM bindings.
 */

import type { AnalyzeResult, ExportFormat, MermaidView } from '@pondpilot/flowscope-core';
import {
  exportCsvArchive,
  exportFilename,
  exportHtml,
  exportJson,
  exportMermaid,
  exportXlsx,
} from '@pondpilot/flowscope-core';

export type MermaidGraphType = MermaidView;

function downloadBlob(content: BlobPart, filename: string, mimeType: string): void {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.download = filename;
  link.href = url;
  link.click();
  URL.revokeObjectURL(url);
}

function resolveExportedAt(exportedAt?: Date): Date {
  return exportedAt ?? new Date();
}

async function resolveFilename(
  format: ExportFormat,
  options: { projectName?: string; exportedAt?: Date; view?: MermaidView; compact?: boolean } = {}
): Promise<string> {
  return exportFilename({
    format,
    projectName: options.projectName,
    exportedAt: resolveExportedAt(options.exportedAt),
    view: options.view,
    compact: options.compact,
  });
}

export async function generateMermaid(
  result: AnalyzeResult,
  graphType: MermaidGraphType = 'table'
): Promise<string> {
  return exportMermaid(result, graphType);
}

export async function downloadMermaid(
  result: AnalyzeResult,
  options: { projectName?: string; exportedAt?: Date; view?: MermaidView } = {}
): Promise<void> {
  const view = options.view ?? 'all';
  const exportedAt = resolveExportedAt(options.exportedAt);
  const content = await exportMermaid(result, view);
  const filename = await resolveFilename('mermaid', { ...options, view, exportedAt });
  downloadBlob(content, filename, 'text/markdown');
}

export async function downloadJson(
  result: AnalyzeResult,
  options: { projectName?: string; exportedAt?: Date; compact?: boolean } = {}
): Promise<void> {
  const exportedAt = resolveExportedAt(options.exportedAt);
  const content = await exportJson(result, { compact: options.compact });
  const filename = await resolveFilename('json', { ...options, exportedAt });
  downloadBlob(content, filename, 'application/json');
}

export async function downloadCsvArchive(
  result: AnalyzeResult,
  options: { projectName?: string; exportedAt?: Date } = {}
): Promise<void> {
  const exportedAt = resolveExportedAt(options.exportedAt);
  const bytes = await exportCsvArchive(result);
  const filename = await resolveFilename('csv', { ...options, exportedAt });
  downloadBlob(bytes, filename, 'application/zip');
}

export async function downloadXlsx(
  result: AnalyzeResult,
  options: { projectName?: string; exportedAt?: Date } = {}
): Promise<void> {
  const exportedAt = resolveExportedAt(options.exportedAt);
  const bytes = await exportXlsx(result);
  const filename = await resolveFilename('xlsx', { ...options, exportedAt });
  downloadBlob(bytes, filename, 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet');
}

export async function downloadHtml(
  result: AnalyzeResult,
  options: { projectName?: string; exportedAt?: Date } = {}
): Promise<void> {
  const exportedAt = resolveExportedAt(options.exportedAt);
  const content = await exportHtml(result, {
    projectName: options.projectName,
    exportedAt,
  });
  const filename = await resolveFilename('html', { ...options, exportedAt });
  downloadBlob(content, filename, 'text/html');
}

export async function downloadPng(
  dataUrl: string,
  options: { projectName?: string; exportedAt?: Date } = {}
): Promise<void> {
  const exportedAt = resolveExportedAt(options.exportedAt);
  const filename = await resolveFilename('png', { ...options, exportedAt });
  const link = document.createElement('a');
  link.download = filename;
  link.href = dataUrl;
  link.click();
}
