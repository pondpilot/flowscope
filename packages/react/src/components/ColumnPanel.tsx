import { useMemo } from 'react';
import { ArrowRight, Columns3 } from 'lucide-react';
import { useLineage } from '../store';
import type { ColumnPanelProps } from '../types';
import type { Node, Edge } from '@pondpilot/flowscope-core';
import { isTableLikeType } from '@pondpilot/flowscope-core';
import { COLORS } from '../constants';

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
    if (isTableLikeType(selectedNode.type)) {
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

  if (isTableLikeType(selectedNode.type)) {
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
            <h4 style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
              <span
                style={{
                  width: '16px',
                  height: '2px',
                  backgroundColor: COLORS.edges.dataFlow,
                  borderRadius: '1px',
                }}
              />
              Upstream ({columnInfo.upstream.length})
            </h4>
            <ul className="flowscope-column-list">
              {columnInfo.upstream.map((node) => (
                <li
                  key={node.id}
                  onClick={() => handleColumnClick(node)}
                  className="flowscope-column-item"
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: '6px',
                    cursor: 'pointer',
                  }}
                >
                  <Columns3 style={{ width: '12px', height: '12px', color: COLORS.nodes.table.textSecondary }} />
                  <span>{node.label}</span>
                  {node.qualifiedName && (() => {
                    const prefix = node.qualifiedName.split('.').slice(0, -1).join('.');
                    return prefix ? (
                      <span style={{ fontSize: '10px', color: COLORS.nodes.table.textSecondary }}>
                        ({prefix})
                      </span>
                    ) : null;
                  })()}
                </li>
              ))}
            </ul>
          </div>
        )}

        {columnInfo && columnInfo.downstream.length > 0 && (
          <div className="flowscope-section">
            <h4 style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
              <span
                style={{
                  width: '16px',
                  height: '2px',
                  backgroundColor: COLORS.edges.derivation,
                  borderRadius: '1px',
                }}
              />
              Downstream ({columnInfo.downstream.length})
            </h4>
            <ul className="flowscope-column-list">
              {columnInfo.downstream.map((node) => (
                <li
                  key={node.id}
                  onClick={() => handleColumnClick(node)}
                  className="flowscope-column-item"
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: '6px',
                    cursor: 'pointer',
                  }}
                >
                  <Columns3 style={{ width: '12px', height: '12px', color: COLORS.nodes.table.textSecondary }} />
                  <span>{node.label}</span>
                </li>
              ))}
            </ul>
          </div>
        )}

        {flowPath.length > 1 && (
          <div className="flowscope-section">
            <h4>Data Flow</h4>
            <div className="flowscope-flow-path" style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: '4px' }}>
              {flowPath.map((label, idx) => {
                const isSelected = label === selectedNode.label;
                return (
                  <span key={`${label}-${idx}`} style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                    <span
                      className="flowscope-flow-chip"
                      style={{
                        backgroundColor: isSelected ? COLORS.interactive.selection : 'var(--flowscope-surface-muted)',
                        color: isSelected ? '#FFFFFF' : 'var(--flowscope-text)',
                        padding: '2px 8px',
                        borderRadius: '4px',
                        fontSize: '12px',
                        fontWeight: isSelected ? 600 : 400,
                      }}
                    >
                      {label}
                    </span>
                    {idx < flowPath.length - 1 && (
                      <ArrowRight
                        className="flowscope-flow-arrow"
                        style={{ width: '12px', height: '12px', color: COLORS.edges.dataFlow }}
                        aria-hidden="true"
                      />
                    )}
                  </span>
                );
              })}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
