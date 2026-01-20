import { type ClassValue, clsx } from 'clsx';
import { twMerge } from 'tailwind-merge';
import { byteOffsetToCharOffset } from '@pondpilot/flowscope-core';

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/**
 * Convert a UTF-8 byte offset into a line:column position within a string.
 * Lines are 1-indexed, columns are 1-indexed.
 *
 * Handles UTF-8 to UTF-16 conversion internally since JavaScript strings
 * use UTF-16 encoding while FlowScope spans use UTF-8 byte offsets.
 *
 * @param content - The string content
 * @param byteOffset - UTF-8 byte offset from the start of the string
 * @returns Line and column (both 1-indexed), or { line: 1, column: 1 } if conversion fails
 */
export function byteOffsetToLineColumn(
  content: string,
  byteOffset: number
): { line: number; column: number } {
  // Handle edge cases - empty content or negative offset
  if (!content || byteOffset < 0) {
    return { line: 1, column: 1 };
  }

  // Convert UTF-8 byte offset to JavaScript character index (UTF-16 code units)
  let charOffset: number;
  try {
    charOffset = byteOffsetToCharOffset(content, byteOffset);
    // Clamp to content length in case the offset exceeds the string
    charOffset = Math.min(charOffset, content.length);
  } catch (error) {
    // If conversion fails (e.g., offset exceeds string length or doesn't land on boundary),
    // clamp to string length to provide best-effort result
    if (import.meta.env.DEV) {
      console.warn('[byteOffsetToLineColumn] Conversion failed, clamping to end:', error);
    }
    charOffset = content.length;
  }

  const textUpToOffset = content.slice(0, charOffset);
  const lines = textUpToOffset.split('\n');
  return {
    line: lines.length,
    column: (lines[lines.length - 1]?.length ?? 0) + 1,
  };
}
