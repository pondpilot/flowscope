import { useState, useCallback, useEffect, useRef } from 'react';
import { analyzeSql } from '@pondpilot/flowscope-core';
import { useLineage } from '@pondpilot/flowscope-react';
import { useProject } from '@/lib/project-store';
import { useAnalysisStore } from '@/lib/analysis-store';
import { FILE_LIMITS } from '@/lib/constants';
import { parseSchemaSQL } from '@/lib/schema-parser';
import type { AnalysisState, AnalysisContext, FileValidationResult } from '@/types';

export function useAnalysis(wasmReady: boolean) {
  const { currentProject, activeProjectId } = useProject();
  const { actions } = useLineage();
  const { getResult, setResult: storeResult } = useAnalysisStore();
  const [state, setState] = useState<AnalysisState>({
    isAnalyzing: false,
    error: null,
    lastAnalyzedAt: null,
  });

  // Use ref for actions to avoid dependency issues (actions object changes every render)
  const actionsRef = useRef(actions);
  useEffect(() => {
    actionsRef.current = actions;
  }, [actions]);

  // Restore cached analysis result when project changes
  useEffect(() => {
    if (!activeProjectId) {
      actionsRef.current.setResult(null);
      return;
    }

    const cachedResult = getResult(activeProjectId);
    actionsRef.current.setResult(cachedResult);
  }, [activeProjectId, getResult]);

  const setAnalyzing = useCallback((isAnalyzing: boolean) => {
    setState(prev => ({ ...prev, isAnalyzing }));
  }, []);

  const setError = useCallback((error: string | null) => {
    setState(prev => ({ ...prev, error }));
  }, []);

  const validateFiles = useCallback(
    (files: Array<{ name: string; content: string }>): FileValidationResult => {
      if (files.length === 0) {
        return { valid: false, error: 'No files to analyze' };
      }

      if (files.length > FILE_LIMITS.MAX_COUNT) {
        return {
          valid: false,
          error: `Too many files selected (max ${FILE_LIMITS.MAX_COUNT}). Currently selected: ${files.length} files.`,
        };
      }

      for (const file of files) {
        if (file.content.length > FILE_LIMITS.MAX_SIZE) {
          return {
            valid: false,
            error: `File "${file.name}" is too large (max ${FILE_LIMITS.MAX_SIZE / 1024 / 1024}MB). File size: ${(file.content.length / 1024 / 1024).toFixed(2)}MB.`,
          };
        }
      }

      return { valid: true };
    },
    []
  );

  const buildAnalysisContext = useCallback(
    (activeFileContent?: string, activeFileName?: string): AnalysisContext | null => {
      if (!currentProject) return null;

      let contextDescription = '';
      let filesToAnalyze: Array<{ name: string; content: string }> = [];
      const runMode = currentProject.runMode;

      if (runMode === 'current' && activeFileContent && activeFileName) {
        filesToAnalyze = [{ name: activeFileName, content: activeFileContent }];
        contextDescription = `Analyzing file: ${activeFileName}`;
      } else if (runMode === 'custom') {
        const selectedIds = currentProject.selectedFileIds || [];
        const selectedFiles = currentProject.files.filter(
          f => selectedIds.includes(f.id) && f.name.endsWith('.sql')
        );
        filesToAnalyze = selectedFiles.map(f => ({ name: f.name, content: f.content }));
        contextDescription = `Analyzing selected: ${filesToAnalyze.length} files`;
      } else {
        const sqlFiles = currentProject.files.filter(f => f.name.endsWith('.sql'));
        filesToAnalyze = sqlFiles.map(f => ({ name: f.name, content: f.content }));
        contextDescription = `Analyzing project: ${sqlFiles.length} files`;
      }

      return {
        description: contextDescription,
        fileCount: filesToAnalyze.length,
        files: filesToAnalyze,
      };
    },
    [currentProject]
  );

  const runAnalysis = useCallback(
    async (activeFileContent?: string, activeFileName?: string) => {
      if (!wasmReady || !currentProject) return;

      setAnalyzing(true);
      setError(null);

      try {
        const context = buildAnalysisContext(activeFileContent, activeFileName);

        if (!context) {
          setError('No project context available');
          return;
        }

        if (context.files.length === 0) {
          if (currentProject.runMode === 'custom') {
            setError('No files selected for analysis.');
            return;
          }
          if (currentProject.files.length > 0) {
            setError('No .sql files found in project.');
            return;
          }
          return;
        }

        const validation = validateFiles(context.files);
        if (!validation.valid) {
          setError(validation.error || 'Validation failed');
          return;
        }

        console.log(context.description);

        const representativeSql = context.files
          .map(f => `-- File: ${f.name}\n${f.content}`)
          .join('\n\n');

        actionsRef.current.setSql(representativeSql);

        // Parse schema SQL to extract imported schema tables
        let importedSchema = undefined;
        const schemaParsingErrors: string[] = [];
        if (currentProject.schemaSQL && currentProject.schemaSQL.trim()) {
          const { tables, errors } = await parseSchemaSQL(
            currentProject.schemaSQL,
            currentProject.dialect,
            analyzeSql
          );

          if (errors.length > 0) {
            console.warn('Schema SQL parsing errors:', errors);
            schemaParsingErrors.push(...errors);
          }

          if (tables.length > 0) {
            importedSchema = {
              allowImplied: true, // Still capture implied schema from DDL in queries
              tables,
            };
          }
        }

        const result = await analyzeSql({
          sql: '',
          files: context.files,
          dialect: currentProject.dialect,
          schema: importedSchema,
          options: {
            enableColumnLineage: true,
          },
        });

        // Inject schema parsing errors as issues in the result
        if (schemaParsingErrors.length > 0) {
          const schemaIssues = schemaParsingErrors.map((errorMsg) => ({
            severity: 'warning' as const,
            code: 'SCHEMA_PARSE_ERROR',
            message: `Schema DDL: ${errorMsg}`,
            locations: [],
          }));

          result.issues = [...(result.issues || []), ...schemaIssues];
        }

        actionsRef.current.setResult(result);
        // Store result per project for restoration when switching projects
        if (activeProjectId) {
          storeResult(activeProjectId, result);
        }
        setState(prev => ({ ...prev, lastAnalyzedAt: Date.now() }));
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Analysis failed');
        console.error(err);
      } finally {
        setAnalyzing(false);
      }
    },
    [
      wasmReady,
      currentProject,
      activeProjectId,
      storeResult,
      buildAnalysisContext,
      validateFiles,
      setAnalyzing,
      setError,
    ]
  );

  return {
    ...state,
    runAnalysis,
    setError,
  };
}
