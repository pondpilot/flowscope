import {
  STORAGE_KEY_AI_API_KEY,
  STORAGE_KEY_AI_ENDPOINT,
  STORAGE_KEY_AI_MODEL,
  STORAGE_KEY_AI_PROVIDER,
  STORAGE_KEY_AI_SYSTEM_PROMPT,
} from '../constants';
import { DEFAULT_LIBRARIAN_SYSTEM_PROMPT } from './context-builder';

export type AIProvider = 'openai' | 'anthropic' | 'custom';

export interface AIConfig {
  provider: AIProvider;
  apiKey: string;
  model: string;
  apiEndpoint?: string;
  systemPrompt?: string;
}

const DEFAULT_MODELS: Record<AIProvider, string> = {
  openai: 'gpt-4o',
  anthropic: 'claude-sonnet-4-20250514',
  custom: '',
};

export function getDefaultModel(provider: AIProvider): string {
  return DEFAULT_MODELS[provider];
}

const VALID_PROVIDERS: AIProvider[] = ['openai', 'anthropic', 'custom'];

export function loadAIConfig(): AIConfig | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY_AI_PROVIDER);
    const provider = VALID_PROVIDERS.includes(raw as AIProvider) ? (raw as AIProvider) : null;
    const apiKey = localStorage.getItem(STORAGE_KEY_AI_API_KEY);

    if (!provider || !apiKey) return null;

    const model = localStorage.getItem(STORAGE_KEY_AI_MODEL) || getDefaultModel(provider);
    const apiEndpoint = localStorage.getItem(STORAGE_KEY_AI_ENDPOINT) || undefined;
    const systemPrompt = localStorage.getItem(STORAGE_KEY_AI_SYSTEM_PROMPT) || undefined;

    return {
      provider,
      apiKey,
      model,
      ...(apiEndpoint ? { apiEndpoint } : {}),
      ...(systemPrompt ? { systemPrompt } : {}),
    };
  } catch {
    return null;
  }
}

export function saveAIConfig(config: AIConfig): void {
  localStorage.setItem(STORAGE_KEY_AI_PROVIDER, config.provider);
  localStorage.setItem(STORAGE_KEY_AI_API_KEY, config.apiKey);
  localStorage.setItem(STORAGE_KEY_AI_MODEL, config.model);
  if (config.apiEndpoint) {
    localStorage.setItem(STORAGE_KEY_AI_ENDPOINT, config.apiEndpoint);
  } else {
    localStorage.removeItem(STORAGE_KEY_AI_ENDPOINT);
  }
  const raw = config.systemPrompt ?? '';
  const normalized = raw.trim();
  if (normalized && normalized !== DEFAULT_LIBRARIAN_SYSTEM_PROMPT.trim()) {
    localStorage.setItem(STORAGE_KEY_AI_SYSTEM_PROMPT, raw);
  } else {
    localStorage.removeItem(STORAGE_KEY_AI_SYSTEM_PROMPT);
  }
}

export async function sendChatMessage(
  config: AIConfig,
  systemPrompt: string,
  userMessage: string,
  signal?: AbortSignal
): Promise<string> {
  if (config.provider === 'openai') {
    return sendOpenAI(config, systemPrompt, userMessage, signal);
  }
  if (config.provider === 'custom') {
    if (!config.apiEndpoint) {
      throw new Error('Custom provider requires a base URL. Please update your AI settings.');
    }
    return sendOpenAI(config, systemPrompt, userMessage, signal, config.apiEndpoint);
  }
  return sendAnthropic(config, systemPrompt, userMessage, signal);
}

async function sendOpenAI(
  config: AIConfig,
  systemPrompt: string,
  userMessage: string,
  signal?: AbortSignal,
  endpointOverride?: string
): Promise<string> {
  const baseUrl =
    endpointOverride?.replace(/\/+$/, '').replace(/\/v1$/, '') ?? 'https://api.openai.com';
  const response = await fetch(`${baseUrl}/v1/chat/completions`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${config.apiKey}`,
    },
    body: JSON.stringify({
      model: config.model,
      max_tokens: 4096,
      messages: [
        { role: 'system', content: systemPrompt },
        { role: 'user', content: userMessage },
      ],
    }),
    signal,
  });

  if (!response.ok) {
    const errorBody = await response.text().catch(() => '');
    const label = endpointOverride ? 'Custom endpoint' : 'OpenAI';
    throw new Error(`${label} API error (${response.status}): ${errorBody}`);
  }

  const data = await response.json();
  const content = data.choices?.[0]?.message?.content;
  if (typeof content !== 'string' || content.length === 0) {
    throw new Error('OpenAI API returned no text content.');
  }
  return content;
}

async function sendAnthropic(
  config: AIConfig,
  systemPrompt: string,
  userMessage: string,
  signal?: AbortSignal
): Promise<string> {
  const response = await fetch('https://api.anthropic.com/v1/messages', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'x-api-key': config.apiKey,
      'anthropic-version': '2023-06-01',
      'anthropic-dangerous-direct-browser-access': 'true',
    },
    body: JSON.stringify({
      model: config.model,
      max_tokens: 4096,
      system: systemPrompt,
      messages: [{ role: 'user', content: userMessage }],
    }),
    signal,
  });

  if (!response.ok) {
    const errorBody = await response.text().catch(() => '');
    throw new Error(`Anthropic API error (${response.status}): ${errorBody}`);
  }

  const data = await response.json();
  const textBlock = data.content?.find(
    (block: { type: string; text?: unknown }) => block.type === 'text'
  );
  if (typeof textBlock?.text !== 'string' || textBlock.text.length === 0) {
    throw new Error('Anthropic API returned no text content.');
  }
  return textBlock.text;
}
