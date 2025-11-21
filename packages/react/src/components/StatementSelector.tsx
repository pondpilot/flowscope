import { useLineage } from '../context';

const styles = {
  container: {
    display: 'flex',
    gap: 4,
    padding: '8px 12px',
    borderBottom: '1px solid #DBDDE1',
    backgroundColor: '#F2F4F8',
    overflowX: 'auto' as const,
  },
  tab: {
    padding: '6px 12px',
    fontSize: 12,
    fontWeight: 500,
    borderRadius: 6,
    border: 'none',
    cursor: 'pointer',
    transition: 'all 0.15s ease',
    fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
  },
  tabActive: {
    backgroundColor: '#4C61FF',
    color: '#FFFFFF',
  },
  tabInactive: {
    backgroundColor: 'transparent',
    color: '#6F7785',
  },
};

export interface StatementSelectorProps {
  className?: string;
}

export function StatementSelector({ className }: StatementSelectorProps): JSX.Element | null {
  const { state, actions } = useLineage();
  const { result, selectedStatementIndex } = state;

  if (!result || result.statements.length <= 1) {
    return null;
  }

  return (
    <div className={className} style={styles.container}>
      {result.statements.map((stmt, index) => (
        <button
          key={index}
          onClick={() => actions.selectStatement(index)}
          style={{
            ...styles.tab,
            ...(index === selectedStatementIndex ? styles.tabActive : styles.tabInactive),
          }}
          onMouseEnter={(e) => {
            if (index !== selectedStatementIndex) {
              e.currentTarget.style.backgroundColor = '#E5E9F2';
            }
          }}
          onMouseLeave={(e) => {
            if (index !== selectedStatementIndex) {
              e.currentTarget.style.backgroundColor = 'transparent';
            }
          }}
        >
          Statement {index + 1}: {stmt.statementType}
        </button>
      ))}
    </div>
  );
}
