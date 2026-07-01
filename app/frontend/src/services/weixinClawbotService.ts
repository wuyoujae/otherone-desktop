import type {
  WeixinClawbotEvent,
  WeixinClawbotStatus,
  WeixinLoginCheckResponse,
  WeixinLoginQr,
} from '../types/weixinClawbot';
import { isDesktopRuntime } from './platform/runtime';
import { invokeDesktop } from './platform/tauri';

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
  if (isDesktopRuntime()) return invokeDesktop<WeixinClawbotStatus>('weixin_clawbot_status');
  return defaultStatus;
}

export async function beginWeixinClawbotLogin(): Promise<WeixinLoginQr | null> {
  if (isDesktopRuntime()) return invokeDesktop<WeixinLoginQr>('weixin_clawbot_begin_login');
  return null;
}

export async function checkWeixinClawbotLogin(
  qrcode: string,
  baseUrl: string,
  verifyCode?: string,
): Promise<WeixinLoginCheckResponse | null> {
  const request = {
    qrcode,
    baseUrl,
    verifyCode: verifyCode?.trim() || null,
  };

  if (isDesktopRuntime()) return invokeDesktop<WeixinLoginCheckResponse>('weixin_clawbot_check_login', { request });
  return null;
}

export async function startWeixinClawbot(): Promise<WeixinClawbotStatus> {
  if (isDesktopRuntime()) return invokeDesktop<WeixinClawbotStatus>('weixin_clawbot_start');
  return defaultStatus;
}

export async function stopWeixinClawbot(): Promise<WeixinClawbotStatus> {
  if (isDesktopRuntime()) return invokeDesktop<WeixinClawbotStatus>('weixin_clawbot_stop');
  return defaultStatus;
}

export async function resetWeixinClawbot(): Promise<WeixinClawbotStatus> {
  if (isDesktopRuntime()) return invokeDesktop<WeixinClawbotStatus>('weixin_clawbot_reset');
  return defaultStatus;
}

export async function loadWeixinClawbotEvents(): Promise<WeixinClawbotEvent[]> {
  if (isDesktopRuntime()) return invokeDesktop<WeixinClawbotEvent[]>('weixin_clawbot_list_events');
  return [];
}
