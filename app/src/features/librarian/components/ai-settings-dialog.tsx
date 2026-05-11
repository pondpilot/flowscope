import { useCallback, useEffect, useMemo, useState } from 'react';

import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';

import {
  type AIConfig,
  type AIProvider,
  getDefaultModel,
  loadAIConfig,
  saveAIConfig,
  sendChatMessage,
} from '../services/ai-service';
import { DEFAULT_LIBRARIAN_SYSTEM_PROMPT, getPromptStats } from '../services/context-builder';
import { useLibrarianStore } from '../store';

interface AISettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function AISettingsDialog({ open, onOpenChange }: AISettingsDialogProps) {
  const [provider, setProvider] = useState<AIProvider>('openai');
  const [apiKey, setApiKey] = useState('');
  const [model, setModel] = useState('');
  const [apiEndpoint, setApiEndpoint] = useState('');
  const [systemPrompt, setSystemPrompt] = useState(DEFAULT_LIBRARIAN_SYSTEM_PROMPT);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; message: string } | null>(null);

  useEffect(() => {
    if (open) {
      const config = loadAIConfig();
      if (config) {
        setProvider(config.provider);
        setApiKey(config.apiKey);
        setModel(config.model);
        setApiEndpoint(config.apiEndpoint ?? '');
        setSystemPrompt(config.systemPrompt ?? DEFAULT_LIBRARIAN_SYSTEM_PROMPT);
      } else {
        setProvider('openai');
        setApiKey('');
        setModel(getDefaultModel('openai'));
        setApiEndpoint('');
        setSystemPrompt(DEFAULT_LIBRARIAN_SYSTEM_PROMPT);
      }
      setTestResult(null);
    }
  }, [open]);

  const handleProviderChange = useCallback((value: string) => {
    const newProvider = value as AIProvider;
    setProvider(newProvider);
    if (newProvider === 'custom') {
      setModel('');
      setApiEndpoint('');
    } else {
      setModel(getDefaultModel(newProvider));
    }
  }, []);

  const refreshConfig = useLibrarianStore((s) => s.refreshConfig);

  const handleSave = useCallback(() => {
    const config: AIConfig = {
      provider,
      apiKey,
      model: provider === 'custom' ? model : model || getDefaultModel(provider),
      ...(provider === 'custom' && apiEndpoint ? { apiEndpoint } : {}),
      systemPrompt,
    };
    saveAIConfig(config);
    refreshConfig();
    onOpenChange(false);
  }, [provider, apiKey, model, apiEndpoint, systemPrompt, onOpenChange, refreshConfig]);

  const handleTestConnection = useCallback(async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const config: AIConfig = {
        provider,
        apiKey,
        model: provider === 'custom' ? model : model || getDefaultModel(provider),
        ...(provider === 'custom' && apiEndpoint ? { apiEndpoint } : {}),
      };
      await sendChatMessage(config, 'You are a test.', 'Say "ok" and nothing else.');
      setTestResult({ ok: true, message: 'Connection successful' });
    } catch (err) {
      setTestResult({
        ok: false,
        message: err instanceof Error ? err.message : 'Connection failed',
      });
    } finally {
      setTesting(false);
    }
  }, [provider, apiKey, model, apiEndpoint]);

  const promptStats = useMemo(() => getPromptStats(systemPrompt), [systemPrompt]);

  const canSave =
    apiKey.trim().length > 0 &&
    (provider !== 'custom' || (apiEndpoint.trim().length > 0 && model.trim().length > 0));

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>AI Settings</DialogTitle>
          <DialogDescription>Configure your AI provider for the Librarian.</DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-4">
          <div className="space-y-2">
            <Label htmlFor="ai-provider">Provider</Label>
            <Select value={provider} onValueChange={handleProviderChange}>
              <SelectTrigger id="ai-provider">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="openai">OpenAI</SelectItem>
                <SelectItem value="anthropic">Anthropic</SelectItem>
                <SelectItem value="custom">Custom (OpenAI-compatible)</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-2">
            <Label htmlFor="ai-api-key">API Key</Label>
            <Input
              id="ai-api-key"
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder={
                provider === 'openai'
                  ? 'sk-...'
                  : provider === 'anthropic'
                    ? 'sk-ant-...'
                    : 'API key'
              }
            />
          </div>

          {provider === 'custom' && (
            <div className="space-y-2">
              <Label htmlFor="ai-endpoint">Base URL</Label>
              <Input
                id="ai-endpoint"
                value={apiEndpoint}
                onChange={(e) => setApiEndpoint(e.target.value)}
                placeholder="https://your-server.com"
              />
            </div>
          )}

          <div className="space-y-2">
            <Label htmlFor="ai-model">Model{provider === 'custom' ? ' Name' : ''}</Label>
            <Input
              id="ai-model"
              value={model}
              onChange={(e) => setModel(e.target.value)}
              placeholder={provider === 'custom' ? 'model-name' : getDefaultModel(provider)}
            />
          </div>

          <div className="space-y-2">
            <div className="flex items-center justify-between gap-3">
              <Label htmlFor="ai-system-prompt">System Prompt</Label>
              <span className="shrink-0 text-xs text-muted-foreground" data-testid="prompt-size">
                {promptStats.characters.toLocaleString()} chars /{' '}
                {promptStats.bytes.toLocaleString()} bytes
              </span>
            </div>
            <textarea
              id="ai-system-prompt"
              className="min-h-[220px] w-full resize-y rounded-md border border-input bg-background px-3 py-2 font-mono text-xs leading-relaxed ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
              value={systemPrompt}
              onChange={(e) => setSystemPrompt(e.target.value)}
              data-testid="system-prompt-textarea"
            />
            <div className="flex justify-end">
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => setSystemPrompt(DEFAULT_LIBRARIAN_SYSTEM_PROMPT)}
              >
                Reset to default
              </Button>
            </div>
          </div>

          {testResult && (
            <p
              className={`text-sm ${testResult.ok ? 'text-green-600 dark:text-green-400' : 'text-red-600 dark:text-red-400'}`}
              data-testid="test-result"
            >
              {testResult.message}
            </p>
          )}
        </div>

        <DialogFooter className="gap-2 sm:gap-0">
          <Button variant="outline" onClick={handleTestConnection} disabled={!canSave || testing}>
            {testing ? 'Testing...' : 'Test Connection'}
          </Button>
          <Button onClick={handleSave} disabled={!canSave}>
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
