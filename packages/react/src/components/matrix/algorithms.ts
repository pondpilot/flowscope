import type { MatrixCellData } from '../../utils/matrixUtils';
import { CLUSTERING_ITERATIONS } from './constants';
import type { FilterMode, TransitiveSet, XRayFilterMode } from './types';

export function clusterItems(
  items: string[],
  cells: Map<string, Map<string, MatrixCellData>>
): string[] {
  let currentOrder = [...items];

  for (let iter = 0; iter < CLUSTERING_ITERATIONS; iter++) {
    const positions = new Map(currentOrder.map((id, i) => [id, i]));
    const newOrder = [...currentOrder].sort((a, b) => {
      const getBarycenter = (node: string) => {
        let sum = 0;
        let count = 0;
        const row = cells.get(node);
        if (row) {
          for (const [target, cell] of row.entries()) {
            if (cell.type !== 'none' && cell.type !== 'self') {
              sum += positions.get(target) || 0;
              count++;
            }
          }
        }
        return count === 0 ? positions.get(node)! : sum / count;
      };
      return getBarycenter(a) - getBarycenter(b);
    });
    currentOrder = newOrder;
  }

  return currentOrder;
}

export function getTransitiveFlow(
  startNode: string,
  cells: Map<string, Map<string, MatrixCellData>>,
  items: string[]
): TransitiveSet {
  const ancestors = new Set<string>();
  const descendants = new Set<string>();

  const descendantQueue = [startNode];
  while (descendantQueue.length > 0) {
    const current = descendantQueue.shift()!;
    const row = cells.get(current);
    if (row) {
      for (const [target, cell] of row.entries()) {
        if (cell.type === 'write' && !descendants.has(target) && target !== startNode) {
          descendants.add(target);
          descendantQueue.push(target);
        }
      }
    }
  }

  const ancestorQueue = [startNode];
  while (ancestorQueue.length > 0) {
    const current = ancestorQueue.shift()!;
    for (const item of items) {
      const cell = cells.get(item)?.get(current);
      if (cell && cell.type === 'write' && !ancestors.has(item) && item !== startNode) {
        ancestors.add(item);
        ancestorQueue.push(item);
      }
    }
  }

  return { ancestors, descendants };
}

interface FilterItemsParams {
  items: string[];
  filterMode: FilterMode;
  filterText: string;
  matchingFieldNodes: Set<string> | null;
  xRayMode: boolean;
  xRayFilterMode: XRayFilterMode;
  activeXRaySet: Set<string> | null;
  targetMode: 'rows' | 'columns';
}

export function filterItems({
  items,
  filterMode,
  filterText,
  matchingFieldNodes,
  xRayMode,
  xRayFilterMode,
  activeXRaySet,
  targetMode,
}: FilterItemsParams): string[] {
  if (filterMode === 'fields' && matchingFieldNodes) {
    return items.filter((item) => matchingFieldNodes.has(item));
  }
  if (xRayMode && xRayFilterMode === 'hide' && activeXRaySet) {
    return items.filter((item) => activeXRaySet.has(item));
  }
  if (filterMode === targetMode && filterText) {
    const lower = filterText.toLowerCase();
    return items.filter((item) => item.toLowerCase().includes(lower));
  }

  return items;
}
