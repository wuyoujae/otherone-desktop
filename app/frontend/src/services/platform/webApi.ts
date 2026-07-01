import { createPlatformUnavailableError, getWebApiBaseUrl, hasWebApiBaseUrl } from './runtime';

type QueryValue = string | number | boolean | null | undefined;

type WebApiRequestOptions = {
  method?: string;
  query?: Record<string, QueryValue>;
  body?: unknown;
  headers?: Record<string, string>;
  signal?: AbortSignal;
};

export function canUseWebApi() {
  return hasWebApiBaseUrl();
}

export async function requestWebApi<T>(path: string, options: WebApiRequestOptions = {}) {
  const baseUrl = getWebApiBaseUrl();

  if (!baseUrl) {
    throw createPlatformUnavailableError('Web API');
  }

  const hasBody = options.body !== undefined;
  const response = await fetch(buildWebApiUrl(baseUrl, path, options.query), {
    method: options.method ?? 'GET',
    headers: {
      Accept: 'application/json',
      ...(hasBody ? { 'Content-Type': 'application/json' } : {}),
      ...options.headers,
    },
    body: hasBody ? JSON.stringify(options.body) : undefined,
    signal: options.signal,
  });

  const text = await response.text();

  if (!response.ok) {
    throw new Error(readWebApiErrorMessage(text, response.status));
  }

  if (!text || response.status === 204) {
    return undefined as T;
  }

  return JSON.parse(text) as T;
}

export function listenWebApiEventStream<T>(
  path: string,
  onEvent: (event: T) => void,
  query?: Record<string, QueryValue>,
) {
  const baseUrl = getWebApiBaseUrl();

  if (!baseUrl || typeof EventSource === 'undefined') {
    return () => undefined;
  }

  const source = new EventSource(buildWebApiUrl(baseUrl, path, query));

  source.onmessage = (event) => {
    if (!event.data) {
      return;
    }

    onEvent(JSON.parse(event.data) as T);
  };

  return () => source.close();
}

function buildWebApiUrl(baseUrl: string, path: string, query?: Record<string, QueryValue>) {
  const normalizedPath = path.startsWith('/') ? path : `/${path}`;
  const url = new URL(`${baseUrl}${normalizedPath}`);

  Object.entries(query ?? {}).forEach(([key, value]) => {
    if (value === null || value === undefined || value === '') {
      return;
    }

    url.searchParams.set(key, String(value));
  });

  return url.toString();
}

function readWebApiErrorMessage(text: string, status: number) {
  if (!text) {
    return `Web API 请求失败 (${status})`;
  }

  try {
    const payload = JSON.parse(text) as { message?: unknown; error?: unknown };
    const message = typeof payload.message === 'string' ? payload.message : payload.error;

    if (typeof message === 'string' && message.trim()) {
      return message;
    }
  } catch {
    return text;
  }

  return `Web API 请求失败 (${status})`;
}
