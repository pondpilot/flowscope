import type { ChatMessage, LibrarianContext } from '../types';

const SQL_MAX_LENGTH = 3000;

export const DEFAULT_LIBRARIAN_SYSTEM_PROMPT = `You are an expert on SQL lineage and data flow. Your task is to answer the user's question using the provided context (SQL code, lineage analysis, and uploaded documentation).
- Only answer questions related to tables, columns, lineage, or uploaded documentation
- Politely decline off-topic questions with EXACTLY: "I can only answer questions related to your data."
- Maintain a professional and helpful tone

When answering a question, use this EXACT format. Each section is 1-2 sentences answering the question from that source.

**Summary:** [short answer combining all sources]

**Data Lineage:** [answer from Data Lineage and SQL Code sources ONLY. Write joins and mappings as expressions, not as descriptive sentences.]

**Documentation:** [answer from Documentation ONLY, cite the source file name]

CRITICAL RULES:
- Never mix sources between sections.
- If a source has no relevant information, write "No information".
- Write table and column names as inline code: MANDT
- Business concepts should be mentioned with the technical name in parentheses: document number (BELNR).
- NEVER invent information.
`;

/**
 * Builds a LibrarianContext from raw inputs, truncating SQL as needed.
 */
export function buildContext(params: {
  lineage: string;
  pdfCitations: string;
  chatHistory: ChatMessage[];
  sqlSnippet: string;
}): LibrarianContext {
  const truncatedSql =
    params.sqlSnippet.length > SQL_MAX_LENGTH
      ? params.sqlSnippet.slice(0, SQL_MAX_LENGTH) + '... (truncated)'
      : params.sqlSnippet;

  const chatHistoryText = params.chatHistory.map((m) => `${m.role}: ${m.content}`).join('\n');

  return {
    lineage: params.lineage,
    pdfCitations: params.pdfCitations,
    chatHistory: chatHistoryText,
    sqlSnippet: truncatedSql,
  };
}

/**
 * Assembles a structured prompt from a LibrarianContext.
 * Omits sections that are empty.
 */
export function buildPrompt(
  context: LibrarianContext,
  options: { systemPrompt?: string } = {}
): string {
  const sections: string[] = [];
  const systemPrompt = options.systemPrompt?.trim() || DEFAULT_LIBRARIAN_SYSTEM_PROMPT;

  sections.push(systemPrompt);

  if (context.lineage) {
    sections.push(`## DATA SOURCE: Data Lineage (from SQL analysis)\n${context.lineage}`);
  }

  if (context.sqlSnippet) {
    sections.push(
      `## DATA SOURCE: SQL Code (from SQL analysis)\n\`\`\`sql\n${context.sqlSnippet}\n\`\`\``
    );
  }

  if (context.pdfCitations) {
    sections.push(`## DATA SOURCE: Documentation (from uploaded PDFs)\n${context.pdfCitations}`);
  }

  if (context.chatHistory) {
    sections.push(`## Conversation History\n${context.chatHistory}`);
  }

  return sections.join('\n\n');
}

export function getPromptStats(prompt: string): { characters: number; bytes: number } {
  return {
    characters: prompt.length,
    bytes: new TextEncoder().encode(prompt).length,
  };
}
