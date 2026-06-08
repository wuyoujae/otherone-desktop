import { openUrl } from '@tauri-apps/plugin-opener';

export async function openExternalUrl(url: string) {
  const normalizedUrl = normalizeExternalUrl(url);

  if (!normalizedUrl) {
    return;
  }

  try {
    await openUrl(normalizedUrl);
  } catch {
    window.open(normalizedUrl, '_blank', 'noopener,noreferrer');
  }
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
