/**
 * Sanitization utilities for user input and SQL content.
 *
 * These utilities help prevent XSS attacks by escaping potentially dangerous characters
 * in user-provided content before rendering or exporting.
 */

/**
 * Escapes HTML special characters to prevent XSS attacks.
 *
 * @param text - The text to escape
 * @returns The escaped text safe for HTML rendering
 *
 * @example
 * ```typescript
 * const userInput = '<script>alert("xss")</script>';
 * const safe = escapeHtml(userInput);
 * // Returns: '&lt;script&gt;alert(&quot;xss&quot;)&lt;/script&gt;'
 * ```
 */
export function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

/**
 * Sanitizes SQL content for safe display.
 * Removes or escapes potentially dangerous content while preserving SQL syntax.
 *
 * @param sql - The SQL content to sanitize
 * @returns Sanitized SQL content
 *
 * @example
 * ```typescript
 * const userSql = 'SELECT * FROM users WHERE id = 1; <script>alert("xss")</script>';
 * const safe = sanitizeSqlContent(userSql);
 * ```
 */
export function sanitizeSqlContent(sql: string): string {
  if (!sql || typeof sql !== 'string') {
    return '';
  }

  // Escape HTML special characters
  return escapeHtml(sql);
}

/**
 * Sanitizes error messages that may contain user input.
 *
 * @param message - The error message to sanitize
 * @returns Sanitized error message
 */
export function sanitizeErrorMessage(message: string): string {
  if (!message || typeof message !== 'string') {
    return 'An unknown error occurred';
  }

  return escapeHtml(message);
}

/**
 * Sanitizes table and column names for safe display.
 * These are typically derived from SQL parsing but should still be sanitized.
 *
 * @param name - The table or column name to sanitize
 * @returns Sanitized name
 */
export function sanitizeIdentifier(name: string): string {
  if (!name || typeof name !== 'string') {
    return '';
  }

  // Remove any HTML tags and escape special characters
  const withoutTags = name.replace(/<[^>]*>/g, '');
  return escapeHtml(withoutTags);
}
