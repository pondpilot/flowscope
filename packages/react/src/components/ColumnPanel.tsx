import { useMemo } from 'react';
import { useLineage } from '../store';
import type { ColumnPanelProps } from '../types';
import type { Node, Edge } from '@pondpilot/flowscope-core';

interface ColumnInfo {
  node: Node;
  upstream: Node[];
  downstream: Node[];
}

function findColumnInfo(
  nodes: Node[],
  edges: Edge[],
  nodeId: string
): ColumnInfo | null {
  const node = nodes.find((n) => n.id === nodeId);
  if (!node) return null;

  const upstream: Node[] = [];
  const downstream: Node[] = [];

  for (const edge of edges) {
    if (edge.to === nodeId && (edge.type === 'data_flow' || edge.type === 'derivation')) {
      const sourceNode = nodes.find((n) => n.id === edge.from);
      if (sourceNode) upstream.push(sourceNode);
    }
    if (edge.from === nodeId && (edge.type === 'data_flow' || edge.type === 'derivation')) {
      const targetNode = nodes.find((n) => n.id === edge.to);
      if (targetNode) downstream.push(targetNode);
    }
  }

  return { node, upstream, downstream };
}

function findTableColumns(
  nodes: Node[],
  edges: Edge[],
  tableId: string
): Node[] {
  const columns: Node[] = [];
  for (const edge of edges) {
    if (edge.from === tableId && edge.type === 'ownership') {
      const col = nodes.find((n) => n.id === edge.to);
      if (col) columns.push(col);
    }
  }
  return columns;
}

export function ColumnPanel({ className }: ColumnPanelProps): JSX.Element {
  const { state, actions } = useLineage();
  const { result, selectedStatementIndex, selectedNodeId } = state;

  const statement = result?.statements[selectedStatementIndex];

  const selectedNode = useMemo(() => {
    if (!statement || !selectedNodeId) return null;
    return statement.nodes.find((n) => n.id === selectedNodeId) || null;
  }, [statement, selectedNodeId]);

  const columnInfo = useMemo(() => {
    if (!statement || !selectedNodeId) return null;
    return findColumnInfo(statement.nodes, statement.edges, selectedNodeId);
  }, [statement, selectedNodeId]);

  const tableColumns = useMemo(() => {
    if (!statement || !selectedNode) return [];
    if (selectedNode.type === 'table' || selectedNode.type === 'cte') {
      return findTableColumns(statement.nodes, statement.edges, selectedNode.id);
    }
    return [];
  }, [statement, selectedNode]);

  const flowPath = useMemo(() => {
    if (!selectedNode || !columnInfo) return [];
    const upstreamLabels = columnInfo.upstream.map((node) => node.label);
    const downstreamLabels = columnInfo.downstream.map((node) => node.label);
    const segments: string[] = [];
    if (upstreamLabels.length > 0) {
      segments.push(...upstreamLabels);
    }
    segments.push(selectedNode.label);
    if (downstreamLabels.length > 0) {
      segments.push(...downstreamLabels);
    }
    return segments;
  }, [selectedNode, columnInfo]);

  const handleColumnClick = (node: Node) => {
    actions.selectNode(node.id);
    if (node.span) {
      actions.highlightSpan(node.span);
    }
  };

  if (!result || !statement) {
    return (
      <div className={`flowscope-column-panel flowscope-panel-empty ${className || ''}`}>
        <p>No data available</p>
      </div>
    );
  }

  if (!selectedNode) {
    return (
      <div className={`flowscope-column-panel ${className || ''}`}>
        <div className="flowscope-panel-header">
          <h3>Column Details</h3>
        </div>
        <div className="flowscope-panel-content">
          <p className="flowscope-hint">Select a node to view column details</p>
        </div>
      </div>
    );
  }

  if (selectedNode.type === 'table' || selectedNode.type === 'cte') {
    return (
      <div className={`flowscope-column-panel ${className || ''}`}>
        <div className="flowscope-panel-header">
          <h3>{selectedNode.label}</h3>
          <span className="flowscope-badge">{selectedNode.type}</span>
        </div>
        <div className="flowscope-panel-content">
          {selectedNode.qualifiedName && (
            <div className="flowscope-detail">
              <span className="flowscope-label">Qualified Name:</span>
              <code>{selectedNode.qualifiedName}</code>
            </div>
          )}
          <div className="flowscope-section">
            <h4>Columns ({tableColumns.length})</h4>
            {tableColumns.length === 0 ? (
              <p className="flowscope-hint">No columns found</p>
            ) : (
              <ul className="flowscope-column-list">
                {tableColumns.map((col) => (
                  <li
                    key={col.id}
                    onClick={() => handleColumnClick(col)}
                    className="flowscope-column-item"
                  >
                    {col.label}
                    {col.expression && (
                      <code className="flowscope-expression">{col.expression}</code>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className={`flowscope-column-panel ${className || ''}`}>
      <div className="flowscope-panel-header">
        <h3>{selectedNode.label}</h3>
        <span className="flowscope-badge">column</span>
      </div>
      <div className="flowscope-panel-content">
        {selectedNode.expression && (
          <div className="flowscope-detail">
            <span className="flowscope-label">Expression:</span>
            <code>{selectedNode.expression}</code>
          </div>
        )}

        {columnInfo && columnInfo.upstream.length > 0 && (
          <div className="flowscope-section">
            <h4>Upstream ({columnInfo.upstream.length})</h4>
            <ul className="flowscope-column-list">
              {columnInfo.upstream.map((node) => (
                <li
                  key={node.id}
                  onClick={() => handleColumnClick(node)}
                  className="flowscope-column-item"
                >
                  {node.label}
                </li>
              ))}
            </ul>
          </div>
        )}

        {columnInfo && columnInfo.downstream.length > 0 && (
          <div className="flowscope-section">
            <h4>Downstream ({columnInfo.downstream.length})</h4>
            <ul className="flowscope-column-list">
              {columnInfo.downstream.map((node) => (
                <li
                  key={node.id}
                  onClick={() => handleColumnClick(node)}
                  className="flowscope-column-item"
                >
                  {node.label}
                </li>
              ))}
            </ul>
          </div>
        )}

        {flowPath.length > 1 && (
          <div className="flowscope-section">
            <h4>Data Flow</h4>
            <div className="flowscope-flow-path">
              {flowPath.map((label, idx) => (
                <span key={`${label}-${idx}`} className="flowscope-flow-chip">
                  {label}
                  {idx < flowPath.length - 1 && (
                    <span className="flowscope-flow-arrow" aria-hidden="true">
                      →
                    </span>
                  )}
                </span>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
