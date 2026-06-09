import {
  Archive,
  ArrowDown,
  ArrowLeft,
  ArrowUp,
  Blocks,
  Cpu,
  Database,
  Folder,
  GitBranch,
  Hexagon,
  Key,
  Layers,
  MessageSquare,
  Moon,
  MoreHorizontal,
  Network,
  PanelLeftClose,
  PanelLeftOpen,
  Paperclip,
  Pin,
  Plus,
  Search,
  Settings,
  Settings2,
  SlidersHorizontal,
  Shrink,
  Square,
  SunMoon,
  Target,
  Trash2,
} from 'lucide-react';
import type { ReactNode } from 'react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { CustomSelect } from './components/CustomControls';
import { MessagePanel } from './components/MessagePanel';
import { ModelSettings } from './components/ModelSettings';
import { EngineSettingsPanel } from './components/EngineSettingsPanel';
import { PluginsPage } from './components/PluginsPage';
import { WorkflowPage } from './components/WorkflowPage';
import { SearchOverlay } from './components/SearchOverlay';
import { ArtifactsPanel, type FileArtifact } from './components/ArtifactsPanel';
import { ToastViewport, type ToastKind, type ToastNotice } from './components/ToastSystem';
import { defaultApiConfigs } from './data/defaultApiConfigs';
import { loadApiConfigsFromStorage, saveApiConfigsToStorage } from './services/apiConfigStorage';
import { loadAppSettingsFromStorage, migrateStorageSettingsToStorage, saveEngineSettingsToStorage } from './services/appSettingsStorage';
import { testAiModel } from './services/aiModelTest';
import { cancelChatMessage, listenToChatStream, sendChatMessageToStorage, type ChatStreamEvent } from './services/chatStorage';
import { loadSessionsFromStorage, readSessionFromStorage, updateSessionTitleInStorage } from './services/sessionStorage';
import { defaultEngineSettings, type StorageSettings } from './types/appSettings';
import type { ModelOption, ProviderConfig, ReasoningEffort } from './types/apiConfig';
import type {
  MessageGroup,
  MessageItem,
  SessionDetail,
  SessionSummary,
  TextMessageItem,
  ThinkingMessageItem,
  ToolMessageItem,
} from './types/session';

type ViewName = 'new' | 'chat' | 'settings' | 'workflow' | 'plugins';
type ThemeName = 'dark' | 'light';
type SettingsSection = 'general' | 'models' | 'api' | 'storage' | 'knowledge';

const iconSize = { width: 16, height: 16 };
const noModelOption = { label: '未配置模型', value: 'none' };

const reasoningOptions: Array<{ label: string; value: ReasoningEffort }> = [
  { label: '不要思考', value: 'none' },
  { label: 'High', value: 'high' },
  { label: 'Medium', value: 'medium' },
  { label: 'Low', value: 'low' },
];

const streamAiGroupId = (sessionId: string) => `stream-ai-${sessionId}`;

// 每个 Agent loop 迭代一组独立的 thinking + text + tool item
const streamThinkingId = (sessionId: string, iter: number) => `stream-thinking-${sessionId}-${iter}`;
const streamTextId = (sessionId: string, iter: number) => `stream-text-${sessionId}-${iter}`;
const streamToolId = (sessionId: string, iter: number, idx: number) => `stream-tool-${sessionId}-${iter}-${idx}`;

function applyStreamEventToSession(
  session: SessionDetail,
  event: ChatStreamEvent,
  iterMap: Map<string, number>,
): SessionDetail {
  const createdAt = new Date().toISOString();
  const aiGroupId = streamAiGroupId(event.sessionId);

  // --- iteration 管理 ---
  const getIter = () => iterMap.get(event.sessionId) ?? 0;
  const bumpIter = () => { const n = getIter() + 1; iterMap.set(event.sessionId, n); };

  const makeThinking = (iter: number): ThinkingMessageItem => ({
    id: streamThinkingId(event.sessionId, iter),
    type: 'thinking',
    label: '正在深度思考',
    content: '',
    status: 'running',
    entryId: '',
    sourceRole: 'assistant-thinking',
    createdAt,
  });

  const makeText = (iter: number): TextMessageItem => ({
    id: streamTextId(event.sessionId, iter),
    type: 'text',
    content: '',
    status: 'running',
    entryId: '',
    sourceRole: 'assistant',
    createdAt,
  });

  let foundAiGroup = false;
  let nextMessages = session.messages.map((group) => {
    if (group.id !== aiGroupId) return group;
    foundAiGroup = true;

    let items = group.items;
    const iter = getIter();

    // ── assistant_thinking_delta ──
    if (event.eventType === 'assistant_thinking_delta') {
      const thinkId = streamThinkingId(event.sessionId, iter);
      const exists = items.some((i) => i.id === thinkId);
      if (!exists) {
        // 新 iteration 的 thinking item — 放到最后
        items = [...items, makeThinking(iter)];
      }
      items = items.map((i) =>
        i.id === thinkId && i.type === 'thinking'
          ? { ...i, content: `${(i as ThinkingMessageItem).content}${event.content}`, status: 'running' as const }
          : i,
      );
    }

    // ── assistant_delta ──
    if (event.eventType === 'assistant_delta') {
      const textId = streamTextId(event.sessionId, iter);
      const exists = items.some((i) => i.id === textId);
      if (!exists) {
        // 插入到当前 iteration 的 thinking 之后（如果有）
        const thinkId = streamThinkingId(event.sessionId, iter);
        const thinkIdx = items.findIndex((i) => i.id === thinkId);
        if (thinkIdx >= 0) {
          items = [...items.slice(0, thinkIdx + 1), makeText(iter), ...items.slice(thinkIdx + 1)];
        } else {
          items = [...items, makeText(iter)];
        }
      }
      items = items.map((i) =>
        i.id === textId && i.type === 'text'
          ? { ...i, content: `${(i as TextMessageItem).content}${event.content}`, status: 'running' as const }
          : i,
      );
    }

    // ── tool_call ──
    if (event.eventType === 'tool_call') {
      const toolId = streamToolId(event.sessionId, iter, items.filter((i) => i.type === 'tool').length);
      const toolItem: ToolMessageItem = {
        id: toolId,
        type: 'tool',
        label: event.toolLabel || '工具调用',
        status: 'completed',
        entryId: '',
        sourceRole: 'assistant',
        createdAt,
        detail: event.toolDetail,
      };
      // 插入到当前 iteration text 之后、下一个 iteration 开始之前
      const textId = streamTextId(event.sessionId, iter);
      const textIdx = items.findIndex((i) => i.id === textId);
      if (textIdx >= 0) {
        items = [...items.slice(0, textIdx + 1), toolItem, ...items.slice(textIdx + 1)];
      } else {
        items = [...items, toolItem];
      }
      // 进入下一个迭代
      bumpIter();
    }

    // ── complete / error / cancelled — 标记所有 running 为 completed ──
    if (event.eventType === 'complete' || event.eventType === 'error' || event.eventType === 'cancelled') {
      items = items.map((i) => {
        if (i.status !== 'running') return i;
        if (i.type === 'thinking') {
          return {
            ...i,
            label: event.eventType === 'cancelled' ? '对话已被用户中断' : event.eventType === 'complete' ? '深度思考已完成' : '深度思考已中断',
            status: 'completed' as const,
          } as ThinkingMessageItem;
        }
        if (i.type === 'text') {
          return {
            ...i,
            content: (i as TextMessageItem).content || event.content || (event.error ?? ''),
            status: 'completed' as const,
          } as TextMessageItem;
        }
        return { ...i, status: 'completed' as const };
      });
      // reset iteration counter
      iterMap.set(event.sessionId, 0);
    }

    return { ...group, items };
  });

  // ── 首次事件，创建 AI group ──
  if (!foundAiGroup) {
    let items: MessageItem[] = [];
    const iter = getIter();

    if (event.eventType === 'assistant_thinking_delta') {
      items.push({ ...makeThinking(iter), content: event.content });
    }

    if (event.eventType === 'tool_call') {
      const toolItem: ToolMessageItem = {
        id: streamToolId(event.sessionId, iter, 0),
        type: 'tool',
        label: event.toolLabel || '工具调用',
        status: 'completed',
        entryId: '',
        sourceRole: 'assistant',
        createdAt,
        detail: event.toolDetail,
      };
      items.push(toolItem);
      bumpIter();
    }

    if (event.eventType === 'assistant_delta' || event.eventType === 'complete' || event.eventType === 'error') {
      items.push({
        ...makeText(iter),
        content: event.eventType === 'assistant_delta' ? event.content : event.eventType === 'complete' ? event.content : (event.error ?? ''),
        status: event.eventType === 'assistant_delta' ? 'running' as const : 'completed' as const,
      });
    }

    if (items.length > 0) {
      nextMessages = [...nextMessages, { id: aiGroupId, role: 'ai' as const, items }];
    }
  }

  return { ...session, updatedAt: createdAt, messages: nextMessages };
}

