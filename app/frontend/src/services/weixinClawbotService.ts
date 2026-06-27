import type {
  WeixinClawbotEvent,
  WeixinClawbotStatus,
  WeixinLoginCheckResponse,
  WeixinLoginQr,
} from '../types/weixinClawbot';

const isTauriRuntime = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

const defaultStatus: WeixinClawbotStatus = {
  configured: false,
  running: false,
  status: 'not_configured',
  botUserId: '',
  ilinkUserId: '',
  baseUrl: 'https://ilinkai.weixin.qq.com',
  hasToken: false,
  loginExpiresAt: null,
  lastConnectedAt: null,
  lastPollAt: null,
  lastError: '',
};

export async function loadWeixinClawbotStatus(): Promise<WeixinClawbotStatus> {
  if (!isTauriRuntime()) return defaultStatus;
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<WeixinClawbotStatus>('weixin_clawbot_status');
}

export async function beginWeixinClawbotLogin(): Promise<WeixinLoginQr | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<WeixinLoginQr>('weixin_clawbot_begin_login');
}

export async function checkWeixinClawbotLogin(
  qrcode: string,
  baseUrl: string,
  verifyCode?: string,
): Promise<WeixinLoginCheckResponse | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<WeixinLoginCheckResponse>('weixin_clawbot_check_login', {
    request: {
      qrcode,
      baseUrl,
      verifyCode: verifyCode?.trim() || null,
    },
  });
}

export async function startWeixinClawbot(): Promise<WeixinClawbotStatus> {
  if (!isTauriRuntime()) return defaultStatus;
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<WeixinClawbotStatus>('weixin_clawbot_start');
}

export async function stopWeixinClawbot(): Promise<WeixinClawbotStatus> {
  if (!isTauriRuntime()) return defaultStatus;
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<WeixinClawbotStatus>('weixin_clawbot_stop');
}

export async function resetWeixinClawbot(): Promise<WeixinClawbotStatus> {
  if (!isTauriRuntime()) return defaultStatus;
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<WeixinClawbotStatus>('weixin_clawbot_reset');
}

export async function loadWeixinClawbotEvents(): Promise<WeixinClawbotEvent[]> {
  if (!isTauriRuntime()) return [];
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<WeixinClawbotEvent[]>('weixin_clawbot_list_events');
}
