import { isDesktopRuntime } from '../services/platform/runtime';

export async function openExternalUrl(url: string) {
  const normalizedUrl = normalizeExternalUrl(url);

  if (!normalizedUrl) {
    return;
  }

  try {
    if (isDesktopRuntime()) {
      const { openUrl } = await import('@tauri-apps/plugin-opener');
      await openUrl(normalizedUrl);
      return;
    }
  } catch {
    // Fall through to the browser opener.
  }

  window.open(normalizedUrl, '_blank', 'noopener,noreferrer');
}

function normalizeExternalUrl(url: string) {
  const trimmedUrl = url.trim();

  if (!trimmedUrl) {
    return null;
  }

  try {
    const parsedUrl = new URL(trimmedUrl);

    if (parsedUrl.protocol !== 'https:' && parsedUrl.protocol !== 'http:') {
      return null;
    }

    return parsedUrl.toString();
  } catch {
    return null;
  }
}
