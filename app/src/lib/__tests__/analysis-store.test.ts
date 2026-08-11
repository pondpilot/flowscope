import type { AnalyzeResult } from '@pondpilot/flowscope-core';
import { beforeEach, describe, expect, it } from 'vitest';
import {
  ANALYSIS_MEMORY_CACHE_MAX_ENTRIES,
  getAnalysisCacheRestoreDecision,
  useAnalysisStore,
} from '@/lib/analysis-store';
import {
  PROACTIVE_ANALYSIS_CACHE_KEY_MAX_CHARS,
  canBuildProactiveAnalysisCacheKey,
} from '@/lib/analysis-cache-policy';
import { buildAnalysisCacheKey, type AnalysisHashInput } from '@/lib/analysis-hash';

const result = { nodes: [], edges: [], statements: [], issues: [] } as unknown as AnalyzeResult;

const baseInput: AnalysisHashInput = {
  files: [{ name: 'model.sql', content: 'SELECT 1' }],
  dialect: 'generic',
  schemaSQL: '',
  hideCTEs: false,
  enableColumnLineage: true,
  enableLinting: false,
  templateMode: 'raw',
};

describe('analysis memory cache', () => {
  beforeEach(() => {
    useAnalysisStore.getState().clearAllResults();
  });

  it('reuses a result only for the exact canonical analysis key', () => {
    const cacheKey = buildAnalysisCacheKey(baseInput);

    useAnalysisStore.getState().setResult('project-1', cacheKey, result);

    expect(useAnalysisStore.getState().getResult('project-1', cacheKey)).toBe(result);
    expect(useAnalysisStore.getState().getResult('project-2', cacheKey)).toBeNull();
  });

  it('misses after SQL content or selected files change', () => {
    const originalKey = buildAnalysisCacheKey(baseInput);
    useAnalysisStore.getState().setResult('project-1', originalKey, result);

    const changedContent = {
      ...baseInput,
      files: [{ name: 'model.sql', content: 'SELECT 2' }],
    };
    const changedSelection = {
      ...baseInput,
      files: [...baseInput.files, { name: 'other.sql', content: 'SELECT 2' }],
    };

    expect(
      useAnalysisStore.getState().getResult('project-1', buildAnalysisCacheKey(changedContent))
    ).toBeNull();
    expect(
      useAnalysisStore.getState().getResult('project-1', buildAnalysisCacheKey(changedSelection))
    ).toBeNull();
  });

  it.each([
    ['dialect', { ...baseInput, dialect: 'postgres' as const }],
    ['schema', { ...baseInput, schemaSQL: 'CREATE TABLE source (id INT);' }],
    ['template configuration', { ...baseInput, templateMode: 'dbt' as const }],
    ['lint configuration', { ...baseInput, enableLinting: true }],
  ])('misses after %s changes', (_name, changedInput) => {
    const originalKey = buildAnalysisCacheKey(baseInput);
    useAnalysisStore.getState().setResult('project-1', originalKey, result);

    expect(
      useAnalysisStore.getState().getResult('project-1', buildAnalysisCacheKey(changedInput))
    ).toBeNull();
  });

  it('evicts the oldest project result when the cache reaches its bound', () => {
    for (let index = 0; index <= ANALYSIS_MEMORY_CACHE_MAX_ENTRIES; index += 1) {
      useAnalysisStore.getState().setResult(`project-${index}`, `cache-${index}`, result);
    }

    expect(Object.keys(useAnalysisStore.getState().results)).toHaveLength(
      ANALYSIS_MEMORY_CACHE_MAX_ENTRIES
    );
    expect(useAnalysisStore.getState().getResult('project-0', 'cache-0')).toBeNull();
    expect(
      useAnalysisStore
        .getState()
        .getResult(
          `project-${ANALYSIS_MEMORY_CACHE_MAX_ENTRIES}`,
          `cache-${ANALYSIS_MEMORY_CACHE_MAX_ENTRIES}`
        )
    ).toBe(result);
  });

  it('restores an exact hit after a project-level miss without clearing same-project misses', () => {
    const keyA = { projectId: 'project-1', cacheKey: 'key-a' };
    const keyB = { projectId: 'project-1', cacheKey: 'key-b' };
    const otherProject = { projectId: 'project-2', cacheKey: 'key-x' };
    const clearedProject = { projectId: 'project-1', cacheKey: null };

    expect(getAnalysisCacheRestoreDecision(otherProject, keyB, null)).toEqual({
      shouldSetResult: true,
      result: null,
    });
    expect(getAnalysisCacheRestoreDecision(keyB, keyA, result)).toEqual({
      shouldSetResult: true,
      result,
    });
    expect(getAnalysisCacheRestoreDecision(keyA, keyB, null)).toEqual({
      shouldSetResult: false,
      result: null,
    });
    expect(getAnalysisCacheRestoreDecision(clearedProject, keyB, null)).toEqual({
      shouldSetResult: false,
      result: null,
    });
    expect(getAnalysisCacheRestoreDecision(clearedProject, keyA, result)).toEqual({
      shouldSetResult: true,
      result,
    });
  });

  it('bounds proactive key hashing without changing canonical run-time keys', () => {
    const overLimit = {
      ...baseInput,
      files: [
        {
          name: '',
          content: 'x'.repeat(PROACTIVE_ANALYSIS_CACHE_KEY_MAX_CHARS + 1),
        },
      ],
    };

    expect(canBuildProactiveAnalysisCacheKey(baseInput)).toBe(true);
    expect(canBuildProactiveAnalysisCacheKey(overLimit)).toBe(false);
    expect(buildAnalysisCacheKey(overLimit)).not.toBe(buildAnalysisCacheKey(baseInput));
  });
});
