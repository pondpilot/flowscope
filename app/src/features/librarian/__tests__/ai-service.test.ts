import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  STORAGE_KEY_AI_API_KEY,
  STORAGE_KEY_AI_ENDPOINT,
  STORAGE_KEY_AI_MODEL,
  STORAGE_KEY_AI_PROVIDER,
  STORAGE_KEY_AI_SYSTEM_PROMPT,
} from '../constants';
import { DEFAULT_LIBRARIAN_SYSTEM_PROMPT } from '../services/context-builder';
import {
  type AIConfig,
  getDefaultModel,
  loadAIConfig,
  saveAIConfig,
  sendChatMessage,
} from '../services/ai-service';

describe('ai-service', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('getDefaultModel', () => {
    it('returns gpt-4o for openai', () => {
      expect(getDefaultModel('openai')).toBe('gpt-4o');
    });

    it('returns claude model for anthropic', () => {
      expect(getDefaultModel('anthropic')).toBe('claude-sonnet-4-20250514');
    });
  });

  describe('loadAIConfig', () => {
    it('returns null when no config is stored', () => {
      expect(loadAIConfig()).toBeNull();
    });

    it('returns null when provider is missing', () => {
      localStorage.setItem(STORAGE_KEY_AI_API_KEY, 'sk-test');
      expect(loadAIConfig()).toBeNull();
    });

    it('returns null when api key is missing', () => {
      localStorage.setItem(STORAGE_KEY_AI_PROVIDER, 'openai');
      expect(loadAIConfig()).toBeNull();
    });

    it('loads config with default model when model is not stored', () => {
      localStorage.setItem(STORAGE_KEY_AI_PROVIDER, 'openai');
      localStorage.setItem(STORAGE_KEY_AI_API_KEY, 'sk-test');

      const config = loadAIConfig();
      expect(config).toEqual({
        provider: 'openai',
        apiKey: 'sk-test',
        model: 'gpt-4o',
      });
    });

    it('loads config with stored model', () => {
      localStorage.setItem(STORAGE_KEY_AI_PROVIDER, 'anthropic');
      localStorage.setItem(STORAGE_KEY_AI_API_KEY, 'sk-ant-test');
      localStorage.setItem(STORAGE_KEY_AI_MODEL, 'claude-3-haiku');

      const config = loadAIConfig();
      expect(config).toEqual({
        provider: 'anthropic',
        apiKey: 'sk-ant-test',
        model: 'claude-3-haiku',
      });
    });

    it('loads config with stored system prompt override', () => {
      localStorage.setItem(STORAGE_KEY_AI_PROVIDER, 'openai');
      localStorage.setItem(STORAGE_KEY_AI_API_KEY, 'sk-test');
      localStorage.setItem(STORAGE_KEY_AI_SYSTEM_PROMPT, 'Custom prompt');

      expect(loadAIConfig()).toEqual({
        provider: 'openai',
        apiKey: 'sk-test',
        model: 'gpt-4o',
        systemPrompt: 'Custom prompt',
      });
    });

    it('returns null if localStorage throws', () => {
      vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
        throw new Error('localStorage disabled');
      });
      expect(loadAIConfig()).toBeNull();
    });
  });

  describe('saveAIConfig', () => {
    it('saves all config fields to localStorage', () => {
      const config: AIConfig = {
        provider: 'openai',
        apiKey: 'sk-123',
        model: 'gpt-4o-mini',
      };

      saveAIConfig(config);

      expect(localStorage.getItem(STORAGE_KEY_AI_PROVIDER)).toBe('openai');
      expect(localStorage.getItem(STORAGE_KEY_AI_API_KEY)).toBe('sk-123');
      expect(localStorage.getItem(STORAGE_KEY_AI_MODEL)).toBe('gpt-4o-mini');
    });

    it('roundtrips through load', () => {
      const config: AIConfig = {
        provider: 'anthropic',
        apiKey: 'sk-ant-456',
        model: 'claude-sonnet-4-20250514',
      };

      saveAIConfig(config);
      expect(loadAIConfig()).toEqual(config);
    });

    it('saves and loads custom provider config with apiEndpoint', () => {
      const config: AIConfig = {
        provider: 'custom',
        apiKey: 'my-key',
        model: 'my-model',
        apiEndpoint: 'https://litellm.example.com',
      };

      saveAIConfig(config);

      expect(localStorage.getItem(STORAGE_KEY_AI_PROVIDER)).toBe('custom');
      expect(localStorage.getItem(STORAGE_KEY_AI_ENDPOINT)).toBe('https://litellm.example.com');

      const loaded = loadAIConfig();
      expect(loaded).toEqual(config);
    });

    it('removes apiEndpoint from localStorage when not provided', () => {
      localStorage.setItem(STORAGE_KEY_AI_ENDPOINT, 'https://old-endpoint.com');

      saveAIConfig({
        provider: 'openai',
        apiKey: 'sk-123',
        model: 'gpt-4o',
      });

      expect(localStorage.getItem(STORAGE_KEY_AI_ENDPOINT)).toBeNull();
    });

    it('saves a custom system prompt override', () => {
      saveAIConfig({
        provider: 'openai',
        apiKey: 'sk-123',
        model: 'gpt-4o',
        systemPrompt: 'Custom prompt',
      });

      expect(localStorage.getItem(STORAGE_KEY_AI_SYSTEM_PROMPT)).toBe('Custom prompt');
      expect(loadAIConfig()?.systemPrompt).toBe('Custom prompt');
    });

    it('removes system prompt override when saving the default prompt', () => {
      localStorage.setItem(STORAGE_KEY_AI_SYSTEM_PROMPT, 'Old prompt');

      saveAIConfig({
        provider: 'openai',
        apiKey: 'sk-123',
        model: 'gpt-4o',
        systemPrompt: DEFAULT_LIBRARIAN_SYSTEM_PROMPT,
      });

      expect(localStorage.getItem(STORAGE_KEY_AI_SYSTEM_PROMPT)).toBeNull();
    });

    it('preserves trailing whitespace in the persisted system prompt', () => {
      const promptWithWhitespace = 'Custom prompt\n\n';

      saveAIConfig({
        provider: 'openai',
        apiKey: 'sk-123',
        model: 'gpt-4o',
        systemPrompt: promptWithWhitespace,
      });

      expect(localStorage.getItem(STORAGE_KEY_AI_SYSTEM_PROMPT)).toBe(promptWithWhitespace);
      expect(loadAIConfig()?.systemPrompt).toBe(promptWithWhitespace);
    });

    it('treats a whitespace-only system prompt as no override', () => {
      localStorage.setItem(STORAGE_KEY_AI_SYSTEM_PROMPT, 'Old prompt');

      saveAIConfig({
        provider: 'openai',
        apiKey: 'sk-123',
        model: 'gpt-4o',
        systemPrompt: '   \n\t',
      });

      expect(localStorage.getItem(STORAGE_KEY_AI_SYSTEM_PROMPT)).toBeNull();
    });

    it('does not persist the default prompt as an override when padded with whitespace', () => {
      localStorage.setItem(STORAGE_KEY_AI_SYSTEM_PROMPT, 'Old prompt');

      saveAIConfig({
        provider: 'openai',
        apiKey: 'sk-123',
        model: 'gpt-4o',
        systemPrompt: `  ${DEFAULT_LIBRARIAN_SYSTEM_PROMPT}\n`,
      });

      expect(localStorage.getItem(STORAGE_KEY_AI_SYSTEM_PROMPT)).toBeNull();
    });
  });

  describe('sendChatMessage', () => {
    const openaiConfig: AIConfig = {
      provider: 'openai',
      apiKey: 'sk-test',
      model: 'gpt-4o',
    };

    const anthropicConfig: AIConfig = {
      provider: 'anthropic',
      apiKey: 'sk-ant-test',
      model: 'claude-sonnet-4-20250514',
    };

    it('sends correct request to OpenAI', async () => {
      const mockFetch = vi.fn().mockResolvedValue({
        ok: true,
        json: () =>
          Promise.resolve({
            choices: [{ message: { content: 'Hello from OpenAI' } }],
          }),
      });
      vi.stubGlobal('fetch', mockFetch);

      const result = await sendChatMessage(openaiConfig, 'system prompt', 'user message');

      expect(result).toBe('Hello from OpenAI');
      expect(mockFetch).toHaveBeenCalledWith(
        'https://api.openai.com/v1/chat/completions',
        expect.objectContaining({
          method: 'POST',
          headers: expect.objectContaining({
            Authorization: 'Bearer sk-test',
          }),
        })
      );

      const body = JSON.parse(mockFetch.mock.calls[0][1].body);
      expect(body.model).toBe('gpt-4o');
      expect(body.messages).toEqual([
        { role: 'system', content: 'system prompt' },
        { role: 'user', content: 'user message' },
      ]);
    });

    it('sends correct request to Anthropic', async () => {
      const mockFetch = vi.fn().mockResolvedValue({
        ok: true,
        json: () =>
          Promise.resolve({
            content: [{ type: 'text', text: 'Hello from Anthropic' }],
          }),
      });
      vi.stubGlobal('fetch', mockFetch);

      const result = await sendChatMessage(anthropicConfig, 'system prompt', 'user message');

      expect(result).toBe('Hello from Anthropic');
      expect(mockFetch).toHaveBeenCalledWith(
        'https://api.anthropic.com/v1/messages',
        expect.objectContaining({
          method: 'POST',
          headers: expect.objectContaining({
            'x-api-key': 'sk-ant-test',
            'anthropic-version': '2023-06-01',
          }),
        })
      );

      const body = JSON.parse(mockFetch.mock.calls[0][1].body);
      expect(body.model).toBe('claude-sonnet-4-20250514');
      expect(body.system).toBe('system prompt');
      expect(body.messages).toEqual([{ role: 'user', content: 'user message' }]);
    });

    it('throws on OpenAI API error', async () => {
      vi.stubGlobal(
        'fetch',
        vi.fn().mockResolvedValue({
          ok: false,
          status: 401,
          text: () => Promise.resolve('Invalid API key'),
        })
      );

      await expect(sendChatMessage(openaiConfig, 'sys', 'msg')).rejects.toThrow(
        'OpenAI API error (401): Invalid API key'
      );
    });

    it('throws on Anthropic API error', async () => {
      vi.stubGlobal(
        'fetch',
        vi.fn().mockResolvedValue({
          ok: false,
          status: 429,
          text: () => Promise.resolve('Rate limited'),
        })
      );

      await expect(sendChatMessage(anthropicConfig, 'sys', 'msg')).rejects.toThrow(
        'Anthropic API error (429): Rate limited'
      );
    });

    it('passes abort signal to fetch', async () => {
      const controller = new AbortController();
      const mockFetch = vi.fn().mockResolvedValue({
        ok: true,
        json: () =>
          Promise.resolve({
            choices: [{ message: { content: 'ok' } }],
          }),
      });
      vi.stubGlobal('fetch', mockFetch);

      await sendChatMessage(openaiConfig, 'sys', 'msg', controller.signal);

      expect(mockFetch.mock.calls[0][1].signal).toBe(controller.signal);
    });

    it('throws when OpenAI response has no content', async () => {
      vi.stubGlobal(
        'fetch',
        vi.fn().mockResolvedValue({
          ok: true,
          json: () => Promise.resolve({ choices: [{ message: {} }] }),
        })
      );

      await expect(sendChatMessage(openaiConfig, 'sys', 'msg')).rejects.toThrow(
        'OpenAI API returned no text content.'
      );
    });

    it('throws when Anthropic response has no text block', async () => {
      vi.stubGlobal(
        'fetch',
        vi.fn().mockResolvedValue({
          ok: true,
          json: () => Promise.resolve({ content: [] }),
        })
      );

      await expect(sendChatMessage(anthropicConfig, 'sys', 'msg')).rejects.toThrow(
        'Anthropic API returned no text content.'
      );
    });

    it('routes custom provider to OpenAI format with custom endpoint URL', async () => {
      const customConfig: AIConfig = {
        provider: 'custom',
        apiKey: 'custom-key',
        model: 'my-local-model',
        apiEndpoint: 'https://litellm.example.com',
      };

      const mockFetch = vi.fn().mockResolvedValue({
        ok: true,
        json: () =>
          Promise.resolve({
            choices: [{ message: { content: 'Hello from custom' } }],
          }),
      });
      vi.stubGlobal('fetch', mockFetch);

      const result = await sendChatMessage(customConfig, 'system prompt', 'user message');

      expect(result).toBe('Hello from custom');
      expect(mockFetch).toHaveBeenCalledWith(
        'https://litellm.example.com/v1/chat/completions',
        expect.objectContaining({
          method: 'POST',
          headers: expect.objectContaining({
            Authorization: 'Bearer custom-key',
          }),
        })
      );

      const body = JSON.parse(mockFetch.mock.calls[0][1].body);
      expect(body.model).toBe('my-local-model');
    });

    it('strips trailing slashes from custom endpoint URL', async () => {
      const customConfig: AIConfig = {
        provider: 'custom',
        apiKey: 'key',
        model: 'model',
        apiEndpoint: 'https://example.com/',
      };

      const mockFetch = vi.fn().mockResolvedValue({
        ok: true,
        json: () =>
          Promise.resolve({
            choices: [{ message: { content: 'ok' } }],
          }),
      });
      vi.stubGlobal('fetch', mockFetch);

      await sendChatMessage(customConfig, 'sys', 'msg');

      expect(mockFetch.mock.calls[0][0]).toBe('https://example.com/v1/chat/completions');
    });

    it('throws custom endpoint error label on failure', async () => {
      const customConfig: AIConfig = {
        provider: 'custom',
        apiKey: 'key',
        model: 'model',
        apiEndpoint: 'https://bad.example.com',
      };

      vi.stubGlobal(
        'fetch',
        vi.fn().mockResolvedValue({
          ok: false,
          status: 500,
          text: () => Promise.resolve('Internal error'),
        })
      );

      await expect(sendChatMessage(customConfig, 'sys', 'msg')).rejects.toThrow(
        'Custom endpoint API error (500): Internal error'
      );
    });

    it('throws when custom provider has no apiEndpoint', async () => {
      const configWithoutEndpoint: AIConfig = {
        provider: 'custom',
        apiKey: 'sk-custom',
        model: 'my-model',
      };

      await expect(sendChatMessage(configWithoutEndpoint, 'sys', 'msg')).rejects.toThrow(
        'Custom provider requires a base URL'
      );
    });
  });
});
