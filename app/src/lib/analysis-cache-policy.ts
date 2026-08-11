import type { AnalysisHashInput } from './analysis-hash';

/**
 * Proactive cache restoration runs on the UI thread after edits settle. Keep
 * that opportunistic work below a small payload size; explicit analysis runs
 * always build the exact canonical key regardless of size.
 */
export const PROACTIVE_ANALYSIS_CACHE_KEY_MAX_CHARS = 250_000;

// Current canonical keys hash a two-character version and three boolean flags
// in addition to the variable semantic fields counted below.
const CANONICAL_CACHE_KEY_FIXED_CHARS = 5;

export function canBuildProactiveAnalysisCacheKey(input: AnalysisHashInput): boolean {
  let totalChars =
    CANONICAL_CACHE_KEY_FIXED_CHARS +
    input.dialect.length +
    input.schemaSQL.length +
    (input.templateMode ?? 'raw').length;

  if (totalChars > PROACTIVE_ANALYSIS_CACHE_KEY_MAX_CHARS) {
    return false;
  }

  for (const file of input.files) {
    totalChars += file.name.length + file.content.length;
    if (totalChars > PROACTIVE_ANALYSIS_CACHE_KEY_MAX_CHARS) {
      return false;
    }
  }

  return true;
}
