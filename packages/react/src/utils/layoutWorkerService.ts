/**
 * Service for communicating with the layout Web Worker.
 * Manages worker lifecycle and request/response handling.
 */
import type { Node, Edge } from '@xyflow/react';
import type { LayoutAlgorithm } from './layout';
import type {
  WorkerNodeData,
  WorkerEdgeData,
  LayoutRequest,
  LayoutResponse,
} from '../workers/layout.worker';
import { LAYOUT_DEBUG } from './debug';

interface NodeData extends Record<string, unknown> {
  columns?: { id: string }[];
  filters?: { expression: string }[];
  isCollapsed?: boolean;
}

interface PendingRequest {
  resolve: (positions: Record<string, { x: number; y: number }>) => void;
  reject: (error: Error) => void;
}

let worker: Worker | null = null;
let requestIdCounter = 0;
const pendingRequests = new Map<string, PendingRequest>();

/**
 * Get or create the layout worker instance.
 */
function getWorker(): Worker {
  if (!worker) {
    // Vite handles worker bundling with this syntax
    worker = new Worker(new URL('../workers/layout.worker.ts', import.meta.url), {
      type: 'module',
    });

    worker.onmessage = (event: MessageEvent<LayoutResponse>) => {
      const { requestId, positions, error } = event.data;

      const pending = pendingRequests.get(requestId);
      if (pending) {
        pendingRequests.delete(requestId);

        if (error) {
          pending.reject(new Error(error));
        } else {
          // Validate positions structure before accepting
          if (!positions || typeof positions !== 'object') {
            pending.reject(new Error('Invalid positions data from worker'));
            return;
          }
          pending.resolve(positions as Record<string, { x: number; y: number }>);
        }
      }
    };

    worker.onerror = (error) => {
      console.error('[LayoutWorker] Worker error:', error);
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
 * Convert React Flow nodes to serializable worker format.
 */
function nodesToWorkerFormat<N extends NodeData>(nodes: Node<N>[]): WorkerNodeData[] {
  return nodes.map((node) => ({
    id: node.id,
    columnCount: node.data?.columns?.length ?? 0,
    filterCount: node.data?.filters?.length ?? 0,
    isCollapsed: node.data?.isCollapsed ?? false,
  }));
}

/**
 * Convert React Flow edges to serializable worker format.
 */
function edgesToWorkerFormat<E extends Record<string, unknown>>(
  edges: Edge<E>[]
): WorkerEdgeData[] {
  return edges.map((edge) => ({
    id: edge.id,
    source: edge.source,
    target: edge.target,
  }));
}

/**
 * Request layout computation from the Web Worker.
 * Returns a promise that resolves with node positions.
 */
export async function computeLayoutInWorker<N extends NodeData, E extends Record<string, unknown>>(
  nodes: Node<N>[],
  edges: Edge<E>[],
  direction: 'LR' | 'TB',
  algorithm: LayoutAlgorithm
): Promise<Record<string, { x: number; y: number }>> {
  if (nodes.length === 0) {
    return {};
  }

  const requestId = `layout-${++requestIdCounter}`;
  const workerInstance = getWorker();

  if (LAYOUT_DEBUG) console.time('[Layout Worker] Serialize nodes/edges');
  const workerNodes = nodesToWorkerFormat(nodes);
  const workerEdges = edgesToWorkerFormat(edges);
  if (LAYOUT_DEBUG) console.timeEnd('[Layout Worker] Serialize nodes/edges');

  return new Promise((resolve, reject) => {
    pendingRequests.set(requestId, { resolve, reject });

    const request: LayoutRequest = {
      type: 'layout',
      requestId,
      nodes: workerNodes,
      edges: workerEdges,
      direction,
      algorithm,
    };

    if (LAYOUT_DEBUG) console.time('[Layout Worker] postMessage to worker');
    workerInstance.postMessage(request);
    if (LAYOUT_DEBUG) console.timeEnd('[Layout Worker] postMessage to worker');
  });
}

/**
 * Apply computed positions to React Flow nodes.
 */
export function applyPositionsToNodes<N extends NodeData>(
  nodes: Node<N>[],
  positions: Record<string, { x: number; y: number }>
): Node<N>[] {
  return nodes.map((node) => {
    const position = positions[node.id];
    if (position) {
      return {
        ...node,
        position,
      };
    }
    return node;
  });
}

/**
 * Cancel all pending layout requests.
 * Call this when the component unmounts or when new layout is requested.
 */
export function cancelPendingLayouts(): void {
  for (const pending of pendingRequests.values()) {
    pending.reject(new Error('Layout cancelled'));
  }
  pendingRequests.clear();
}

/**
 * Terminate the worker when no longer needed.
 * Call this on app shutdown or when switching views.
 */
export function terminateLayoutWorker(): void {
  if (worker) {
    cancelPendingLayouts();
    worker.terminate();
    worker = null;
  }
}

/**
 * Check if Web Workers are supported in the current environment.
 */
export function isWorkerSupported(): boolean {
  return typeof Worker !== 'undefined';
}
