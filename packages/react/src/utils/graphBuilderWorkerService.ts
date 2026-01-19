/**
 * Service for communicating with the graph builder Web Worker.
 * Manages worker lifecycle and request/response handling.
 *
 * This service offloads the CPU-intensive graph building (buildFlowNodes, buildFlowEdges)
 * to a Web Worker, preventing UI blocking when processing large SQL files.
 */
import type { StatementLineage, ResolvedSchemaMetadata, GlobalLineage, Node as LineageNode } from '@pondpilot/flowscope-core';
import type { Node as FlowNode, Edge as FlowEdge } from '@xyflow/react';
import type {
  GraphBuildRequest,
  ScriptGraphBuildRequest,
  GraphBuildResponse,
  SerializedFlowNode,
  SerializedFlowEdge,
  SerializedTableNodeData,
  SerializedScriptNodeData,
} from '../workers/graphBuilder.worker';

interface PendingRequest {
  resolve: (result: { nodes: FlowNode[]; edges: FlowEdge[]; lineageNodes?: LineageNode[] }) => void;
  reject: (error: Error) => void;
}

let worker: Worker | null = null;
let requestIdCounter = 0;
const pendingRequests = new Map<string, PendingRequest>();

/**
 * Get or create the graph builder worker instance.
 */
function getWorker(): Worker {
  if (!worker) {
    // Vite handles worker bundling with this syntax
    worker = new Worker(
      new URL('../workers/graphBuilder.worker.ts', import.meta.url),
      { type: 'module' }
    );

    worker.onmessage = (event: MessageEvent<GraphBuildResponse>) => {
      const { requestId, nodes, edges, error, lineageNodes } = event.data;

      const pending = pendingRequests.get(requestId);
      if (pending) {
        pendingRequests.delete(requestId);

        if (error) {
          pending.reject(new Error(error));
        } else {
          // Convert serialized nodes/edges back to React Flow types
          const flowNodes = deserializeNodes(nodes);
          const flowEdges = deserializeEdges(edges);
          pending.resolve({ nodes: flowNodes, edges: flowEdges, lineageNodes });
        }
      }
    };

    worker.onerror = (error) => {
      console.error('[GraphBuilder Worker] Worker error:', error);
      // Reject all pending requests
      for (const pending of pendingRequests.values()) {
        pending.reject(new Error('Worker error'));
      }
      pendingRequests.clear();
    };
  }

  return worker;
}

/**
 * Convert serialized nodes back to React Flow node format.
 * The serialized format is already compatible, we just need proper typing.
 */
function deserializeNodes(nodes: SerializedFlowNode[]): FlowNode[] {
  return nodes.map((node) => ({
    id: node.id,
    type: node.type,
    position: node.position,
    data: node.data as SerializedTableNodeData | SerializedScriptNodeData,
  }));
}

/**
 * Convert serialized edges back to React Flow edge format.
 */
function deserializeEdges(edges: SerializedFlowEdge[]): FlowEdge[] {
  return edges.map((edge) => ({
    id: edge.id,
    source: edge.source,
    target: edge.target,
    sourceHandle: edge.sourceHandle,
    targetHandle: edge.targetHandle,
    type: edge.type,
    label: edge.label,
    animated: edge.animated,
    zIndex: edge.zIndex,
    data: edge.data,
    style: edge.style,
  }));
}

/**
 * Options for building a table-view graph in the worker.
 */
export interface TableGraphBuildOptions {
  statements: StatementLineage[];
  selectedNodeId: string | null;
  searchTerm: string;
  collapsedNodeIds: Set<string>;
  expandedTableIds: Set<string>;
  resolvedSchema: ResolvedSchemaMetadata | null | undefined;
  defaultCollapsed: boolean;
  globalLineage: GlobalLineage | null | undefined;
  showColumnEdges: boolean;
}

/**
 * Options for building a script-view graph in the worker.
 */
export interface ScriptGraphBuildOptions {
  statements: StatementLineage[];
  selectedNodeId: string | null;
  searchTerm: string;
  showTables: boolean;
}

/**
 * Build a table-view graph in the Web Worker.
 * Returns a promise that resolves with the built nodes and edges.
 */
export async function buildTableGraphInWorker(
  options: TableGraphBuildOptions
): Promise<{ nodes: FlowNode[]; edges: FlowEdge[]; lineageNodes?: LineageNode[] }> {
  const requestId = `graph-${++requestIdCounter}`;
  const workerInstance = getWorker();

  // Convert Sets to arrays for serialization
  const request: GraphBuildRequest = {
    type: 'build-table-graph',
    requestId,
    statements: options.statements,
    selectedNodeId: options.selectedNodeId,
    searchTerm: options.searchTerm,
    collapsedNodeIds: Array.from(options.collapsedNodeIds),
    expandedTableIds: Array.from(options.expandedTableIds),
    resolvedSchema: options.resolvedSchema ?? null,
    defaultCollapsed: options.defaultCollapsed,
    globalLineage: options.globalLineage ?? null,
    showColumnEdges: options.showColumnEdges,
  };

  return new Promise((resolve, reject) => {
    pendingRequests.set(requestId, { resolve, reject });

    console.time('[GraphBuilder] postMessage to worker');
    workerInstance.postMessage(request);
    console.timeEnd('[GraphBuilder] postMessage to worker');
  });
}

/**
 * Build a script-view graph in the Web Worker.
 * Returns a promise that resolves with the built nodes and edges.
 */
export async function buildScriptGraphInWorker(
  options: ScriptGraphBuildOptions
): Promise<{ nodes: FlowNode[]; edges: FlowEdge[]; lineageNodes?: LineageNode[] }> {
  const requestId = `graph-${++requestIdCounter}`;
  const workerInstance = getWorker();

  const request: ScriptGraphBuildRequest = {
    type: 'build-script-graph',
    requestId,
    statements: options.statements,
    selectedNodeId: options.selectedNodeId,
    searchTerm: options.searchTerm,
    showTables: options.showTables,
  };

  return new Promise((resolve, reject) => {
    pendingRequests.set(requestId, { resolve, reject });

    console.time('[GraphBuilder] postMessage to worker');
    workerInstance.postMessage(request);
    console.timeEnd('[GraphBuilder] postMessage to worker');
  });
}

/**
 * Cancel all pending graph build requests.
 * Call this when the component unmounts or when new build is requested.
 */
export function cancelPendingBuilds(): void {
  for (const pending of pendingRequests.values()) {
    pending.reject(new Error('Build cancelled'));
  }
  pendingRequests.clear();
}

/**
 * Terminate the worker when no longer needed.
 * Call this on app shutdown or when switching views.
 */
export function terminateGraphBuilderWorker(): void {
  if (worker) {
    cancelPendingBuilds();
    worker.terminate();
    worker = null;
  }
}

/**
 * Check if Web Workers are supported in the current environment.
 */
export function isGraphBuilderWorkerSupported(): boolean {
  return typeof Worker !== 'undefined';
}
