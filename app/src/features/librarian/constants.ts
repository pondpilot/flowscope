/** Chat limits */
export const CHAT_HISTORY_LIMIT = 10;
export const MAX_MESSAGE_LENGTH = 4000;

/** PDF processing */
export const MAX_PDF_SIZE_MB = 10;
export const MAX_PDF_SIZE_BYTES = MAX_PDF_SIZE_MB * 1024 * 1024;
export const PDF_CHUNK_SIZE = 500;
export const PDF_CHUNK_OVERLAP = 50;

/** Vector search */
export const VECTOR_SEARCH_TOP_K = 5;

/** Embedding model */
export const EMBEDDING_MODEL = 'Xenova/multilingual-e5-small';

/** LocalStorage keys */
export const STORAGE_KEY_AI_PROVIDER = 'librarian-ai-provider';
export const STORAGE_KEY_AI_API_KEY = 'librarian-ai-api-key';
export const STORAGE_KEY_AI_MODEL = 'librarian-ai-model';
export const STORAGE_KEY_AI_ENDPOINT = 'librarian-ai-endpoint';
export const STORAGE_KEY_AI_SYSTEM_PROMPT = 'librarian-ai-system-prompt';
