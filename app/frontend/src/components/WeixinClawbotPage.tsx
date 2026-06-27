import {
  BotMessageSquare,
  CheckCircle2,
  CircleAlert,
  Clock3,
  Loader2,
  MessageSquareText,
  Pause,
  Play,
  QrCode,
  RefreshCw,
  RotateCcw,
  ShieldCheck,
  Smartphone,
  X,
} from 'lucide-react';
import QRCode from 'qrcode';
import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  beginWeixinClawbotLogin,
  checkWeixinClawbotLogin,
  loadWeixinClawbotEvents,
  loadWeixinClawbotStatus,
  startWeixinClawbot,
  stopWeixinClawbot,
} from '../services/weixinClawbotService';
import type {
  WeixinClawbotEvent,
  WeixinClawbotStatus,
  WeixinLoginQr,
} from '../types/weixinClawbot';

const iconSize = { width: 16, height: 16 };

type NoticeKind = 'success' | 'warn' | 'fail' | 'info';

type WeixinClawbotPageProps = {
  onClose: () => void;
  onNotice?: (type: NoticeKind, title: string, description?: string) => void;
};

type BusyAction = 'login' | 'check' | 'start' | 'stop' | 'refresh' | null;

const STATUS_LABELS: Record<string, string> = {
  not_configured: '未连接',
  login_pending: '待扫码',
  connected: '已连接',
  running: '运行中',
  stopped: '已停止',
  error: '异常',
  disconnected: '未连接',
};

function formatStatus(status: string): string {
  return STATUS_LABELS[status] || status || '未知';
}

function formatTime(value: string | null): string {
  if (!value) return '-';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function eventLabel(direction: string): string {
  if (direction === 'inbound') return '收到';
  if (direction === 'outbound') return '回复';
  return '系统';
}

function toBase64Utf8(value: string): string {
  const bytes = new TextEncoder().encode(value);
  let binary = '';
  bytes.forEach((byte) => {
    binary += String.fromCharCode(byte);
  });
  return window.btoa(binary);
}

function directQrImageSrc(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) return '';
  if (trimmed.startsWith('data:image/')) {
    return trimmed;
  }
  if (trimmed.startsWith('<svg')) {
    return `data:image/svg+xml;base64,${toBase64Utf8(trimmed)}`;
  }
  if (/^(iVBORw0KGgo|\/9j\/|R0lGOD|PHN2Z)/.test(trimmed)) {
    return `data:image/png;base64,${trimmed}`;
  }
  return '';
}

