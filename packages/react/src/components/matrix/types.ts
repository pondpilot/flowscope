import type { MatrixData } from '../../utils/matrixUtils';

export interface MatrixMetrics {
  rowCounts: Map<string, number>;
  colCounts: Map<string, number>;
  maxRow: number;
  maxCol: number;
  maxIntensity: number;
}

export interface MatrixWorkerPayload {
  tableMatrix: MatrixData;
  scriptMatrix: MatrixData;
  allColumnNames: string[];
  tableMetrics: MatrixMetrics;
  scriptMetrics: MatrixMetrics;
  tableItemCount: number;
  tableItemsRendered: number;
  scriptItemCount: number;
  scriptItemsRendered: number;
}

export type FilterMode = 'rows' | 'columns' | 'fields';
export type XRayFilterMode = 'dim' | 'hide';

export interface MatrixViewControlledState {
  filterText: string;
  filterMode: FilterMode;
  heatmapMode: boolean;
  xRayMode: boolean;
  xRayFilterMode: XRayFilterMode;
  clusterMode: boolean;
  complexityMode: boolean;
  showLegend: boolean;
  focusedNode: string | null;
  firstColumnWidth: number;
  headerHeight: number;
}

export interface TransitiveSet {
  ancestors: Set<string>;
  descendants: Set<string>;
}
