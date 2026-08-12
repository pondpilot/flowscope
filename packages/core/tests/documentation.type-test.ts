import {
  analyzeSql,
  edgesInStatement,
  nodesInStatement,
  type AnalyzeRequest,
  type AnalyzeResult,
} from '../src';

async function documentationExample(): Promise<void> {
  const request = {
    sql: 'SELECT * FROM users JOIN orders ON users.id = orders.user_id',
    dialect: 'postgres',
    sourceName: 'example.sql',
  } satisfies AnalyzeRequest;

  const result = await analyzeSql(request);

  console.log(result.nodes, result.edges);
  for (const statement of result.statements) {
    console.log(
      nodesInStatement(result, statement.statementIndex),
      edgesInStatement(result, statement.statementIndex)
    );

    // @ts-expect-error StatementMeta contains metadata, not graph collections.
    console.log(statement.nodes);
    // @ts-expect-error StatementMeta contains metadata, not graph collections.
    console.log(statement.edges);
  }

  // @ts-expect-error The flat graph replaces the removed globalLineage wrapper.
  console.log(result.globalLineage);
}

const staleRequest: AnalyzeRequest = {
  sql: 'SELECT 1',
  dialect: 'postgres',
  // @ts-expect-error Use sourceName; filePath is not a current request field.
  filePath: 'example.sql',
};

declare const typedResult: AnalyzeResult;
void typedResult.nodes;
void typedResult.edges;
void documentationExample;
void staleRequest;
