import { expect } from 'vitest';
import * as matchers from '@testing-library/jest-dom/matchers';

expect.extend(matchers);

// Node 22+ exposes an experimental global localStorage object in some
// configurations. It is not the jsdom Storage implementation and may be
// missing clear/setItem, which breaks tests and Zustand persist. Install a
// deterministic in-memory Storage-compatible shim before modules import stores.
class MemoryStorage implements Storage {
  private values = new Map<string, string>();

  get length(): number {
    return this.values.size;
  }

  clear(): void {
    this.values.clear();
  }

  getItem(key: string): string | null {
    return this.values.has(key) ? (this.values.get(key) ?? null) : null;
  }

  key(index: number): string | null {
    return Array.from(this.values.keys())[index] ?? null;
  }

  removeItem(key: string): void {
    this.values.delete(key);
  }

  setItem(key: string, value: string): void {
    this.values.set(key, String(value));
  }
}

const localStorageShim = new MemoryStorage();
Object.defineProperty(globalThis, 'localStorage', {
  value: localStorageShim,
  configurable: true,
});
Object.defineProperty(window, 'localStorage', {
  value: localStorageShim,
  configurable: true,
});

// jsdom doesn't implement scrollIntoView
Element.prototype.scrollIntoView = () => {};
