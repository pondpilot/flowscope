import { type JSX, type MouseEvent } from 'react';

import { useLineageActions, useLineageStore } from '../store';
import { useColors } from '../hooks/useColors';
import { findMergedNodeById } from '../utils/nodeOccurrences';

interface OccurrenceCyclerProps {
  /** The graph node id this cycler belongs to. */
  nodeId: string;
}

/**
 * Renders the per-node ◀ n/total ▶ cycler used by graph→text navigation.
 *
 * Shown only when the node is currently selected and has more than one
 * `nameSpans` entry — single-occurrence nodes already had their one location
 * highlighted on click, so an unclickable "1/1" badge would just be noise.
 *
 * The buttons stop propagation so that clicking the arrows does not also
 * trigger the parent node's click handler (which would reset the focused
 * occurrence index back to 0).
 */
export function OccurrenceCycler({ nodeId }: OccurrenceCyclerProps): JSX.Element | null {
  const { cycleOccurrence } = useLineageActions();
  const isSelected = useLineageStore((state) => state.selectedNodeId === nodeId);
  const focusedIndex = useLineageStore((state) => state.focusedOccurrenceIndex);
  const result = useLineageStore((state) => state.result);
  const colors = useColors();

  if (!isSelected || result === null) {
    return null;
  }

  const node = findMergedNodeById(result, nodeId);
  const total = node?.nameSpans?.length ?? 0;
  if (total < 2) {
    return null;
  }

  const handlePrev = (event: MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    cycleOccurrence('prev');
  };
  const handleNext = (event: MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    cycleOccurrence('next');
  };

  const buttonStyle = {
    background: 'none',
    border: 'none',
    cursor: 'pointer',
    padding: '2px 4px',
    color: colors.nodes.table.textSecondary,
    display: 'inline-flex',
    alignItems: 'center',
    borderRadius: 3,
    lineHeight: 1,
  } as const;

  // 1-based for users; 0-based for state.
  const displayIndex = focusedIndex + 1;

  return (
    <span
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 2,
        backgroundColor: `${colors.accent}12`,
        color: colors.accent,
        borderRadius: 999,
        padding: '2px 4px',
        fontSize: 10,
        fontWeight: 600,
        fontVariantNumeric: 'tabular-nums',
      }}
      title={`Cycle through ${total} occurrences of this name in the SQL (n / Shift+n)`}
      aria-label={`Occurrence ${displayIndex} of ${total}`}
      role="status"
      aria-live="polite"
      aria-atomic="true"
    >
      <button
        type="button"
        onClick={handlePrev}
        style={buttonStyle}
        aria-label="Previous occurrence"
      >
        ◀
      </button>
      <span style={{ minWidth: 22, textAlign: 'center' }}>
        {displayIndex}/{total}
      </span>
      <button type="button" onClick={handleNext} style={buttonStyle} aria-label="Next occurrence">
        ▶
      </button>
    </span>
  );
}
