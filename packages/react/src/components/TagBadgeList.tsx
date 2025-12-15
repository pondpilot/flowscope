import type { ColumnTag } from '@pondpilot/flowscope-core';
import { getTagColor } from '../utils/tagColors';

interface TagBadgeListProps {
  tags?: ColumnTag[];
  compact?: boolean;
  max?: number;
  style?: React.CSSProperties;
}

export function TagBadgeList({ tags, compact = false, max = 3, style }: TagBadgeListProps): JSX.Element | null {
  if (!tags || tags.length === 0) return null;
  const displayTags = tags.slice(0, max);
  const remaining = tags.length - displayTags.length;

  return (
    <div
      style={{
        display: 'flex',
        flexWrap: 'wrap',
        gap: compact ? 4 : 6,
        marginTop: compact ? 2 : 6,
        ...style,
      }}
    >
      {displayTags.map((tag) => {
        const color = getTagColor(tag.name);
        return (
          <span
            key={`${tag.name}-${tag.inherited ? 'inherited' : 'explicit'}`}
            style={{
              fontSize: compact ? 9 : 10,
              textTransform: 'uppercase',
              letterSpacing: 0.3,
              borderRadius: 9999,
              border: tag.inherited ? `1px dashed ${color}` : `1px solid ${color}`,
              padding: compact ? '0px 4px' : '1px 6px',
              color,
              backgroundColor: `${color}1A`,
            }}
          >
            {tag.name}
          </span>
        );
      })}
      {remaining > 0 && (
        <span
          style={{
            fontSize: compact ? 9 : 10,
            color: '#6B7280',
            fontWeight: 500,
          }}
        >
          +{remaining}
        </span>
      )}
    </div>
  );
}