export function WeixinClawbotPage({ onClose, onNotice }: WeixinClawbotPageProps) {
  const [status, setStatus] = useState<WeixinClawbotStatus | null>(null);
  const [events, setEvents] = useState<WeixinClawbotEvent[]>([]);
  const [qr, setQr] = useState<WeixinLoginQr | null>(null);
  const [verifyCode, setVerifyCode] = useState('');
  const [busy, setBusy] = useState<BusyAction>(null);
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState('');
  const [loginMessage, setLoginMessage] = useState('');
  const [qrImageDataUrl, setQrImageDataUrl] = useState('');
  const [qrRenderError, setQrRenderError] = useState('');

  const running = status?.running === true;
  const configured = status?.configured === true;
  const qrDisplayContent = useMemo(() => (qr ? qr.qrcodeImgContent.trim() : ''), [qr]);

  useEffect(() => {
    let cancelled = false;
    setQrImageDataUrl('');
    setQrRenderError('');

    if (!qr) return undefined;

    const direct = directQrImageSrc(qrDisplayContent);
    if (direct) {
      setQrImageDataUrl(direct);
      return undefined;
    }

    const payload = qrDisplayContent;
    if (!payload) {
      setQrRenderError('iLink 没有返回可扫码内容，请重新生成。');
      return undefined;
    }

    void QRCode.toDataURL(payload, {
      errorCorrectionLevel: 'M',
      margin: 4,
      width: 224,
      color: {
        dark: '#111827',
        light: '#ffffff',
      },
    }).then((dataUrl) => {
      if (!cancelled) setQrImageDataUrl(dataUrl);
    }).catch((err) => {
      if (!cancelled) {
        setQrRenderError(err instanceof Error ? err.message : String(err));
      }
    });

    return () => {
      cancelled = true;
    };
  }, [qr, qrDisplayContent]);

  const refresh = useCallback(async (silent = false) => {
    if (!silent) setBusy('refresh');
    try {
      setError('');
      const [nextStatus, nextEvents] = await Promise.all([
        loadWeixinClawbotStatus(),
        loadWeixinClawbotEvents(),
      ]);
      setStatus(nextStatus);
      setEvents(nextEvents);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      if (!silent) onNotice?.('fail', '刷新微信 ClawBot 失败', message);
    } finally {
      if (!silent) setBusy(null);
    }
  }, [onNotice]);

  useEffect(() => {
    void refresh(true);
    const timer = window.setInterval(() => {
      void refresh(true);
    }, 5000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  const handleBeginLogin = useCallback(async () => {
    setBusy('login');
    try {
      setError('');
      setLoginMessage('');
      const nextQr = await beginWeixinClawbotLogin();
      if (!nextQr) {
        setError('当前不是 Tauri 运行环境，无法连接微信 ClawBot。');
        return;
      }
      setQr(nextQr);
      setVerifyCode('');
      await refresh(true);
      onNotice?.('info', '已生成微信 ClawBot 二维码');
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      onNotice?.('fail', '生成二维码失败', message);
    } finally {
      setBusy(null);
    }
  }, [onNotice, refresh]);

  const handleCheckLogin = useCallback(async (silent = false) => {
    if (!qr || checking) return;
    if (!silent) setBusy('check');
    setChecking(true);
    try {
      setError('');
      const result = await checkWeixinClawbotLogin(qr.qrcode, qr.baseUrl, verifyCode);
      if (!result) return;

      setLoginMessage(result.message);
      setQr((current) => current ? { ...current, baseUrl: result.baseUrl } : current);

      if (result.confirmed) {
        setQr(null);
        setVerifyCode('');
        onNotice?.('success', '微信 ClawBot 已连接');
      } else if (result.expired) {
        onNotice?.('warn', '二维码已过期', result.message);
      } else if (!silent) {
        onNotice?.('info', '微信 ClawBot 登录状态', result.message);
      }

      await refresh(true);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      if (!silent) onNotice?.('fail', '检查登录状态失败', message);
    } finally {
      setChecking(false);
      if (!silent) setBusy(null);
    }
  }, [checking, onNotice, qr, refresh, verifyCode]);

  useEffect(() => {
    if (!qr) return undefined;
    const timer = window.setInterval(() => {
      void handleCheckLogin(true);
    }, 2500);
    return () => window.clearInterval(timer);
  }, [handleCheckLogin, qr]);

  const handleStart = useCallback(async () => {
    setBusy('start');
    try {
      setError('');
      setStatus(await startWeixinClawbot());
      await refresh(true);
      onNotice?.('success', '微信 ClawBot 已启动');
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      onNotice?.('fail', '启动微信 ClawBot 失败', message);
    } finally {
      setBusy(null);
    }
  }, [onNotice, refresh]);

  const handleStop = useCallback(async () => {
    setBusy('stop');
    try {
      setError('');
      setStatus(await stopWeixinClawbot());
      await refresh(true);
      onNotice?.('info', '微信 ClawBot 已停止');
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      onNotice?.('fail', '停止微信 ClawBot 失败', message);
    } finally {
      setBusy(null);
    }
  }, [onNotice, refresh]);

  return (
    <div className="weixin-page">
      <div className="weixin-nav">
        <div className="weixin-nav-left">
          <div className="weixin-title-mark">
            <BotMessageSquare style={{ width: 18, height: 18 }} />
            <span>微信 ClawBot</span>
          </div>
          <span className={`weixin-runtime-pill ${running ? 'is-running' : ''}`}>
            {running ? '运行中' : formatStatus(status?.status || 'not_configured')}
          </span>
        </div>

        <div className="weixin-nav-right">
          <button
            className="weixin-nav-btn"
            type="button"
            disabled={busy !== null}
            onClick={() => void refresh()}
          >
            {busy === 'refresh' ? (
              <Loader2 style={{ width: 16, height: 16, animation: 'spin 1s linear infinite' }} />
            ) : (
              <RefreshCw style={iconSize} />
            )}
            <span>刷新</span>
          </button>
          <button className="weixin-close-btn" type="button" aria-label="关闭" onClick={onClose}>
            <X style={{ width: 18, height: 18 }} />
          </button>
        </div>
      </div>

      {error && (
        <div className="weixin-error-bar">
          <span>{error}</span>
          <button type="button" onClick={() => { setError(''); void refresh(true); }}>重试</button>
        </div>
      )}

      <div className="weixin-body">
        <section className="weixin-status-grid">
          <div className="weixin-status-cell">
            <span className="weixin-status-label">连接状态</span>
            <strong>{formatStatus(status?.status || 'not_configured')}</strong>
          </div>
          <div className="weixin-status-cell">
            <span className="weixin-status-label">Bot ID</span>
            <strong>{status?.botUserId || '-'}</strong>
          </div>
          <div className="weixin-status-cell">
            <span className="weixin-status-label">最近轮询</span>
            <strong>{formatTime(status?.lastPollAt || null)}</strong>
          </div>
          <div className="weixin-status-cell">
            <span className="weixin-status-label">Token 到期</span>
            <strong>{formatTime(status?.loginExpiresAt || null)}</strong>
          </div>
        </section>

        <section className="weixin-main-grid">
          <div className="weixin-panel">
            <div className="weixin-panel-header">
              <div>
                <span className="weixin-panel-kicker">Channel</span>
                <h2>微信直连通道</h2>
              </div>
              <Smartphone style={{ width: 22, height: 22 }} />
            </div>

            <div className="weixin-connection-row">
              <div>
                <span className="weixin-field-label">iLink Base URL</span>
                <code>{status?.baseUrl || 'https://ilinkai.weixin.qq.com'}</code>
              </div>
              <span className={`weixin-token-state ${configured ? 'is-ready' : ''}`}>
                {configured ? 'Token 已保存' : '等待扫码'}
              </span>
            </div>

            <div className="weixin-actions">
              <button className="weixin-primary-btn" type="button" disabled={busy !== null} onClick={handleBeginLogin}>
                {busy === 'login' ? <Loader2 style={{ width: 16, height: 16, animation: 'spin 1s linear infinite' }} /> : <QrCode style={iconSize} />}
                <span>{configured ? '重新扫码' : '扫码连接'}</span>
              </button>
              <button
                className="weixin-secondary-btn"
                type="button"
                disabled={!configured || running || busy !== null}
                onClick={handleStart}
              >
                {busy === 'start' ? <Loader2 style={{ width: 16, height: 16, animation: 'spin 1s linear infinite' }} /> : <Play style={iconSize} />}
                <span>启动</span>
              </button>
              <button
                className="weixin-secondary-btn"
                type="button"
                disabled={!running || busy !== null}
                onClick={handleStop}
              >
                {busy === 'stop' ? <Loader2 style={{ width: 16, height: 16, animation: 'spin 1s linear infinite' }} /> : <Pause style={iconSize} />}
                <span>停止</span>
              </button>
            </div>

            {qr && (
              <div className="weixin-qr-box">
                <div className="weixin-qr-image-wrap">
                  {qrImageDataUrl ? (
                    <img
                      src={qrImageDataUrl}
                      alt="微信 ClawBot 登录二维码"
                      onError={() => {
                        setQrImageDataUrl('');
                        setQrRenderError('二维码图片加载失败，请重新生成。');
                      }}
                    />
                  ) : (
                    <QrCode style={{ width: 84, height: 84 }} />
                  )}
                </div>
                <div className="weixin-qr-side">
                  <span className="weixin-field-label">扫码确认</span>
                  <p>{loginMessage || '请使用微信扫码，并在手机端确认登录。'}</p>
                  {qrRenderError && <span className="weixin-qr-error">{qrRenderError}</span>}
                  <input
                    className="weixin-verify-input"
                    value={verifyCode}
                    placeholder="配对码（如手机端要求）"
                    onChange={(event) => setVerifyCode(event.target.value)}
                  />
                  <button
                    className="weixin-secondary-btn"
                    type="button"
                    disabled={busy !== null || checking}
                    onClick={() => void handleCheckLogin()}
                  >
                    {checking || busy === 'check' ? (
                      <Loader2 style={{ width: 16, height: 16, animation: 'spin 1s linear infinite' }} />
                    ) : (
                      <RotateCcw style={iconSize} />
                    )}
                    <span>检查状态</span>
                  </button>
                </div>
              </div>
            )}
          </div>

          <div className="weixin-panel">
            <div className="weixin-panel-header">
              <div>
                <span className="weixin-panel-kicker">Policy</span>
                <h2>Agent 安全边界</h2>
              </div>
              <ShieldCheck style={{ width: 22, height: 22 }} />
            </div>

            <div className="weixin-policy-list">
              <div className="weixin-policy-item">
                <CheckCircle2 style={iconSize} />
                <span>微信消息使用独立会话上下文。</span>
              </div>
              <div className="weixin-policy-item">
                <CheckCircle2 style={iconSize} />
                <span>仅开放搜索、网页读取、Skill、Plugin Tool。</span>
              </div>
              <div className="weixin-policy-item">
                <CircleAlert style={iconSize} />
                <span>第一版不处理群聊、文件、图片、语音和多账号。</span>
              </div>
            </div>

            {status?.lastError && (
              <div className="weixin-last-error">
                <span className="weixin-field-label">最近错误</span>
                <p>{status.lastError}</p>
              </div>
            )}
          </div>
        </section>

        <section className="weixin-events-panel">
          <div className="weixin-events-header">
            <div>
              <span className="weixin-panel-kicker">Activity</span>
              <h2>最近消息</h2>
            </div>
            <Clock3 style={{ width: 20, height: 20 }} />
          </div>

          <div className="weixin-events-list">
            {events.length === 0 ? (
              <div className="weixin-empty-state">
                <MessageSquareText style={{ width: 24, height: 24 }} />
                <span>暂无微信 ClawBot 事件</span>
              </div>
            ) : events.map((event) => (
              <div className={`weixin-event-row direction-${event.direction}`} key={event.id}>
                <span className="weixin-event-direction">{eventLabel(event.direction)}</span>
                <div className="weixin-event-body">
                  <span className="weixin-event-summary">{event.summary || event.error || '-'}</span>
                  <span className="weixin-event-meta">
                    {event.fromUserId || '-'} · {event.status || '-'} · {formatTime(event.createdAt)}
                  </span>
                </div>
              </div>
            ))}
          </div>
        </section>
      </div>
    </div>
  );
}
