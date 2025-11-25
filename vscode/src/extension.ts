import * as vscode from 'vscode';
import { initWasm, isWasmInitialized, getEngineVersion } from './analysis';
import { FlowScopeCodeLensProvider } from './providers/codeLens';
import { FlowScopeHoverProvider } from './providers/hover';
import { FlowScopeDiagnosticsProvider } from './providers/diagnostics';
import { LineagePanel } from './webview/lineagePanel';

let diagnosticsProvider: FlowScopeDiagnosticsProvider | undefined;
let codeLensProvider: FlowScopeCodeLensProvider | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  console.log('FlowScope extension activating...');

  // Initialize WASM
  try {
    await initWasm(context.extensionPath);
    console.log(`FlowScope WASM initialized. Engine version: ${getEngineVersion()}`);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    vscode.window.showErrorMessage(`FlowScope: Failed to initialize WASM engine: ${message}`);
    console.error('FlowScope WASM initialization failed:', error);
    return;
  }

  // Register CodeLens provider
  codeLensProvider = new FlowScopeCodeLensProvider();
  context.subscriptions.push(
    vscode.languages.registerCodeLensProvider({ language: 'sql' }, codeLensProvider)
  );

  // Register Hover provider
  const hoverProvider = new FlowScopeHoverProvider();
  context.subscriptions.push(
    vscode.languages.registerHoverProvider({ language: 'sql' }, hoverProvider)
  );

  // Register Diagnostics provider
  diagnosticsProvider = new FlowScopeDiagnosticsProvider();
  context.subscriptions.push(diagnosticsProvider);

  // Register commands
  context.subscriptions.push(
    vscode.commands.registerCommand(
      'flowscope.showLineage',
      (uri?: vscode.Uri, statementIndex?: number) => {
        const document = uri
          ? vscode.workspace.textDocuments.find((d) => d.uri.toString() === uri.toString())
          : vscode.window.activeTextEditor?.document;

        if (document && document.languageId === 'sql') {
          LineagePanel.createOrShow(context.extensionUri, context.extensionPath, document, statementIndex);
        } else {
          vscode.window.showWarningMessage('FlowScope: Please open a SQL file first.');
        }
      }
    )
  );

  context.subscriptions.push(
    vscode.commands.registerCommand('flowscope.analyzeFile', () => {
      const document = vscode.window.activeTextEditor?.document;
      if (document && document.languageId === 'sql') {
        LineagePanel.createOrShow(context.extensionUri, context.extensionPath, document);
      } else {
        vscode.window.showWarningMessage('FlowScope: Please open a SQL file first.');
      }
    })
  );

  // Show status bar item
  const statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
  statusBarItem.text = '$(graph) FlowScope';
  statusBarItem.tooltip = 'Click to show SQL lineage';
  statusBarItem.command = 'flowscope.showLineage';
  context.subscriptions.push(statusBarItem);

  // Show/hide status bar based on active editor
  const updateStatusBar = () => {
    const editor = vscode.window.activeTextEditor;
    if (editor && editor.document.languageId === 'sql') {
      statusBarItem.show();
    } else {
      statusBarItem.hide();
    }
  };

  context.subscriptions.push(vscode.window.onDidChangeActiveTextEditor(updateStatusBar));
  updateStatusBar();

  // Show welcome message
  if (isWasmInitialized()) {
    vscode.window.showInformationMessage(
      `FlowScope SQL Lineage is ready! Engine v${getEngineVersion()}`
    );
  }

  console.log('FlowScope extension activated successfully.');
}

export function deactivate(): void {
  console.log('FlowScope extension deactivating...');

  if (diagnosticsProvider) {
    diagnosticsProvider.dispose();
  }

  if (LineagePanel.currentPanel) {
    LineagePanel.currentPanel.dispose();
  }
}
