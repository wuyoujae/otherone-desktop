export type WeixinClawbotStatus = {
  configured: boolean;
  running: boolean;
  status: string;
  botUserId: string;
  ilinkUserId: string;
  baseUrl: string;
  hasToken: boolean;
  loginExpiresAt: string | null;
  lastConnectedAt: string | null;
  lastPollAt: string | null;
  lastError: string;
};

export type WeixinLoginQr = {
  qrcode: string;
  qrcodeImgContent: string;
  baseUrl: string;
  status: string;
};

export type WeixinLoginCheckResponse = {
  status: string;
  message: string;
  baseUrl: string;
  confirmed: boolean;
  verifyCodeRequired: boolean;
  expired: boolean;
};

export type WeixinClawbotEvent = {
  id: string;
  direction: string;
  fromUserId: string;
  summary: string;
  status: string;
  error: string;
  createdAt: string;
};
