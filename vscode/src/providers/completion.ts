import * as vscode from 'vscode';
import { completionItems, isWasmInitialized } from '../analysis';
import { VALID_DIALECTS } from '../types';
import type { CompletionItem as FlowscopeCompletionItem, Dialect } from '../types';

/**
 * Provides SQL IntelliSense backed by the flowscope completion engine:
 * dialect keywords, built-in functions, operators, and any table / view /
 * CTE / column names recovered from the current parse.
 */
const IDENTIFIER_TRIGGER_CHARACTERS = [
  '.',
  '_',
  '$',
  ...'abcdefghijklmnopqrstuvwxyz',
  ...'ABCDEFGHIJKLMNOPQRSTUVWXYZ',
] as const;

export class FlowScopeCompletionProvider implements vscode.CompletionItemProvider {
  /**
   * Trigger on identifier characters so keyword/function/table suggestions
   * appear while the user types, not only after manual invocation or `.`.
   */
  public static readonly triggerCharacters: readonly string[] = IDENTIFIER_TRIGGER_CHARACTERS;

  public provideCompletionItems(
    document: vscode.TextDocument,
    position: vscode.Position,
    token: vscode.CancellationToken,
    _context: vscode.CompletionContext
  ): vscode.ProviderResult<vscode.CompletionList> {
    if (token.isCancellationRequested) {
      return null;
    }

    const config = vscode.workspace.getConfiguration('flowscope');
    if (!config.get<boolean>('enableCompletion', true)) {
      return null;
    }

    if (!isWasmInitialized()) {
      return null;
    }

    const sql = document.getText();
    const dialect = resolveDialect(config);
    // `document.offsetAt` yields a UTF-16 code-unit offset, which matches
    // `encoding: 'utf16'` on the engine request — no byte conversion needed.
    const cursorOffset = document.offsetAt(position);

    if (token.isCancellationRequested) {
      return null;
    }

    let result;
    try {
      result = completionItems({ sql, dialect, cursorOffset, encoding: 'utf16' });
    } catch (error) {
      console.warn(
        '[FlowScope] completion failed:',
        error instanceof Error ? error.message : String(error)
      );
      return null;
    }

    if (token.isCancellationRequested) {
      return null;
    }

    if (result.error) {
      console.warn('[FlowScope] completion failed:', result.error);
      return null;
    }

    if (!result.shouldShow || result.items.length === 0) {
      return null;
    }

    const replaceRange = resolveReplaceRange(document, result.token);

    return new vscode.CompletionList(
      result.items.map((item) => toVsCodeCompletionItem(item, replaceRange)),
      /* isIncomplete */ false
    );
  }
}

function resolveReplaceRange(
  document: vscode.TextDocument,
  token: { kind?: string; span: { start: number; end: number } } | undefined
): vscode.Range | undefined {
  if (!token || (token.kind !== 'identifier' && token.kind !== 'keyword')) {
    return undefined;
  }

  const docLength = document.getText().length;
  const clamp = (offset: number) => Math.max(0, Math.min(docLength, offset));
  const from = clamp(token.span.start);
  const to = Math.max(from, clamp(token.span.end));
  return new vscode.Range(document.positionAt(from), document.positionAt(to));
}

function resolveDialect(config: vscode.WorkspaceConfiguration): Dialect {
  const raw = config.get<string>('dialect', 'generic');
  if ((VALID_DIALECTS as readonly string[]).includes(raw)) {
    return raw as Dialect;
  }
  console.warn(`[FlowScope] unknown dialect "${raw}" in settings; falling back to "generic".`);
  return 'generic';
}

const VSCODE_KIND_BY_FLOWSCOPE_KIND: Record<
  FlowscopeCompletionItem['kind'],
  vscode.CompletionItemKind
> = {
  keyword: vscode.CompletionItemKind.Keyword,
  operator: vscode.CompletionItemKind.Operator,
  function: vscode.CompletionItemKind.Function,
  snippet: vscode.CompletionItemKind.Snippet,
  table: vscode.CompletionItemKind.Struct,
  schemaTable: vscode.CompletionItemKind.Module,
  column: vscode.CompletionItemKind.Field,
};

const FALLBACK_DETAIL: Record<FlowscopeCompletionItem['kind'], string> = {
  keyword: 'keyword',
  operator: 'operator',
  function: 'function',
  snippet: 'snippet',
  table: 'table',
  schemaTable: 'schema table',
  column: 'column',
};

/**
 * VS Code sorts completions by `sortText` ascending. The flowscope engine
 * returns higher scores for better matches, so we invert the score to keep
 * the order the user expects.
 */
const MAX_SORT_BUCKET = 9999;

function toVsCodeCompletionItem(
  item: FlowscopeCompletionItem,
  replaceRange: vscode.Range | undefined
): vscode.CompletionItem {
  const completion = new vscode.CompletionItem(
    item.label,
    VSCODE_KIND_BY_FLOWSCOPE_KIND[item.kind]
  );
  completion.detail = item.detail ?? FALLBACK_DETAIL[item.kind];
  if (item.insertText !== item.label) {
    completion.insertText = item.insertText;
  }
  if (replaceRange) {
    completion.range = replaceRange;
  }
  const bucket = Math.max(0, Math.min(MAX_SORT_BUCKET, Math.floor(MAX_SORT_BUCKET - item.score)));
  completion.sortText = bucket.toString().padStart(5, '0') + item.label.toLowerCase();
  return completion;
}
