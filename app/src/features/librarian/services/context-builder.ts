import type { ChatMessage, LibrarianContext } from '../types';

const SQL_MAX_LENGTH = 3000;

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
export function buildPrompt(context: LibrarianContext): string {
  const sections: string[] = [];

  sections.push(
    `You are a SQL lineage assistant. Answer questions ONLY based on the data provided below (SQL, lineage analysis, and uploaded documentation). You may explain and clarify the provided information, but you must NOT introduce new facts or use general knowledge beyond what is given.

If the provided data does not contain relevant information, respond: "Based on the current data, there is no information on your question."

Always use this exact response format:

**Summary**
1-2 sentences answering the question directly.

**Data Lineage**
State the answer as compactly as possible using the most natural notation for the question:
- For "where is X stored?" questions: "<column> in <table> (exposed as <output name>)".
- For join/relationship questions: write the join condition directly, e.g. "rseg.EBELN = ekko.EBELN and rseg.EBELP = ekpo.EBELP".
- For transformation questions: "<source> → <output>" is fine.
- If lineage is not relevant, write "No information."
- 1-2 sentences. No commentary, no qualifiers ("only", "directly"), no statements about what other tables do or don't contain, no explanation of why.

**Documentation**
1-2 sentences based on uploaded PDFs. Write "No information." if no PDFs are provided or relevant.

Keep all sections concise.

IMPORTANT: The "Data Lineage" section in your response must ONLY contain information from the "DATA SOURCE: Data Lineage" and "DATA SOURCE: SQL Code" sections. The "Documentation" section in your response must ONLY contain information from the "DATA SOURCE: Documentation" section. Never mix information between these sources.`
  );

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
