import type { AnalyzeResult } from '@pondpilot/flowscope-core';

const CACHE_DB_NAME = 'flowscope-analysis-cache';
const CACHE_DB_VERSION = 1;
const CACHE_STORE_NAME = 'analysis-results';

interface CacheEntry {
  key: string;
  resultJson: string;
  sizeBytes: number;
  createdAt: number;
  lastAccessedAt: number;
}

function isCacheSupported(): boolean {
  return typeof indexedDB !== 'undefined';
}

function requestToPromise<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error('IndexedDB request failed'));
  });
}

function transactionToPromise(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onerror = () =>
      reject(transaction.error ?? new Error('IndexedDB transaction failed'));
    transaction.onabort = () =>
      reject(transaction.error ?? new Error('IndexedDB transaction aborted'));
  });
}

async function openCacheDatabase(): Promise<IDBDatabase> {
  if (!isCacheSupported()) {
    throw new Error('IndexedDB not supported');
  }

  return new Promise((resolve, reject) => {
    const request = indexedDB.open(CACHE_DB_NAME, CACHE_DB_VERSION);

    request.onupgradeneeded = () => {
      const database = request.result;
      if (!database.objectStoreNames.contains(CACHE_STORE_NAME)) {
        const store = database.createObjectStore(CACHE_STORE_NAME, { keyPath: 'key' });
        store.createIndex('lastAccessedAt', 'lastAccessedAt');
      }
    };

    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error('Failed to open cache database'));
  });
}

function computeSizeBytes(value: string): number {
  return new TextEncoder().encode(value).length;
}

async function enforceCacheLimit(database: IDBDatabase, maxBytes: number): Promise<void> {
  const transaction = database.transaction(CACHE_STORE_NAME, 'readwrite');
  const store = transaction.objectStore(CACHE_STORE_NAME);
  const entries = await requestToPromise(store.getAll());

  let totalBytes = entries.reduce((sum, entry: CacheEntry) => sum + entry.sizeBytes, 0);
  if (totalBytes <= maxBytes) {
    await transactionToPromise(transaction);
    return;
  }

  entries.sort((left, right) => left.lastAccessedAt - right.lastAccessedAt);

  for (const entry of entries) {
    if (totalBytes <= maxBytes) {
      break;
    }
    store.delete(entry.key);
    totalBytes -= entry.sizeBytes;
  }

  await transactionToPromise(transaction);
}

export async function readCachedAnalysisResult(cacheKey: string): Promise<AnalyzeResult | null> {
  if (!isCacheSupported()) {
    return null;
  }

  const database = await openCacheDatabase();
  try {
    const transaction = database.transaction(CACHE_STORE_NAME, 'readwrite');
    const store = transaction.objectStore(CACHE_STORE_NAME);
    const entry = await requestToPromise(store.get(cacheKey));

    if (!entry) {
      await transactionToPromise(transaction);
      return null;
    }

    const updatedEntry: CacheEntry = {
      ...entry,
      lastAccessedAt: Date.now(),
    };

    store.put(updatedEntry);
    await transactionToPromise(transaction);

    try {
      return JSON.parse(updatedEntry.resultJson) as AnalyzeResult;
    } catch {
      await deleteCachedAnalysisResult(cacheKey);
      return null;
    }
  } finally {
    database.close();
  }
}

export async function writeCachedAnalysisResult(
  cacheKey: string,
  result: AnalyzeResult,
  maxBytes: number
): Promise<void> {
  if (!isCacheSupported()) {
    return;
  }

  const resultJson = JSON.stringify(result);
  const sizeBytes = computeSizeBytes(resultJson);

  if (sizeBytes > maxBytes) {
    return;
  }

  const database = await openCacheDatabase();
  try {
    const now = Date.now();
    const entry: CacheEntry = {
      key: cacheKey,
      resultJson,
      sizeBytes,
      createdAt: now,
      lastAccessedAt: now,
    };

    const transaction = database.transaction(CACHE_STORE_NAME, 'readwrite');
    const store = transaction.objectStore(CACHE_STORE_NAME);
    store.put(entry);
    await transactionToPromise(transaction);

    await enforceCacheLimit(database, maxBytes);
  } finally {
    database.close();
  }
}

export async function deleteCachedAnalysisResult(cacheKey: string): Promise<void> {
  if (!isCacheSupported()) {
    return;
  }

  const database = await openCacheDatabase();
  try {
    const transaction = database.transaction(CACHE_STORE_NAME, 'readwrite');
    const store = transaction.objectStore(CACHE_STORE_NAME);
    store.delete(cacheKey);
    await transactionToPromise(transaction);
  } finally {
    database.close();
  }
}

export async function clearAnalysisCache(): Promise<void> {
  if (!isCacheSupported()) {
    return;
  }

  const database = await openCacheDatabase();
  try {
    const transaction = database.transaction(CACHE_STORE_NAME, 'readwrite');
    const store = transaction.objectStore(CACHE_STORE_NAME);
    store.clear();
    await transactionToPromise(transaction);
  } finally {
    database.close();
  }
}