export function App() {
  const [view, setView] = useState<ViewName>('new');
  const [theme, setTheme] = useState<ThemeName>('light');
  const [activeItem, setActiveItem] = useState('');
  const [activeSettingsSection, setActiveSettingsSection] = useState<SettingsSection>('api');
  const [message, setMessage] = useState('');
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [providers, setProviders] = useState<ProviderConfig[]>(defaultApiConfigs);
  const [selectedModelId, setSelectedModelId] = useState('');
  const [isSavingConfigs, setIsSavingConfigs] = useState(false);
  const [isSavingEngine, setIsSavingEngine] = useState(false);
  const [isMigratingStorage, setIsMigratingStorage] = useState(false);
  const [storageStatus, setStorageStatus] = useState('配置尚未保存。');
  const [storagePath, setStoragePath] = useState('C:\\Users\\jae\\AppData\\Roaming\\Otherone');
  const [storageDraft, setStorageDraft] = useState(storagePath);
  const [migrationStatus, setMigrationStatus] = useState('当前使用 localfile 与 SQLite 组合存储。');
  const [artifactPath, setArtifactPath] = useState('C:\\Users\\jae\\AppData\\Roaming\\Otherone\\artifacts');
  const [artifactDraft, setArtifactDraft] = useState(artifactPath);
  const [dialogueDataPath, setDialogueDataPath] = useState('C:\\Users\\jae\\AppData\\Roaming\\Otherone\\dialogues');
  const [dialogueDataDraft, setDialogueDataDraft] = useState(dialogueDataPath);
  const [engineSettings, setEngineSettings] = useState(defaultEngineSettings);
  const [testingProviderId, setTestingProviderId] = useState('');
  const [reasoningEffort, setReasoningEffort] = useState<ReasoningEffort>('medium');
  const [promptPanelOpen, setPromptPanelOpen] = useState(false);
  const [contextCompressionEnabled, setContextCompressionEnabled] = useState(false);
  const [branchModeEnabled, setBranchModeEnabled] = useState(false);
  const [targetModeEnabled, setTargetModeEnabled] = useState(false);
  const [artifactsPanelOpen, setArtifactsPanelOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [editedFiles] = useState<FileArtifact[]>([
    { id: 'e1', name: 'index.tsx', path: 'src/pages/index.tsx', extension: 'tsx', timestamp: '2 分钟前' },
    { id: 'e2', name: 'styles.css', path: 'src/styles.css', extension: 'css', timestamp: '5 分钟前' },
    { id: 'e3', name: 'report.pdf', path: 'output/report.pdf', extension: 'pdf', timestamp: '12 分钟前' },
  ]);
  const [deletedFiles] = useState<FileArtifact[]>([
    { id: 'd1', name: 'old-config.json', path: 'config/old-config.json', extension: 'json', timestamp: '8 分钟前' },
  ]);
  const [addedFiles] = useState<FileArtifact[]>([
    { id: 'a1', name: 'dashboard.tsx', path: 'src/components/dashboard.tsx', extension: 'tsx', timestamp: '刚刚' },
    { id: 'a2', name: 'logo.png', path: 'assets/logo.png', extension: 'png', timestamp: '3 分钟前' },
    { id: 'a3', name: 'proposal.pptx', path: 'output/proposal.pptx', extension: 'pptx', timestamp: '7 分钟前' },
    { id: 'a4', name: 'data.xlsx', path: 'data/data.xlsx', extension: 'xlsx', timestamp: '10 分钟前' },
  ]);
  const [toasts, setToasts] = useState<ToastNotice[]>([]);
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [activeSession, setActiveSession] = useState<SessionDetail | null>(null);
  const [isLoadingSessions, setIsLoadingSessions] = useState(false);
  const [isLoadingSessionDetail, setIsLoadingSessionDetail] = useState(false);
  const streamingSessionIdsRef = useRef<Set<string>>(new Set());
  const sessionCacheRef = useRef<Map<string, SessionDetail>>(new Map());
  const [sessionError, setSessionError] = useState('');
  const [editingSessionId, setEditingSessionId] = useState('');
  const [editingSessionTitle, setEditingSessionTitle] = useState('');
  const attachmentInputRef = useRef<HTMLInputElement | null>(null);
  const promptRef = useRef<HTMLTextAreaElement | null>(null);
  const chatSectionRef = useRef<HTMLElement | null>(null);
  const toastCounterRef = useRef(0);
  const isAutoScrollEnabledRef = useRef(true);
  const streamIterRef = useRef<Map<string, number>>(new Map());
  const [showScrollButton, setShowScrollButton] = useState(false);
  const timelineHideTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastScrollTopRef = useRef(0);
  const pendingStreamEventsRef = useRef<Map<string, ChatStreamEvent[]>>(new Map());
  const previousView = useRef<ViewName>('new');

  const isAiStreaming = useMemo(() => {
    if (!activeSession) return false;
    return activeSession.messages.some((group) =>
      group.role === 'ai' && group.items.some((item) => item.status === 'running'),
    );
  }, [activeSession]);

  // Timeline 导航：收集所有用户 prompt 作为锚点
  const timelineAnchors = useMemo(() => {
    if (!activeSession) return [];
    return activeSession.messages
      .filter((group) => group.role === 'user')
      .map((group) => {
        const textItem = group.items.find((item) => item.type === 'text');
        const preview = (textItem?.content ?? '新对话').slice(0, 32);
        return { id: group.id, label: preview || '新对话' };
      });
  }, [activeSession]);

  // Timeline 可见性 & 当前激活索引
  const [timelineVisible, setTimelineVisible] = useState(false);
  const [activeTimelineIndex, setActiveTimelineIndex] = useState(-1);

  const modelOptions = useMemo<ModelOption[]>(
    () =>
      providers.flatMap((provider) =>
        provider.models.map((model) => ({
          id: model.id,
          label: `${provider.name} / ${model.name}`,
          providerName: provider.name,
        })),
      ),
    [providers],
  );

  const allArtifacts = useMemo(() => [...editedFiles, ...deletedFiles, ...addedFiles], [addedFiles, deletedFiles, editedFiles]);

  const selectorOptions = modelOptions.length
    ? modelOptions.map((model) => ({ label: model.label, value: model.id }))
    : [noModelOption];

  useEffect(() => {
    let cancelled = false;

    async function loadConfigs() {
      try {
        const storedProviders = await loadApiConfigsFromStorage();

        if (cancelled) {
          return;
        }

        if (storedProviders.length > 0) {
          setProviders(storedProviders);
          setStorageStatus('已从本地 SQLite 读取 API 配置。');
        } else {
          setStorageStatus('尚未保存 API 配置，当前使用默认配置模板。');
        }
      } catch {
        if (!cancelled) {
          setStorageStatus('读取本地 SQLite 配置失败，当前使用默认配置模板。');
        }
      }
    }

    void loadConfigs();

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;

    async function loadAppSettings() {
      try {
        const settings = await loadAppSettingsFromStorage();

        if (cancelled || !settings) {
          return;
        }

        applyStorageSettings(settings.storage);
        setEngineSettings(settings.engine);
        setReasoningEffort(settings.engine.defaultReasoningEffort);
        setMigrationStatus('已从本地设置读取存储路径。');
      } catch (error) {
        if (!cancelled) {
          setMigrationStatus(error instanceof Error ? error.message : String(error));
        }
      }
    }

    void loadAppSettings();

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;

    async function loadSessionList() {
      setIsLoadingSessions(true);
      setSessionError('');

      try {
        const storedSessions = await loadSessionsFromStorage();

        if (!cancelled) {
          setSessions(storedSessions);
        }
      } catch (error) {
        if (!cancelled) {
          setSessionError(error instanceof Error ? error.message : String(error));
        }
      } finally {
        if (!cancelled) {
          setIsLoadingSessions(false);
        }
      }
    }

    void loadSessionList();

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (modelOptions.length === 0) {
      setSelectedModelId('');
      return;
    }

    const selectedStillExists = modelOptions.some((model) => model.id === selectedModelId);
    const defaultModel = providers.flatMap((provider) => provider.models).find((model) => model.defaultModel);

    if (!selectedStillExists) {
      setSelectedModelId(defaultModel?.id ?? modelOptions[0].id);
    }
  }, [modelOptions, providers, selectedModelId]);

  useEffect(() => {
    document.documentElement.style.colorScheme = theme;

    let themeMeta = document.querySelector<HTMLMetaElement>('meta[name="theme-color"]');
    if (!themeMeta) {
      themeMeta = document.createElement('meta');
      themeMeta.name = 'theme-color';
      document.head.appendChild(themeMeta);
    }
    themeMeta.content = theme === 'dark' ? '#000000' : '#ffffff';

    if ('__TAURI_INTERNALS__' in window) {
      import('@tauri-apps/api/window')
        .then(({ getCurrentWindow }) => getCurrentWindow().setTheme(theme))
        .catch(() => {
          // Browser preview does not expose Tauri window APIs.
        });
    }
  }, [theme]);

  useEffect(() => {
    resizePrompt();
  }, [message]);

  // 直接钉在底部：每次消息更新时立刻 scrollTop = scrollHeight
  const pinToBottom = useCallback(() => {
    const section = chatSectionRef.current;
    if (!section) return;
    if (!isAutoScrollEnabledRef.current) return;
    section.scrollTop = section.scrollHeight;
  }, []);

  // 监听鼠标滚轮 — 向上滚打断，向下滚到底恢复
  useEffect(() => {
    const section = chatSectionRef.current;
    if (!section || view !== 'chat') return;

    const onWheel = (e: WheelEvent) => {
      if (e.deltaY < 0) {
        isAutoScrollEnabledRef.current = false;
        setShowScrollButton(true);
        setTimelineVisible(true);
      } else if (e.deltaY > 0) {
        const sec = chatSectionRef.current;
        if (!sec) return;
        const dist = sec.scrollHeight - sec.scrollTop - sec.clientHeight;
        if (dist < 50) {
          isAutoScrollEnabledRef.current = true;
          setShowScrollButton(false);
          setTimelineVisible(false);
        }
      }
    };

    section.addEventListener('wheel', onWheel, { passive: true });
    return () => section.removeEventListener('wheel', onWheel);
  }, [view]);

  // 流式/非流式消息更新 → 直接钉到底部
  useEffect(() => {
    if (view !== 'chat' || !activeSession) return;
    pinToBottom();
  }, [activeSession?.messages, view, pinToBottom]);

  const handleScrollToBottom = useCallback(() => {
    const section = chatSectionRef.current;
    if (!section) return;
    isAutoScrollEnabledRef.current = true;
    setShowScrollButton(false);
    setTimelineVisible(false);
    section.scrollTo({ top: section.scrollHeight, behavior: 'smooth' });
  }, []);

  // 跳转到指定锚点 (timeline 点击)
  const jumpToAnchor = useCallback((groupId: string) => {
    const section = chatSectionRef.current;
    if (!section) return;
    const el = document.getElementById(`turn-${groupId}`);
    if (!el) return;

    // 高亮动画
    document.querySelectorAll('.chat-turn').forEach((turn) => turn.classList.remove('target-highlight'));
    void (el as HTMLElement).offsetWidth;
    el.classList.add('target-highlight');

    // 平滑滚动到目标
    el.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }, []);

  // 判断是否在底部 + 滚动方向检测 + timeline 3s 自动隐藏
  const updateScrollState = useCallback(() => {
    const section = chatSectionRef.current;
    if (!section) return;
    const distanceFromBottom = section.scrollHeight - section.scrollTop - section.clientHeight;
    const isAtBottom = distanceFromBottom < 50;

    // 触底/离底更新 UI
    setShowScrollButton(!isAtBottom);

    // 检测滚动方向
    const currentTop = section.scrollTop;
    const isScrollingUp = currentTop < lastScrollTopRef.current;
    lastScrollTopRef.current = currentTop;

    // 清除之前的 timer
    if (timelineHideTimerRef.current) {
      clearTimeout(timelineHideTimerRef.current);
      timelineHideTimerRef.current = null;
    }

    if (isScrollingUp && !isAtBottom) {
      setTimelineVisible(true);
    } else if (!isAtBottom) {
      // 停止滚动或向下滚动 → 3s 后隐藏
      timelineHideTimerRef.current = setTimeout(() => {
        setTimelineVisible(false);
        timelineHideTimerRef.current = null;
      }, 3000);
    } else {
      setTimelineVisible(false);
    }
  }, []);

  useEffect(() => {
    if (view !== 'chat') {
      return;
    }

    // 切换到 chat view 时强制滚动到底部
    isAutoScrollEnabledRef.current = true;
    const section = chatSectionRef.current;
    if (section) {
      section.scrollTop = section.scrollHeight;
    }
  }, [view]);

  // Scrollspy: 用 IntersectionObserver 追踪哪个 user-prompt 可见
  useEffect(() => {
    if (view !== 'chat' || timelineAnchors.length === 0) {
      setActiveTimelineIndex(-1);
      return;
    }

    const container = chatSectionRef.current;
    if (!container) return;

    const targets = timelineAnchors
      .map((anchor) => document.getElementById(`turn-${anchor.id}`))
      .filter(Boolean);

    if (targets.length === 0) return;

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            const idx = timelineAnchors.findIndex((a) => `turn-${a.id}` === entry.target.id);
            if (idx >= 0) setActiveTimelineIndex(idx);
            // 只跟踪第一个进入视口的即可
            break;
          }
        }
      },
      { root: container, rootMargin: '-15% 0px -70% 0px', threshold: 0 },
    );

    targets.forEach((el) => observer.observe(el!));
    return () => observer.disconnect();
  }, [view, timelineAnchors, activeSession?.messages]);

  const applyStorageSettings = (storage: StorageSettings) => {
    setStoragePath(storage.dataRoot);
    setStorageDraft(storage.dataRoot);
    setArtifactPath(storage.artifactRoot);
    setArtifactDraft(storage.artifactRoot);
    setDialogueDataPath(storage.dialogueRoot);
    setDialogueDataDraft(storage.dialogueRoot);
  };

  const switchView = (nextView: ViewName, itemId = '') => {
    if (nextView === 'settings') {
      previousView.current = view;
    }
    if (nextView !== 'new' && nextView !== 'chat') {
      setArtifactsPanelOpen(false);
    }
    if (nextView === 'new') {
      setActiveSession(null);
    }
    setView(nextView);
    setActiveItem(itemId);
  };

  const openSession = async (sessionId: string) => {
    setView('chat');
    setActiveItem(sessionId);

    // 优先使用缓存（保留流式状态）
    const cached = sessionCacheRef.current.get(sessionId);
    if (cached) {
      setActiveSession(cached);
      setSessionError('');
      if (cached.messages.length > 0) {
        setIsLoadingSessionDetail(false);
      } else {
        setIsLoadingSessionDetail(false);
      }
      return;
    }

    setIsLoadingSessionDetail(true);
    setSessionError('');

    try {
      const session = await readSessionFromStorage(sessionId);
      if (session) {
        sessionCacheRef.current.set(sessionId, session);
      }
      setActiveSession(session);
    } catch (error) {
      setActiveSession(null);
      setSessionError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsLoadingSessionDetail(false);
    }
  };

  const startEditingSessionTitle = (session: SessionSummary) => {
    setEditingSessionId(session.id);
    setEditingSessionTitle(session.title);
  };

  const cancelEditingSessionTitle = () => {
    setEditingSessionId('');
    setEditingSessionTitle('');
  };

  const saveSessionTitle = async () => {
    const nextTitle = editingSessionTitle.trim();

    if (!editingSessionId || !nextTitle) {
      cancelEditingSessionTitle();
      return;
    }

    try {
      await updateSessionTitleInStorage(editingSessionId, nextTitle);
      setSessions((current) =>
        current.map((session) => (session.id === editingSessionId ? { ...session, title: nextTitle } : session)),
      );
      setActiveSession((current) => (current?.id === editingSessionId ? { ...current, title: nextTitle } : current));
    } catch (error) {
      pushToast('error', '保存会话标题失败', error instanceof Error ? error.message : String(error));
    } finally {
      cancelEditingSessionTitle();
    }
  };

  const togglePinSession = (sessionId: string) => {
    setSessions((current) =>
      current.map((session) =>
        session.id === sessionId ? { ...session, pinned: !session.pinned } : session,
      ),
    );
  };

  const archiveSession = (sessionId: string) => {
    setSessions((current) =>
      current.map((session) =>
        session.id === sessionId ? { ...session, archived: !session.archived } : session,
      ),
    );
  };

  const deleteSession = (sessionId: string) => {
    setSessions((current) => current.filter((session) => session.id !== sessionId));
    if (activeItem === sessionId) {
      setActiveItem('');
      setActiveSession(null);
    }
  };

  const goBackFromSettings = () => {
    setView(previousView.current);
    setActiveItem('');
  };

  const toggleTheme = () => {
    setTheme((currentTheme) => (currentTheme === 'dark' ? 'light' : 'dark'));
  };

  const chooseStoragePath = () => {
    const nextPath = 'D:\\OtheroneData';
    setStorageDraft(nextPath);
    setMigrationStatus('已选择新的数据目录，保存后会迁移 localfile 与 SQLite 数据。');
  };

  const saveStoragePath = () => {
    void migrateStorageSettings();
    return;
    setStoragePath(storageDraft);
    setMigrationStatus(`已保存数据目录：${storageDraft}`);
  };

  const chooseArtifactPath = () => {
    const nextPath = 'D:\\OtheroneData\\artifacts';
    setArtifactDraft(nextPath);
  };

  const saveArtifactPath = () => {
    void migrateStorageSettings();
    return;
    setArtifactPath(artifactDraft);
  };

  const chooseDialogueDataPath = () => {
    const nextPath = 'D:\\OtheroneData\\dialogues';
    setDialogueDataDraft(nextPath);
  };

  const saveDialogueDataPath = () => {
    void migrateStorageSettings();
    return;
    setDialogueDataPath(dialogueDataDraft);
  };

  const migrateStorageSettings = async () => {
    const confirmed = window.confirm(
      '迁移会把当前受管数据复制到新目录，校验成功后删除旧目录中的受管数据。迁移或清理过程中如果出现数据丢失，应用无法恢复旧数据。请先手动备份当前数据目录，再继续。',
    );

    if (!confirmed) {
      setMigrationStatus('已取消迁移。');
      return;
    }

    setIsMigratingStorage(true);
    setMigrationStatus('正在迁移存储数据，请不要关闭应用。');

    try {
      const nextStorage = {
        dataRoot: storageDraft,
        artifactRoot: artifactDraft,
        dialogueRoot: dialogueDataDraft,
      };
      const settings = await migrateStorageSettingsToStorage(nextStorage);

      applyStorageSettings(settings?.storage ?? nextStorage);
      setMigrationStatus('存储迁移完成，旧受管数据已清理。');
      setSessions(await loadSessionsFromStorage());
    } catch (error) {
      setMigrationStatus(error instanceof Error ? error.message : String(error));
    } finally {
      setIsMigratingStorage(false);
    }
  };

  const saveApiConfigs = async () => {
    setIsSavingConfigs(true);
    setStorageStatus('正在保存 API 配置到本地 SQLite...');

    try {
      await saveApiConfigsToStorage(providers);
      setStorageStatus('API 配置已保存到本地 SQLite。');
    } catch {
      setStorageStatus('保存 API 配置失败，请稍后重试。');
    } finally {
      setIsSavingConfigs(false);
    }
  };

  const saveEngineSettings = async () => {
    setIsSavingEngine(true);

    try {
      const settings = await saveEngineSettingsToStorage(engineSettings);
      if (settings) {
        setEngineSettings(settings.engine);
        setReasoningEffort(settings.engine.defaultReasoningEffort);
      }
      pushToast('success', '引擎配置已保存');
    } catch (error) {
      pushToast('error', '保存引擎配置失败', error instanceof Error ? error.message : String(error));
    } finally {
      setIsSavingEngine(false);
    }
  };

  const pushToast = useCallback((type: ToastKind, title: string, description?: string) => {
    toastCounterRef.current += 1;
    setToasts((current) => {
      const nextToast = {
        id: `toast-${Date.now()}-${toastCounterRef.current}`,
        type,
        title,
        description,
      };

      return [nextToast, ...current];
    });
  }, []);

  const dismissToast = useCallback((id: string) => {
    setToasts((current) => current.filter((toast) => toast.id !== id));
  }, []);

  const resizePrompt = () => {
    const textarea = promptRef.current;

    if (!textarea) {
      return;
    }

    textarea.style.height = 'auto';
    textarea.style.height = `${Math.min(textarea.scrollHeight, 180)}px`;
  };

  const handleTestProvider = async (provider: ProviderConfig) => {
    const model = provider.models.find((item) => item.defaultModel) ?? provider.models[0];

    if (!model) {
      pushToast('warn', '没有可测试的模型', '请先在这个配置块中新增一个模型。');
      return;
    }

    if (!provider.baseUrl.trim() || !provider.apiKey.trim() || !model.name.trim()) {
      pushToast('warn', '测试配置不完整', 'Base URL、API Key 和模型名称都需要填写。');
      return;
    }

    setTestingProviderId(provider.id);
    pushToast('info', '正在测试 AI 模型', `${provider.name} / ${model.name}`);

    try {
      const result = await testAiModel(provider, model);
      pushToast('success', '模型测试成功', `响应时间 ${result.latencyMs} ms`);
    } catch (error) {
      pushToast('error', '模型测试失败', error instanceof Error ? error.message : String(error));
    } finally {
      setTestingProviderId('');
    }
  };

  const handlePromptChange = (value: string) => {
    setMessage(value);
    requestAnimationFrame(resizePrompt);
  };

  const handleAttachmentChange = (files: FileList | null) => {
    const fileCount = files?.length ?? 0;

    if (fileCount > 0) {
      pushToast('info', '附件已加入当前消息', `${fileCount} 个文件等待随消息发送。`);
    }
  };

  const toggleContextCompression = () => {
    setContextCompressionEnabled((current) => {
      const next = !current;
      pushToast(next ? 'info' : 'warn', next ? '已启用压缩上下文' : '已关闭压缩上下文');
      return next;
    });
  };

  const toggleBranchMode = () => {
    setBranchModeEnabled((current) => {
      const next = !current;
      pushToast(next ? 'info' : 'warn', next ? '已启用创建分支' : '已关闭创建分支');
      return next;
    });
  };

  const toggleTargetMode = () => {
    setTargetModeEnabled((current) => {
      const next = !current;
      pushToast(next ? 'info' : 'warn', next ? '已启用目标模式' : '已关闭目标模式');
      return next;
    });
  };

  // 从 current streaming state 中收集 thinking items，
  // 然后合并到 reloaded session 的最后一个 AI group 中。
  // 因为 otherone-agent 框架不单独持久化 thinking delta，只存最终 AI 文本 entry。
  const refreshSessionFromStorage = useCallback(
    async (sessionId: string, pendingThinkingItems?: ThinkingMessageItem[]) => {
      try {
        const [session, storedSessions] = await Promise.all([
          readSessionFromStorage(sessionId),
          loadSessionsFromStorage(),
        ]);

        if (!session) {
          setSessionError('会话数据读取失败。');
          return;
        }

        setActiveSession((current) => {
          // 从 current state（或传入的 pending items）收集 thinking
          const thinkingItems: ThinkingMessageItem[] = pendingThinkingItems ?? (() => {
            if (!current || current.id !== sessionId) return [];
            const items: ThinkingMessageItem[] = [];
            for (const group of current.messages) {
              for (const item of group.items) {
                if (item.type === 'thinking' && item.content) {
                  items.push({
                    ...(item as ThinkingMessageItem),
                    status: 'completed' as const,
                    label: '深度思考已完成',
                  });
                }
              }
            }
            return items;
          })();

          if (thinkingItems.length === 0) return session;

          // 把 thinking items 插入到最后一个 AI group 的最前面
          const aiIndexes: number[] = [];
          session.messages.forEach((g, i) => { if (g.role === 'ai') aiIndexes.push(i); });
          const lastAiIdx = aiIndexes.length > 0 ? aiIndexes[aiIndexes.length - 1] : -1;

          if (lastAiIdx < 0) return session;

          return {
            ...session,
            messages: session.messages.map((group, idx) => {
              if (idx !== lastAiIdx) return group;
              const nonThinkingItems = group.items.filter((i) => i.type !== 'thinking');
              return { ...group, items: [...thinkingItems, ...nonThinkingItems] };
            }),
          };
        });

        setSessions(storedSessions);
        setActiveItem(sessionId);
      } catch (error) {
        setSessionError(error instanceof Error ? error.message : String(error));
      }
    },
    [],
  );

  const updateStreamingMessage = (event: ChatStreamEvent) => {
    setActiveSession((current) => {
      if (!current || current.id !== event.sessionId) {
        return current;
      }

      const messages = current.messages.map((group) => {
        if (group.id !== `stream-ai-${event.sessionId}`) {
          return group;
        }

        return {
          ...group,
          items: group.items.map((item) => {
            if (item.type !== 'text') {
              return item;
            }

            if (event.eventType === 'assistant_delta') {
              return { ...item, content: `${item.content}${event.content}`, status: 'running' as const };
            }

            if (event.eventType === 'complete') {
              return { ...item, content: item.content || event.content, status: 'completed' as const };
            }

            if (event.eventType === 'error') {
              return {
                ...item,
                content: item.content || event.error || '对话执行失败。',
                status: 'completed' as const,
              };
            }

            return item;
          }),
        };
      });

      return { ...current, messages };
    });
  };

  const updateStreamingMessageV2 = useCallback((event: ChatStreamEvent) => {
    const streamEvent =
      event.eventType === 'thinking'
        ? { ...event, eventType: 'assistant_thinking_delta' as const }
        : event;

    if (
      streamEvent.eventType !== 'assistant_delta' &&
      streamEvent.eventType !== 'assistant_thinking_delta' &&
      streamEvent.eventType !== 'tool_call' &&
      streamEvent.eventType !== 'complete' &&
      streamEvent.eventType !== 'error' &&
      streamEvent.eventType !== 'cancelled'
    ) {
      return;
    }

    const sid = streamEvent.sessionId;

    // 始终更新 session 缓存
    const cached = sessionCacheRef.current.get(sid);
    if (cached) {
      const updated = applyStreamEventToSession(cached, streamEvent, streamIterRef.current);
      sessionCacheRef.current.set(sid, updated);
    }

    // 只有当前展示的 session 才更新 activeSession（触发 rerender）
    setActiveSession((current) => {
      if (!current || current.id !== sid) return current;
      return applyStreamEventToSession(current, streamEvent, streamIterRef.current);
    });
  }, []);

  const handleChatStreamEvent = useCallback(
    (event: ChatStreamEvent) => {
      if (
        event.eventType === 'assistant_delta' ||
        event.eventType === 'assistant_thinking_delta' ||
        event.eventType === 'thinking' ||
        event.eventType === 'tool_call' ||
        event.eventType === 'complete' ||
        event.eventType === 'error' ||
        event.eventType === 'cancelled'
      ) {
        updateStreamingMessageV2(event);
      }

      if (event.eventType === 'complete') {
        streamingSessionIdsRef.current.delete(event.sessionId);
        void refreshSessionFromStorage(event.sessionId);
      }

      if (event.eventType === 'error') {
        streamingSessionIdsRef.current.delete(event.sessionId);
        pushToast('error', '对话执行失败', event.error ?? event.content);
      }

      if (event.eventType === 'cancelled') {
        streamingSessionIdsRef.current.delete(event.sessionId);
        pushToast('info', '对话已被终止');
      }
    },
    [pushToast, refreshSessionFromStorage, updateStreamingMessageV2],
  );

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    let cancelled = false;

    void listenToChatStream(handleChatStreamEvent).then((unlisten) => {
      if (cancelled) {
        unlisten();
        return;
      }
      cleanup = unlisten;
    });

    return () => {
      cancelled = true;
      cleanup?.();
    };
  }, [handleChatStreamEvent]);

  const handleCancelMessage = async () => {
    if (!activeSession) return;

    const sessionId = activeSession.id;
    // 立即在前端标记流式结束
    streamingSessionIdsRef.current.delete(sessionId);

    try {
      await cancelChatMessage(sessionId);
    } catch (error) {
      pushToast('error', '终止对话失败', error instanceof Error ? error.message : String(error));
    }
  };

  const handleSendMessage = async () => {
    const prompt = message.trim();
    const modelId = selectedModelId || selectorOptions[0]?.value;

    if (!prompt || !modelId || modelId === 'none') {
      return;
    }

    const sessionId = activeSession?.id ?? createClientSessionId();

    // 检查目标 session 是否已在流式传输中
    if (streamingSessionIdsRef.current.has(sessionId)) return;

    const createdAt = new Date().toISOString();
    const userGroup: MessageGroup = {
      id: `stream-user-${sessionId}-${Date.now()}`,
      role: 'user',
      items: [
        {
          id: `stream-user-item-${sessionId}`,
          type: 'text',
          content: prompt,
          status: 'completed',
          entryId: '',
          sourceRole: 'user',
          createdAt,
        },
      ],
    };
    const aiGroup: MessageGroup = {
      id: streamAiGroupId(sessionId),
      role: 'ai',
      items: [
        {
          id: streamTextId(sessionId, 0),
          type: 'text',
          content: '',
          status: 'running',
          entryId: '',
          sourceRole: 'assistant',
          createdAt,
        },
      ],
    };

    setActiveSession((current) => {
      const base = {
        id: sessionId,
        title: prompt.slice(0, 24) || '新对话',
        createdAt,
        updatedAt: createdAt,
        messages: [userGroup, aiGroup],
      };
      // 同步到缓存
      sessionCacheRef.current.set(sessionId, {
        ...base,
        messages: current?.id === sessionId
          ? [...current.messages, userGroup, aiGroup]
          : base.messages,
      });
      if (current?.id === sessionId) {
        return { ...current, messages: [...current.messages, userGroup, aiGroup] };
      }
      return base;
    });
    setSessions((current) => {
      const nextSummary: SessionSummary = {
        id: sessionId,
        title: prompt.slice(0, 24) || '新对话',
        createdAt,
        updatedAt: createdAt,
        lastMessage: prompt,
        messageCount: 1,
        pinned: false,
        archived: false,
      };
      const existing = current.find((session) => session.id === sessionId);

      if (existing) {
        return [
          { ...existing, updatedAt: createdAt, lastMessage: prompt, messageCount: existing.messageCount + 1 },
          ...current.filter((session) => session.id !== sessionId),
        ];
      }

      return [nextSummary, ...current];
    });
    setView('chat');
    setActiveItem(sessionId);
    setMessage('');
    setPromptPanelOpen(false);

    // 标记为目标 session 正在流式传输
    streamingSessionIdsRef.current.add(sessionId);
    streamIterRef.current.set(sessionId, 0);

    try {
      await sendChatMessageToStorage({
        sessionId,
        modelId,
        prompt,
        reasoningEffort,
        contextCompressionEnabled,
        branchModeEnabled,
        targetModeEnabled,
      });
    } catch (error) {
      streamingSessionIdsRef.current.delete(sessionId);
      handleChatStreamEvent({
        sessionId,
        eventType: 'error',
        content: '',
        error: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const isCurrentSessionStreaming = activeSession != null && streamingSessionIdsRef.current.has(activeSession.id);

  const showChatUi = view !== 'settings' && view !== 'workflow' && view !== 'plugins';
  const currentModelValue = selectedModelId || selectorOptions[0].value;
  const pinnedSessions = sessions.filter((session) => session.pinned);
  const regularSessions = sessions.filter((session) => !session.pinned);

  return (
    <div className={`app-shell ${sidebarCollapsed ? 'sidebar-collapsed' : ''}`} data-theme={theme}>
      <aside className="sidebar">
        <div className="sidebar-header">
          <div className="logo-area">
            <div className="logo-title">
              <Hexagon />
              <span>otherone</span>
            </div>
            <button
              className="sidebar-collapse-btn"
              type="button"
              aria-label={sidebarCollapsed ? '展开侧边栏' : '收起侧边栏'}
              onClick={() => setSidebarCollapsed((current) => !current)}
            >
              {sidebarCollapsed ? <PanelLeftOpen style={iconSize} /> : <PanelLeftClose style={iconSize} />}
            </button>
          </div>
          <button className="new-chat-btn" type="button" onClick={() => switchView('new')}>
            <Plus className="new-chat-btn-icon" style={{ width: 18, height: 18 }} />
            <span>新对话</span>
          </button>
        </div>

        <div className="nav-section">
          <div className="nav-primary">
            <SidebarItem icon={<Search style={iconSize} />} label="全局搜索" compact={sidebarCollapsed} onClick={() => setSearchOpen(true)} />
            <SidebarItem icon={<Network style={iconSize} />} label="工作流" compact={sidebarCollapsed} onClick={() => switchView('workflow')} />
            <SidebarItem icon={<Blocks style={iconSize} />} label="插件管理" compact={sidebarCollapsed} onClick={() => switchView('plugins')} />
            <SidebarItem icon={<SlidersHorizontal style={iconSize} />} label="个性化" compact={sidebarCollapsed} />
          </div>

          <div className="nav-history">
            <div className="nav-divider" />

            {isLoadingSessions && <div className="nav-empty">正在读取本地会话...</div>}
            {!isLoadingSessions && sessionError && <div className="nav-empty">会话读取失败</div>}
            {!isLoadingSessions && !sessionError && sessions.length === 0 && <div className="nav-empty">暂无本地会话</div>}

            {pinnedSessions.length > 0 && (
              <div className="nav-session-group nav-session-group-pinned">
                <div className="nav-title">置顶</div>
                <div className="nav-session-items">
                  {pinnedSessions.map((session) => (
                    <SessionSidebarItem
                      key={session.id}
                      active={activeItem === session.id}
                      editing={editingSessionId === session.id}
                      icon={<Pin style={{ width: 14, height: 14 }} />}
                      title={session.title}
                      draftTitle={editingSessionTitle}
                      onClick={() => void openSession(session.id)}
                      onDoubleClick={() => startEditingSessionTitle(session)}
                      onDraftTitleChange={setEditingSessionTitle}
                      onCancelEdit={cancelEditingSessionTitle}
                      onSaveEdit={() => void saveSessionTitle()}
                      onPin={() => togglePinSession(session.id)}
                      onRename={() => startEditingSessionTitle(session)}
                      onArchive={() => archiveSession(session.id)}
                      onDelete={() => deleteSession(session.id)}
                      pinned={session.pinned}
                      archived={session.archived}
                    />
                  ))}
                </div>
              </div>
            )}

            {regularSessions.length > 0 && (
              <div className="nav-session-group nav-session-group-regular">
                <div className="nav-title">会话</div>
                <div className="nav-session-items">
                  {regularSessions.map((session) => (
                    <SessionSidebarItem
                      key={session.id}
                      active={activeItem === session.id}
                      editing={editingSessionId === session.id}
                      icon={<MessageSquare style={{ width: 14, height: 14 }} />}
                      title={session.title}
                      draftTitle={editingSessionTitle}
                      onClick={() => void openSession(session.id)}
                      onDoubleClick={() => startEditingSessionTitle(session)}
                      onDraftTitleChange={setEditingSessionTitle}
                      onCancelEdit={cancelEditingSessionTitle}
                      onSaveEdit={() => void saveSessionTitle()}
                      onPin={() => togglePinSession(session.id)}
                      onRename={() => startEditingSessionTitle(session)}
                      onArchive={() => archiveSession(session.id)}
                      onDelete={() => deleteSession(session.id)}
                      pinned={session.pinned}
                      archived={session.archived}
                    />
                  ))}
                </div>
              </div>
            )}
          </div>
        </div>

        <div className="sidebar-footer">
          <SidebarItem
            active={activeItem === 'settings'}
            compact={sidebarCollapsed}
            icon={<Settings style={{ width: 18, height: 18 }} />}
            label="设置"
            onClick={() => switchView('settings', 'settings')}
          />
        </div>
      </aside>

      <main className="main-content">
        {showChatUi && (
          <header className="top-bar chat-ui-element">
            <div className="top-model-selector">
              <CustomSelect
                options={selectorOptions}
                value={currentModelValue}
                onChange={(value) => {
                  if (value !== 'none') {
                    setSelectedModelId(value);
                  }
                }}
              />
            </div>
            <button
              className={`top-artifacts-toggle ${artifactsPanelOpen ? 'active' : ''}`}
              type="button"
              title={artifactsPanelOpen ? '关闭产物面板' : '打开产物面板'}
              onClick={() => setArtifactsPanelOpen((v) => !v)}
            >
              <Layers style={{ width: 18, height: 18 }} />
            </button>
          </header>
        )}

        {showChatUi ? (
          <>
            <section id="view-new" className={`view-container new-chat-view ${view === 'new' ? 'active' : ''}`}>
                <h1 className="greeting">有什么我可以帮您的吗？</h1>
                <div className="suggestion-grid">
                  <SuggestionCard title="数据分析与可视化" desc="上传 Excel 或 CSV 文件，我将为您进行深度分析并生成图表。" />
                  <SuggestionCard title="自动化研究报告" desc="输入一个主题，我将自动检索全网信息并撰写深度报告。" />
                  <SuggestionCard title="代码编写与审查" desc="提供您的需求，我将为您搭建工程骨架或查找代码漏洞。" />
                  <SuggestionCard title="全自动工作流" desc="描述您想要完成的复杂任务，我将自动规划步骤并执行。" />
                </div>
              </section>

              <section
                id="view-chat"
                ref={chatSectionRef}
                className={`view-container chat-history-view ${view === 'chat' ? 'active' : ''}`}
                onScroll={updateScrollState}
              >
                {isLoadingSessionDetail ? (
                  <MessagePanel messages={[]} emptyText="正在读取会话消息..." />
                ) : (
                  <MessagePanel
                    messages={activeSession?.messages ?? []}
                    emptyText={sessionError || (activeSession ? '这个会话还没有消息。' : '请选择一个本地会话。')}
                  />
                )}
                <div className="chat-scroll-anchor" />
              </section>

              {/* Timeline 导航 — 用户向上滚动时从右侧滑入 */}
              <nav
                className={`timeline-nav ${timelineVisible && timelineAnchors.length > 0 ? 'is-visible' : ''} ${artifactsPanelOpen ? 'timeline-compact' : ''}`}
                style={{ right: artifactsPanelOpen ? 332 : 32 }}
              >
                <ul className="timeline-list">
                  {timelineAnchors.map((anchor, index) => (
                    <li key={anchor.id} className="timeline-item">
                      <button
                        type="button"
                        className={`timeline-link ${index === activeTimelineIndex ? 'active' : ''}`}
                        onClick={() => jumpToAnchor(anchor.id)}
                        title={anchor.label}
                      >
                        <span className="timeline-dot" />
                        <span className="timeline-label">{anchor.label}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              </nav>

              <div className="input-container-wrapper chat-ui-element">
                {showScrollButton && (
                  <button
                    className="scroll-to-bottom-btn"
                    type="button"
                    onClick={handleScrollToBottom}
                    aria-label="滚动到底部"
                  >
                    <ArrowDown style={{ width: 20, height: 20 }} />
                  </button>
                )}
                <div style={{ width: '100%', maxWidth: 800 }}>
                  <div className={`input-box ${promptPanelOpen ? 'is-expanded' : ''}`}>
                    <textarea
                      ref={promptRef}
                      rows={1}
                      placeholder="输入指令，或者描述您想要 Agent 完成的复杂任务..."
                      value={message}
                      onChange={(event) => handlePromptChange(event.target.value)}
                      onKeyDown={(event) => {
                        if (event.key === 'Enter' && event.ctrlKey) {
                          event.preventDefault();
                          void handleSendMessage();
                        }
                      }}
                    />

                    <div className="input-actions">
                      <div className="action-group">
                        <button
                          className="icon-btn prompt-toggle-btn"
                          type="button"
                          title="更多选项"
                          aria-expanded={promptPanelOpen}
                          onClick={() => setPromptPanelOpen((current) => !current)}
                        >
                          <Plus style={{ width: 18, height: 18 }} />
                        </button>
                        <div className="reasoning-select-wrap">
                          <CustomSelect
                            options={reasoningOptions}
                            value={reasoningEffort}
                            onChange={setReasoningEffort}
                          />
                        </div>
                      </div>
                      <div className="send-group">
                        <span className="send-hint">{isCurrentSessionStreaming && message.trim().length === 0 ? '' : 'Ctrl+回车'}</span>
                        {isCurrentSessionStreaming && message.trim().length === 0 ? (
                          <button
                            className="icon-btn stop-btn"
                            type="button"
                            title="终止对话"
                            onClick={() => void handleCancelMessage()}
                          >
                            <Square style={{ width: 16, height: 16 }} />
                          </button>
                        ) : (
                          <button
                            className={`icon-btn send-btn ${message.trim().length === 0 ? 'disabled' : ''}`}
                            type="button"
                            disabled={message.trim().length === 0}
                            onClick={() => void handleSendMessage()}
                          >
                            <ArrowUp style={{ width: 18, height: 18 }} />
                          </button>
                        )}
                      </div>
                    </div>

                    <div className="feature-panel-wrapper">
                      <div className="feature-panel-inner">
                        <div className="feature-grid">
                          <PromptFeatureButton
                            icon={<Paperclip style={{ width: 20, height: 20 }} />}
                            label="上传附件"
                            onClick={() => attachmentInputRef.current?.click()}
                          />
                          <PromptFeatureButton
                            active={contextCompressionEnabled}
                            icon={<Shrink style={{ width: 20, height: 20 }} />}
                            label="压缩上下文"
                            onClick={toggleContextCompression}
                          />
                          <PromptFeatureButton
                            active={branchModeEnabled}
                            icon={<GitBranch style={{ width: 20, height: 20 }} />}
                            label="创建分支"
                            onClick={toggleBranchMode}
                          />
                          <PromptFeatureButton
                            active={targetModeEnabled}
                            icon={<Target style={{ width: 20, height: 20 }} />}
                            label="目标模式"
                            onClick={toggleTargetMode}
                          />
                        </div>
                      </div>
                    </div>

                    <input
                      ref={attachmentInputRef}
                      className="prompt-file-input"
                      type="file"
                      multiple
                      onChange={(event) => {
                        handleAttachmentChange(event.target.files);
                        event.target.value = '';
                      }}
                    />
                  </div>
                </div>
              </div>
          </>
        ) : view === 'settings' ? (
          <section id="view-settings" className="view-container settings-view active">
          <div className="settings-sidebar">
            <button className="settings-back-btn" type="button" onClick={goBackFromSettings}>
              <ArrowLeft style={iconSize} /> 返回
            </button>
            <button
              className={`settings-tab ${activeSettingsSection === 'general' ? 'active' : ''}`}
              type="button"
              onClick={() => setActiveSettingsSection('general')}
            >
              <Settings2 style={iconSize} /> 通用设置
            </button>
            <button
              className={`settings-tab ${activeSettingsSection === 'models' ? 'active' : ''}`}
              type="button"
              onClick={() => setActiveSettingsSection('models')}
            >
              <Cpu style={iconSize} /> 模型与引擎
            </button>
            <button
              className={`settings-tab ${activeSettingsSection === 'api' ? 'active' : ''}`}
              type="button"
              onClick={() => setActiveSettingsSection('api')}
            >
              <Key style={iconSize} /> API 密钥
            </button>
            <button
              className={`settings-tab ${activeSettingsSection === 'storage' ? 'active' : ''}`}
              type="button"
              onClick={() => setActiveSettingsSection('storage')}
            >
              <Folder style={iconSize} /> 存储配置
            </button>
            <button
              className={`settings-tab ${activeSettingsSection === 'knowledge' ? 'active' : ''}`}
              type="button"
              onClick={() => setActiveSettingsSection('knowledge')}
            >
              <Database style={iconSize} /> 本地知识库
            </button>
          </div>

          <div className="settings-body">
            {activeSettingsSection === 'general' && (
              <>
                <div className="settings-title">通用设置</div>

                <div className="setting-item">
                  <div className="setting-item-info">
                    <span>主题外观</span>
                    <small>切换 otherone 的显示模式</small>
                  </div>
                  <div>
                    <button id="theme-toggle-btn" className="setting-btn" type="button" onClick={toggleTheme}>
                      <span style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                        {theme === 'dark' ? (
                          <>
                            <SunMoon style={iconSize} /> 切换到浅色模式
                          </>
                        ) : (
                          <>
                            <Moon style={iconSize} /> 切换到深色模式
                          </>
                        )}
                      </span>
                    </button>
                  </div>
                </div>
              </>            )}

            <div className={activeSettingsSection === 'api' ? 'settings-panel-active' : 'settings-panel-hidden'}>
              <ModelSettings
                isSaving={isSavingConfigs}
                onProvidersChange={setProviders}
                onSave={saveApiConfigs}
                onTestProvider={handleTestProvider}
                providers={providers}
                storageStatus={storageStatus}
                testingProviderId={testingProviderId}
              />
            </div>

            {activeSettingsSection === 'storage' && (
              <>
                <div className="settings-title">存储配置</div>
                <div className="settings-warning">
                  迁移会复制当前受管数据并在校验成功后删除旧目录中的受管数据。迁移或清理过程中如果出现数据丢失，应用无法恢复旧数据，请先手动备份当前目录。
                </div>

                <div className="setting-item setting-item-column">
                  <div className="setting-item-info">
                    <span>数据存储路径</span>
                    <small>localfile 会保存 Agent 上下文，SQLite 会保存配置、会话索引和应用元数据。</small>
                  </div>
                  <div className="storage-path-panel">
                    <input
                      className="storage-path-input"
                      value={storageDraft}
                      onChange={(event) => {
                        setStorageDraft(event.target.value);
                        setMigrationStatus('路径已修改，保存后会触发数据迁移。');
                      }}
                    />
                    <div className="storage-path-actions">
                      <button className="setting-btn" type="button" onClick={chooseStoragePath}>
                        选择目录
                      </button>
                      <button className="setting-btn" type="button" disabled={isMigratingStorage} onClick={saveStoragePath}>
                        {isMigratingStorage ? '迁移中' : '保存并迁移'}
                      </button>
                    </div>
                    <small className="storage-path-status">
                      当前路径：{storagePath}
                      <br />
                      {migrationStatus}
                    </small>
                  </div>
                </div>

                <div className="setting-item setting-item-column">
                  <div className="setting-item-info">
                    <span>产物存储位置</span>
                    <small>Agent 运行过程中生成的图片、文件、报告等产物的保存目录。</small>
                  </div>
                  <div className="storage-path-panel">
                    <input
                      className="storage-path-input"
                      value={artifactDraft}
                      onChange={(event) => setArtifactDraft(event.target.value)}
                    />
                    <div className="storage-path-actions">
                      <button className="setting-btn" type="button" onClick={chooseArtifactPath}>
                        选择目录
                      </button>
                      <button className="setting-btn" type="button" disabled={isMigratingStorage} onClick={saveArtifactPath}>
                        {isMigratingStorage ? '迁移中' : '保存'}
                      </button>
                    </div>
                    <small className="storage-path-status">
                      当前路径：{artifactPath}
                    </small>
                  </div>
                </div>

                <div className="setting-item setting-item-column">
                  <div className="setting-item-info">
                    <span>对话数据存储位置</span>
                    <small>所有聊天记录、会话历史与消息附件的持久化存储目录。</small>
                  </div>
                  <div className="storage-path-panel">
                    <input
                      className="storage-path-input"
                      value={dialogueDataDraft}
                      onChange={(event) => setDialogueDataDraft(event.target.value)}
                    />
                    <div className="storage-path-actions">
                      <button className="setting-btn" type="button" onClick={chooseDialogueDataPath}>
                        选择目录
                      </button>
                      <button className="setting-btn" type="button" disabled={isMigratingStorage} onClick={saveDialogueDataPath}>
                        {isMigratingStorage ? '迁移中' : '保存'}
                      </button>
                    </div>
                    <small className="storage-path-status">
                      当前路径：{dialogueDataPath}
                    </small>
                  </div>
                </div>

                <div className="setting-item">
                  <div className="setting-item-info">
                    <span>清空所有数据</span>
                    <small style={{ color: 'var(--danger-color)' }}>将删除所有工作流、对话历史及本地缓存</small>
                  </div>
                  <div>
                    <button className="setting-btn setting-btn-danger" type="button">
                      全部清除
                    </button>
                  </div>
                </div>
              </>
            )}

            {activeSettingsSection === 'models' && (
              <EngineSettingsPanel
                engine={engineSettings}
                isSaving={isSavingEngine}
                modelOptions={modelOptions}
                onChange={setEngineSettings}
                onSave={() => void saveEngineSettings()}
              />
            )}

            {activeSettingsSection === 'knowledge' && (
              <div className="settings-empty-panel">
                <div className="settings-title">本地知识库</div>
                <p>知识库索引、文件导入和本地检索配置会在后续模块中实现。</p>
              </div>
            )}
          </div>
        </section>
        ) : view === 'workflow' ? (
          <WorkflowPage onClose={() => switchView('new')} />
        ) : (
          <PluginsPage onClose={() => switchView('new')} />
        )}
      </main>

      <ArtifactsPanel
        addedFiles={addedFiles}
        deletedFiles={deletedFiles}
        editedFiles={editedFiles}
        open={artifactsPanelOpen}
      />

      <ToastViewport messages={toasts} onDismiss={dismissToast} />
      <SearchOverlay
        allArtifacts={allArtifacts}
        open={searchOpen}
        sessions={sessions}
        onClose={() => setSearchOpen(false)}
        onOpenSession={(sessionId) => {
          void openSession(sessionId);
        }}
      />
    </div>
  );
}

type SidebarItemProps = {
  active?: boolean;
  compact?: boolean;
  icon: ReactNode;
  label: string;
  onClick?: () => void;
};

function createClientSessionId() {
  return `session-${Date.now()}-${Math.floor(Math.random() * 1_000_000)}`;
}

function SidebarItem({ active = false, compact = false, icon, label, onClick }: SidebarItemProps) {
  return (
    <button className={`sidebar-item ${active ? 'active' : ''}`} type="button" title={compact ? label : undefined} onClick={onClick}>
      {icon}
      <span>{label}</span>
    </button>
  );
}

type SessionSidebarItemProps = {
  active: boolean;
  editing: boolean;
  icon: ReactNode;
  title: string;
  draftTitle: string;
  onClick: () => void;
  onDoubleClick: () => void;
  onDraftTitleChange: (value: string) => void;
  onCancelEdit: () => void;
  onSaveEdit: () => void;
  onPin: () => void;
  onRename: () => void;
  onArchive: () => void;
  onDelete: () => void;
  pinned: boolean;
  archived: boolean;
};

function SessionSidebarItem({
  active,
  editing,
  icon,
  title,
  draftTitle,
  onClick,
  onDoubleClick,
  onDraftTitleChange,
  onCancelEdit,
  onSaveEdit,
  onPin,
  onRename,
  onArchive,
  onDelete,
  pinned,
  archived,
}: SessionSidebarItemProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!menuOpen) return;
    const handler = (e: PointerEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
      }
    };
    document.addEventListener('pointerdown', handler);
    return () => document.removeEventListener('pointerdown', handler);
  }, [menuOpen]);

  if (editing) {
    return (
      <form
        className={`sidebar-item session-title-editor ${active ? 'active' : ''}`}
        onSubmit={(event) => {
          event.preventDefault();
          onSaveEdit();
        }}
      >
        {icon}
        <input
          autoFocus
          value={draftTitle}
          onBlur={onSaveEdit}
          onChange={(event) => onDraftTitleChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              event.preventDefault();
              onCancelEdit();
            }
          }}
        />
      </form>
    );
  }

  return (
    <div className="session-item-wrapper">
      <button
        className={`sidebar-item session-item-btn ${active ? 'active' : ''}`}
        type="button"
        onClick={onClick}
        onDoubleClick={onDoubleClick}
      >
        {icon}
        <span>{title}</span>
      </button>

      <div className="session-hover-actions">
        <button
          className="session-action-btn"
          type="button"
          aria-label={pinned ? '取消置顶' : '置顶'}
          title={pinned ? '取消置顶' : '置顶'}
          onClick={(e) => {
            e.stopPropagation();
            onPin();
          }}
        >
          <Pin style={{ width: 13, height: 13, fill: pinned ? 'currentColor' : 'none' }} />
        </button>

        <div className="session-menu-wrap" ref={menuRef}>
          <button
            className="session-action-btn"
            type="button"
            aria-label="更多操作"
            title="更多操作"
            onClick={(e) => {
              e.stopPropagation();
              setMenuOpen((v) => !v);
            }}
          >
            <MoreHorizontal style={{ width: 14, height: 14 }} />
          </button>

          <div className={`session-context-menu ${menuOpen ? 'is-open' : ''}`}>
            <button
              className="session-context-menu-item"
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                setMenuOpen(false);
                onRename();
              }}
            >
              重命名会话
            </button>
            <button
              className="session-context-menu-item"
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                setMenuOpen(false);
                onArchive();
              }}
            >
              {archived ? '取消归档' : '归档会话'}
            </button>
            <div className="session-context-menu-separator" />
            <button
              className="session-context-menu-item danger"
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                setMenuOpen(false);
                onDelete();
              }}
            >
              <Trash2 style={{ width: 13, height: 13 }} />
              删除会话
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

type SuggestionCardProps = {
  title: string;
  desc: string;
};

function SuggestionCard({ title, desc }: SuggestionCardProps) {
  return (
    <button className="suggestion-card" type="button">
      <span className="title">{title}</span>
      <span className="desc">{desc}</span>
    </button>
  );
}

type PromptFeatureButtonProps = {
  active?: boolean;
  icon: ReactNode;
  label: string;
  onClick: () => void;
};

function PromptFeatureButton({ active = false, icon, label, onClick }: PromptFeatureButtonProps) {
  return (
    <button className={`feature-item ${active ? 'active' : ''}`} type="button" onClick={onClick}>
      <span className="feature-icon-box">{icon}</span>
      <span className="feature-name">{label}</span>
    </button>
  );
}
