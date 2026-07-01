import {
  Archive,
  ArrowDown,
  ArrowLeft,
  ArrowUp,
  Blocks,
  BotMessageSquare,
  Cpu,
  Database,
  Folder,
  FolderOpen,
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
import { useCallback, useEffect, useMemo, useRef, useState, useDeferredValue } from 'react';
import { CustomSelect } from './components/CustomControls';
import { MessagePanel } from './components/MessagePanel';
import { ModelSettings } from './components/ModelSettings';
import { EngineSettingsPanel } from './components/EngineSettingsPanel';
import { PluginsPage } from './components/PluginsPage';
import { PersonalizationPage } from './components/PersonalizationPage';
import { WorkflowPage } from './components/WorkflowPage';
import { WeixinClawbotPage } from './components/WeixinClawbotPage';
import { SearchOverlay } from './components/SearchOverlay';
import { ArtifactsPanel, type FileArtifact } from './components/ArtifactsPanel';
import { ConfirmDialog } from './components/ConfirmDialog';
import { ToastViewport, type ToastKind, type ToastNotice } from './components/ToastSystem';
import { WindowTitleBar } from './components/WindowTitleBar';
import { defaultApiConfigs } from './data/defaultApiConfigs';
import { loadApiConfigsFromStorage, saveApiConfigsToStorage } from './services/apiConfigStorage';
import {
  clearAllOtheroneDataFromStorage,
  loadAppSettingsFromStorage,
  migrateStorageSettingsToStorage,
  openDirectoryInSystem,
  revealFileInSystem,
  saveEngineSettingsToStorage,
  selectDirectoryFromSystem,
} from './services/appSettingsStorage';
import { testAiModel } from './services/aiModelTest';
import { VirtuosoHandle } from 'react-virtuoso';
import { listFileArtifacts, listenToFileArtifacts, type FileArtifactRecord } from './services/artifactStorage';
import { cancelChatMessage, enqueueChatMessageToStorage, listenToChatStream, sendChatMessageToStorage, type ChatStreamEvent } from './services/chatStorage';
import { loadSessionsFromStorage, readSessionFromStorage, updateSessionTitleInStorage } from './services/sessionStorage';
import { isDesktopRuntime } from './services/platform/runtime';
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

type ViewName = 'new' | 'chat' | 'settings' | 'workflow' | 'plugins' | 'personalization' | 'weixinClawbot';
type ThemeName = 'dark' | 'light';
type SettingsSection = 'general' | 'models' | 'api' | 'storage' | 'knowledge';
type StorageRootKey = keyof StorageSettings;
type PendingStorageMigration = {
  changedKeys: StorageRootKey[];
  storage: StorageSettings;
  targetKey: StorageRootKey;
};
type StreamItemOverlay = Array<MessageItem[] | undefined>;
type PendingChatSend = {
  sessionId: string;
  aiGroupId: string;
  userGroupIds: string[];
  prompts: string[];
  timer: ReturnType<typeof setTimeout> | null;
  previousSession: SessionDetail | null;
  previousSummary: SessionSummary | null;
  previousSummaryIndex: number;
  modelId: string;
  reasoningEffort: ReasoningEffort;
  contextCompressionEnabled: boolean;
  branchModeEnabled: boolean;
  targetModeEnabled: boolean;
  memoryEnabled: boolean;
};

const iconSize = { width: 16, height: 16 };
const SEND_REMORSE_DELAY_MS = 3000;
const noModelOption = { label: '未配置模型', value: 'none' };

const reasoningOptions: Array<{ label: string; value: ReasoningEffort }> = [
  { label: '不要思考', value: 'none' },
  { label: 'High', value: 'high' },
  { label: 'Medium', value: 'medium' },
  { label: 'Low', value: 'low' },
];

const streamAiGroupId = (sessionId: string, turnId = 'current') => `stream-ai-${sessionId}-${turnId}`;

// 每个 Agent loop 迭代一组独立的 thinking + text + tool item
const streamThinkingId = (groupId: string, iter: number) => `${groupId}-thinking-${iter}`;
const streamTextId = (groupId: string, iter: number) => `${groupId}-text-${iter}`;
const streamToolPrefix = (groupId: string, iter: number) => `${groupId}-tool-${iter}-`;
const streamToolId = (groupId: string, iter: number, idx: number) => `${groupId}-tool-${iter}-${idx}`;
const streamToolItemStatus = (event: ChatStreamEvent) =>
  event.toolStatus === 'completed' || event.toolStatus === 'error' ? 'completed' as const : 'running' as const;

function createOptimisticUserGroup(sessionId: string, prompt: string, createdAt: string) {
  const turnId = `${Date.now()}-${Math.floor(Math.random() * 1_000_000)}`;
  const userGroupId = `stream-user-${sessionId}-${turnId}`;
  const userGroup: MessageGroup = {
    id: userGroupId,
    role: 'user',
    items: [
      {
        id: `stream-user-item-${userGroupId}`,
        type: 'text',
        content: prompt,
        status: 'completed',
        entryId: '',
        sourceRole: 'user',
        createdAt,
      },
    ],
  };

  return { turnId, userGroupId, userGroup };
}

function createOptimisticAiGroup(aiGroupId: string, createdAt: string): MessageGroup {
  return {
    id: aiGroupId,
    role: 'ai',
    items: [
      {
        id: streamTextId(aiGroupId, 0),
        type: 'text',
        content: '',
        status: 'running',
        entryId: '',
        sourceRole: 'assistant',
        createdAt,
      },
    ],
  };
}

function insertUserGroupBeforeAiGroup(
  session: SessionDetail,
  userGroup: MessageGroup,
  aiGroupId: string,
  updatedAt: string,
): SessionDetail {
  const aiGroupIndex = session.messages.findIndex((group) => group.id === aiGroupId);
  const messages =
    aiGroupIndex >= 0
      ? [...session.messages.slice(0, aiGroupIndex), userGroup, ...session.messages.slice(aiGroupIndex)]
      : [...session.messages, userGroup];

  return { ...session, updatedAt, messages };
}

function completeStreamItemsFromGroup(
  session: SessionDetail | null | undefined,
  groupId: string | undefined,
): MessageItem[] {
  if (!session || !groupId) return [];

  const group = session.messages.find((message) => message.id === groupId);
  if (!group) return [];

  return completeStreamItems(group.items);
}

function completeStreamItems(items: MessageItem[]): MessageItem[] {
  return items
    .filter((item) => item.type !== 'thinking' || Boolean(item.content))
    .map((item) => {
      if (item.type === 'thinking') {
        return {
          ...item,
          status: 'completed' as const,
          label: '深度思考已完成',
        };
      }

      return {
        ...item,
        status: 'completed' as const,
      };
    });
}

function completeAiGroupInSession(session: SessionDetail, groupId: string, updatedAt: string): SessionDetail {
  const groupIndex = session.messages.findIndex((group) => group.id === groupId);
  if (groupIndex < 0) return session;

  const group = session.messages[groupIndex];
  if (group.role !== 'ai') return session;

  const messages = [...session.messages];
  messages[groupIndex] = {
    ...group,
    items: completeStreamItems(group.items),
  };

  return { ...session, updatedAt, messages };
}

function isEmptyRunningAiGroup(session: SessionDetail | null | undefined, groupId: string | undefined) {
  if (!session || !groupId) return false;
  const group = session.messages.find((message) => message.id === groupId);
  if (!group || group.role !== 'ai') return false;

  return group.items.every((item) => {
    if (item.type === 'text') return !item.content;
    if (item.type === 'thinking') return !item.content;
    return false;
  });
}

function collectStreamItemOverlay(session: SessionDetail | null | undefined): StreamItemOverlay {
  const overlay: StreamItemOverlay = [];
  if (!session) return overlay;

  let aiIndex = -1;
  for (const group of session.messages) {
    if (group.role !== 'ai') continue;

    aiIndex += 1;
    const items = completeStreamItems(group.items);
    if (items.some((item) => item.type === 'thinking')) {
      overlay[aiIndex] = items;
    }
  }

  return overlay;
}

function hasStreamItemOverlay(overlay: StreamItemOverlay | null | undefined) {
  return Boolean(overlay?.some((items) => items?.some((item) => item.type === 'thinking')));
}

function mergeStreamItemOverlays(
  current: StreamItemOverlay | null | undefined,
  incoming: StreamItemOverlay | null | undefined,
): StreamItemOverlay {
  const next = [...(current ?? [])];
  incoming?.forEach((items, index) => {
    if (items?.some((item) => item.type === 'thinking')) {
      next[index] = items;
    }
  });
  return next;
}

function mergeStreamItemsIntoStoredSession(
  session: SessionDetail,
  cachedSession: SessionDetail | null | undefined,
  activeStreamGroupId?: string,
  activeStreamItems?: MessageItem[],
  streamItemOverlay?: StreamItemOverlay,
): SessionDetail {
  const cachedAiGroups = cachedSession?.messages.filter((group) => group.role === 'ai') ?? [];
  let aiIndex = -1;
  let activeStreamItemsUsed = false;

  const messages = session.messages.map((group) => {
    if (group.role !== 'ai') return group;

    aiIndex += 1;
    const cachedGroup = cachedAiGroups[aiIndex];
    const cachedItems = cachedGroup?.items ?? [];
    const overlayItems = streamItemOverlay?.[aiIndex];
    const streamItems =
      activeStreamItems && cachedGroup?.id === activeStreamGroupId
        ? activeStreamItems
        : cachedItems.some((item) => item.type === 'thinking')
          ? cachedItems
          : overlayItems ?? cachedItems;

    if (activeStreamItems && cachedGroup?.id === activeStreamGroupId) {
      activeStreamItemsUsed = true;
    }

    if (!streamItems.some((item) => item.type === 'thinking')) return group;

    return {
      ...group,
      items: mergeStreamItemsIntoStoredItems(group.items, completeStreamItems(streamItems)),
    };
  });

  if (activeStreamItems && !activeStreamItemsUsed && activeStreamItems.some((item) => item.type === 'thinking')) {
    for (let index = messages.length - 1; index >= 0; index -= 1) {
      const group = messages[index];
      if (group.role !== 'ai') continue;

      messages[index] = {
        ...group,
        items: mergeStreamItemsIntoStoredItems(group.items, completeStreamItems(activeStreamItems)),
      };
      break;
    }
  }

  return { ...session, messages };
}

function mergeStreamItemsIntoStoredItems(
  storedItems: MessageItem[],
  streamItems: MessageItem[],
): MessageItem[] {
  const storedNonThinkingItems = storedItems.filter((item) => item.type !== 'thinking');
  const usedStoredIndexes = new Set<number>();
  const mergedItems: MessageItem[] = [];

  const takeStoredItem = (type: MessageItem['type']) => {
    const index = storedNonThinkingItems.findIndex(
      (item, itemIndex) => item.type === type && !usedStoredIndexes.has(itemIndex),
    );
    if (index < 0) return null;
    usedStoredIndexes.add(index);
    return storedNonThinkingItems[index];
  };

  for (const streamItem of streamItems) {
    if (streamItem.type === 'thinking') {
      if (streamItem.content) {
        mergedItems.push(streamItem);
      }
      continue;
    }

    const storedItem = takeStoredItem(streamItem.type);
    if (storedItem) {
      mergedItems.push(storedItem);
    }
  }

  storedNonThinkingItems.forEach((item, index) => {
    if (!usedStoredIndexes.has(index)) {
      mergedItems.push(item);
    }
  });

  return mergedItems.length > 0 ? mergedItems : storedItems;
}

/**
 * 优化版：仅更新匹配的事件 AI group，其他 group 保持引用不变。
 * 配合 React.memo，非流式消息组件跳过 re-render。
 */
function applyStreamEventToSession(
  session: SessionDetail,
  event: ChatStreamEvent,
  iterMap: Map<string, number>,
  streamGroupMap: Map<string, string>,
): SessionDetail {
  const createdAt = new Date().toISOString();
  const eventType = event.eventType === 'thinking' ? 'assistant_thinking_delta' : event.eventType;
  const aiGroupId = streamGroupMap.get(event.sessionId) ?? streamAiGroupId(event.sessionId);

  // --- iteration 管理 ---
  const getIter = () => iterMap.get(event.sessionId) ?? 0;
  const bumpIter = () => { const n = getIter() + 1; iterMap.set(event.sessionId, n); };

  const makeThinking = (iter: number): ThinkingMessageItem => ({
    id: streamThinkingId(aiGroupId, iter),
    type: 'thinking',
    label: '正在深度思考',
    content: '',
    status: 'running',
    entryId: '',
    sourceRole: 'assistant-thinking',
    createdAt,
  });

  const makeText = (iter: number): TextMessageItem => ({
    id: streamTextId(aiGroupId, iter),
    type: 'text',
    content: '',
    status: 'running',
    entryId: '',
    sourceRole: 'assistant',
    createdAt,
  });

  const completeCurrentIterationItems = (sourceItems: MessageItem[], currentIter: number) => {
    const thinkId = streamThinkingId(aiGroupId, currentIter);
    const textId = streamTextId(aiGroupId, currentIter);
    const toolPrefix = streamToolPrefix(aiGroupId, currentIter);

    return sourceItems.map((item) => {
      if (item.id === thinkId && item.type === 'thinking' && item.status === 'running') {
        return {
          ...item,
          label: '深度思考已完成',
          status: 'completed' as const,
        } as ThinkingMessageItem;
      }

      if (item.id === textId && item.type === 'text' && item.status === 'running') {
        return {
          ...item,
          status: 'completed' as const,
        } as TextMessageItem;
      }

      if (item.id.startsWith(toolPrefix) && item.type === 'tool' && item.status === 'running') {
        return {
          ...item,
          status: 'completed' as const,
        } as ToolMessageItem;
      }

      return item;
    });
  };

  // 快速查找已存在的 AI group
  const aiGroupIdx = session.messages.findIndex((g) => g.id === aiGroupId);

  if (aiGroupIdx < 0) {
    // ── 首次事件，创建新的 AI group ──
    let items: MessageItem[] = [];
    const iter = getIter();

    if (eventType === 'assistant_thinking_delta') {
      items.push({ ...makeThinking(iter), content: event.content });
    }

    if (eventType === 'tool_call') {
      items.push({
        id: streamToolId(aiGroupId, iter, 0),
        type: 'tool',
        label: event.toolLabel || '工具调用',
        status: streamToolItemStatus(event),
        entryId: '',
        sourceRole: 'assistant',
        createdAt,
        detail: event.toolDetail,
      } as ToolMessageItem);
    }

    if (eventType === 'assistant_delta' || eventType === 'complete' || eventType === 'error') {
      items.push({
        ...makeText(iter),
        content: eventType === 'assistant_delta' ? event.content : eventType === 'complete' ? event.content : (event.error ?? ''),
        status: eventType === 'assistant_delta' ? 'running' as const : 'completed' as const,
      });
    }

    if (items.length === 0) return session;

    return {
      ...session,
      updatedAt: createdAt,
      messages: [...session.messages, { id: aiGroupId, role: 'ai' as const, items }],
    };
  }

  // ── 已有 AI group：仅更新这一个 group ──
  const group = session.messages[aiGroupIdx];
  let items = group.items;
  let iter = getIter();

  if (
    (eventType === 'assistant_thinking_delta' || eventType === 'assistant_delta') &&
    items.some((item) => item.type === 'tool' && item.id.startsWith(streamToolPrefix(aiGroupId, iter)))
  ) {
    items = completeCurrentIterationItems(items, iter);
    bumpIter();
    iter = getIter();
  }

  // ── assistant_thinking_delta ──
  if (eventType === 'assistant_thinking_delta') {
    const thinkId = streamThinkingId(aiGroupId, iter);
    const exists = items.some((i) => i.id === thinkId);
    if (!exists) {
      const textId = streamTextId(aiGroupId, iter);
      const textIdx = items.findIndex((i) => i.id === textId);
      items = textIdx >= 0
        ? [...items.slice(0, textIdx), makeThinking(iter), ...items.slice(textIdx)]
        : [...items, makeThinking(iter)];
    }
    items = items.map((i) =>
      i.id === thinkId && i.type === 'thinking'
        ? { ...i, content: `${(i as ThinkingMessageItem).content}${event.content}`, status: 'running' as const }
        : i,
    );
  }

  // ── assistant_delta ──
  if (eventType === 'assistant_delta') {
    const textId = streamTextId(aiGroupId, iter);
    const exists = items.some((i) => i.id === textId);
    if (!exists) {
      const thinkId = streamThinkingId(aiGroupId, iter);
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
  if (eventType === 'tool_call') {
    items = completeCurrentIterationItems(items, iter);
    const toolId = streamToolId(
      aiGroupId,
      iter,
      items.filter((item) => item.type === 'tool' && item.id.startsWith(streamToolPrefix(aiGroupId, iter))).length,
    );
    const toolItem: ToolMessageItem = {
      id: toolId,
      type: 'tool',
      label: event.toolLabel || '工具调用',
      status: streamToolItemStatus(event),
      entryId: '',
      sourceRole: 'assistant',
      createdAt,
      detail: event.toolDetail,
    };
    const textId = streamTextId(aiGroupId, iter);
    const textIdx = items.findIndex((i) => i.id === textId);
    const textItem = textIdx >= 0 && items[textIdx].type === 'text' ? (items[textIdx] as TextMessageItem) : null;
    if (textIdx >= 0 && textItem?.content) {
      items = [...items.slice(0, textIdx + 1), toolItem, ...items.slice(textIdx + 1)];
    } else if (textIdx >= 0) {
      items = [...items.slice(0, textIdx), ...items.slice(textIdx + 1), toolItem];
    } else {
      items = [...items, toolItem];
    }
  }

  // ── complete / error / cancelled ──
  if (eventType === 'complete' || eventType === 'error' || eventType === 'cancelled') {
    items = items.map((i) => {
      if (i.status !== 'running') return i;
      if (i.type === 'thinking') {
        return {
          ...i,
          label: eventType === 'cancelled' ? '对话已被用户中断' : eventType === 'complete' ? '深度思考已完成' : '深度思考已中断',
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
    iterMap.set(event.sessionId, 0);
  }

  if (items === group.items) return session; // 无变化则返回原对象

  // 仅替换变化的那个 group，其余保持引用
  const newMessages = [...session.messages];
  newMessages[aiGroupIdx] = { ...group, items };

  return { ...session, updatedAt: createdAt, messages: newMessages };
}

function artifactRecordToFileArtifact(record: FileArtifactRecord): FileArtifact {
  return {
    id: record.id,
    name: record.name,
    path: record.path,
    extension: record.extension,
    timestamp: record.createdAt,
  };
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
  const [memoryModelId, setMemoryModelId] = useState('');
  const [memoryEnabled, setMemoryEnabled] = useState(true);
  const [isSavingConfigs, setIsSavingConfigs] = useState(false);
  const [isSavingEngine, setIsSavingEngine] = useState(false);
  const [isMigratingStorage, setIsMigratingStorage] = useState(false);
  const [isClearingAllData, setIsClearingAllData] = useState(false);
  const [storageStatus, setStorageStatus] = useState('配置尚未保存。');
  const [storagePath, setStoragePath] = useState('C:\\Users\\jae\\AppData\\Roaming\\Otherone');
  const [storageDraft, setStorageDraft] = useState(storagePath);
  const [storageMigrationStatus, setStorageMigrationStatus] = useState('当前使用 localfile 与 SQLite 组合存储。');
  const [artifactPath, setArtifactPath] = useState('C:\\Users\\jae\\AppData\\Roaming\\Otherone\\artifacts');
  const [artifactDraft, setArtifactDraft] = useState(artifactPath);
  const [artifactMigrationStatus, setArtifactMigrationStatus] = useState('当前产物目录用于保存 Agent 运行产物。');
  const [dialogueDataPath, setDialogueDataPath] = useState('C:\\Users\\jae\\AppData\\Roaming\\Otherone\\dialogues');
  const [dialogueDataDraft, setDialogueDataDraft] = useState(dialogueDataPath);
  const [dialogueMigrationStatus, setDialogueMigrationStatus] = useState('当前对话目录用于保存 otherone localfile 数据。');
  const [pendingStorageMigration, setPendingStorageMigration] = useState<PendingStorageMigration | null>(null);
  const [pendingClearAllData, setPendingClearAllData] = useState(false);
  const [engineSettings, setEngineSettings] = useState(defaultEngineSettings);
  const [testingProviderId, setTestingProviderId] = useState('');
  const [reasoningEffort, setReasoningEffort] = useState<ReasoningEffort>('medium');
  const [promptPanelOpen, setPromptPanelOpen] = useState(false);
  const [contextCompressionEnabled, setContextCompressionEnabled] = useState(false);
  const [branchModeEnabled, setBranchModeEnabled] = useState(false);
  const [targetModeEnabled, setTargetModeEnabled] = useState(false);
  const [artifactsPanelOpen, setArtifactsPanelOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [editedFiles, setEditedFiles] = useState<FileArtifact[]>([]);
  const [deletedFiles, setDeletedFiles] = useState<FileArtifact[]>([]);
  const [addedFiles, setAddedFiles] = useState<FileArtifact[]>([]);
  const [toasts, setToasts] = useState<ToastNotice[]>([]);
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [activeSession, setActiveSession] = useState<SessionDetail | null>(null);
  const [isLoadingSessions, setIsLoadingSessions] = useState(false);
  const [isLoadingSessionDetail, setIsLoadingSessionDetail] = useState(false);
  const streamingSessionIdsRef = useRef<Set<string>>(new Set());
  const sessionCacheRef = useRef<Map<string, SessionDetail>>(new Map());
  const streamItemOverlayRef = useRef<Map<string, StreamItemOverlay>>(new Map());
  const pendingChatSendsRef = useRef<Map<string, PendingChatSend>>(new Map());
  const activeSessionIdRef = useRef<string | null>(null);
  const [sessionError, setSessionError] = useState('');
  const [editingSessionId, setEditingSessionId] = useState('');
  const [editingSessionTitle, setEditingSessionTitle] = useState('');
  const attachmentInputRef = useRef<HTMLInputElement | null>(null);
  const promptRef = useRef<HTMLTextAreaElement | null>(null);
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const toastCounterRef = useRef(0);
  const streamIterRef = useRef<Map<string, number>>(new Map());
  const activeStreamGroupIdRef = useRef<Map<string, string>>(new Map());
  const queuedStreamAiGroupIdRef = useRef<Map<string, string>>(new Map());
  const [showScrollButton, setShowScrollButton] = useState(false);
  const timelineHideTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingStreamEventsRef = useRef<Map<string, ChatStreamEvent[]>>(new Map());
  const previousView = useRef<ViewName>('new');

  const hasRunningAiItem = useMemo(() => {
    if (!activeSession) return false;
    return activeSession.messages.some((group) =>
      group.role === 'ai' && group.items.some((item) => item.status === 'running'),
    );
  }, [activeSession]);
  const isCurrentSessionStreaming = activeSession != null && streamingSessionIdsRef.current.has(activeSession.id);
  const isAiStreaming = isCurrentSessionStreaming || hasRunningAiItem;

  // 流式期间用 useDeferredValue 延迟渲染，保证用户输入/点击不被阻塞
  const rawMessages = activeSession?.messages ?? [];
  const displayMessages = useDeferredValue(rawMessages);

  const rememberStreamItemOverlay = useCallback((sessionId: string, session: SessionDetail | null | undefined) => {
    const incoming = collectStreamItemOverlay(session);
    if (!hasStreamItemOverlay(incoming)) return;

    const current = streamItemOverlayRef.current.get(sessionId);
    streamItemOverlayRef.current.set(sessionId, mergeStreamItemOverlays(current, incoming));
  }, []);

  const setStorageRootStatus = (key: StorageRootKey, message: string) => {
    if (key === 'dataRoot') {
      setStorageMigrationStatus(message);
      return;
    }

    if (key === 'artifactRoot') {
      setArtifactMigrationStatus(message);
      return;
    }

    setDialogueMigrationStatus(message);
  };

  const setChangedStorageRootStatuses = (keys: StorageRootKey[], message: string) => {
    keys.forEach((key) => setStorageRootStatus(key, message));
  };

  // Timeline 导航：收集所有用户 prompt 作为锚点（含 messages 数组索引用于 Virtuoso scrollToIndex）
  const timelineAnchors = useMemo(() => {
    if (!activeSession) return [];
    const anchors: Array<{ id: string; label: string; messageIndex: number }> = [];
    activeSession.messages.forEach((group, idx) => {
      if (group.role !== 'user') return;
      const textItem = group.items.find((item) => item.type === 'text');
      const preview = (textItem?.content ?? '新对话').slice(0, 32);
      anchors.push({ id: group.id, label: preview || '新对话', messageIndex: idx });
    });
    return anchors;
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
        setStorageMigrationStatus('已从本地设置读取数据存储路径。');
        setArtifactMigrationStatus('已从本地设置读取产物存储路径。');
        setDialogueMigrationStatus('已从本地设置读取对话存储路径。');
      } catch (error) {
        if (!cancelled) {
          const message = error instanceof Error ? error.message : String(error);
          setStorageMigrationStatus(message);
          setArtifactMigrationStatus(message);
          setDialogueMigrationStatus(message);
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
    if (modelOptions.length === 0) {
      setMemoryModelId('');
      return;
    }

    const selectedStillExists = modelOptions.some((model) => model.id === memoryModelId);
    if (selectedStillExists) {
      return;
    }

    const selectedChatModel = modelOptions.find((model) => model.id === selectedModelId);
    const defaultModel = providers.flatMap((provider) => provider.models).find((model) => model.defaultModel);
    setMemoryModelId(selectedChatModel?.id ?? defaultModel?.id ?? modelOptions[0].id);
  }, [memoryModelId, modelOptions, providers, selectedModelId]);

  useEffect(() => {
    document.documentElement.style.colorScheme = theme;

    let themeMeta = document.querySelector<HTMLMetaElement>('meta[name="theme-color"]');
    if (!themeMeta) {
      themeMeta = document.createElement('meta');
      themeMeta.name = 'theme-color';
      document.head.appendChild(themeMeta);
    }
    themeMeta.content = theme === 'dark' ? '#000000' : '#ffffff';

    if (isDesktopRuntime()) {
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

  // ---- Virtuoso 回调：处理底部状态变化 (替代手动 onScroll + wheel 监听) ----
  const handleBottomStateChange = useCallback((atBottom: boolean) => {
    setShowScrollButton(!atBottom);
    if (atBottom) {
      setTimelineVisible(false);
      if (timelineHideTimerRef.current) {
        clearTimeout(timelineHideTimerRef.current);
        timelineHideTimerRef.current = null;
      }
    } else {
      setTimelineVisible(true);
      if (timelineHideTimerRef.current) {
        clearTimeout(timelineHideTimerRef.current);
      }
      timelineHideTimerRef.current = setTimeout(() => {
        setTimelineVisible(false);
        timelineHideTimerRef.current = null;
      }, 3000);
    }
  }, []);

  // ---- Virtuoso 回调：可见范围变化 (替代 IntersectionObserver scrollspy) ----
  const handleRangeChanged = useCallback(
    (range: { startIndex: number; endIndex: number }) => {
      if (timelineAnchors.length === 0) {
        setActiveTimelineIndex(-1);
        return;
      }
      // 找到第一个在可见范围内的用户消息锚点
      const firstVisible = timelineAnchors.find(
        (a) => a.messageIndex >= range.startIndex && a.messageIndex <= range.endIndex,
      );
      if (firstVisible) {
        const anchorIdx = timelineAnchors.indexOf(firstVisible);
        setActiveTimelineIndex(anchorIdx);
      }
    },
    [timelineAnchors],
  );

  const handleScrollToBottom = useCallback(() => {
    virtuosoRef.current?.scrollToIndex({ index: 'LAST', behavior: 'smooth' });
  }, []);

  // 跳转到指定锚点 (timeline 点击) — 使用 Virtuoso scrollToIndex
  const jumpToAnchor = useCallback((groupId: string) => {
    const anchor = timelineAnchors.find((a) => a.id === groupId);
    if (!anchor || !virtuosoRef.current) return;
    virtuosoRef.current.scrollToIndex({ index: anchor.messageIndex, align: 'start', behavior: 'smooth' });

    // 滚动到位后高亮动画
    setTimeout(() => {
      const el = document.getElementById(`turn-${groupId}`);
      if (!el) return;
      document.querySelectorAll('.chat-turn').forEach((turn) => turn.classList.remove('target-highlight'));
      void (el as HTMLElement).offsetWidth;
      el.classList.add('target-highlight');
    }, 600);
  }, [timelineAnchors]);

  // 切换到 chat view 时滚动到底部
  useEffect(() => {
    if (view === 'chat') {
      virtuosoRef.current?.scrollToIndex({ index: 'LAST' });
    }
  }, [view]);

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
      rememberStreamItemOverlay(sessionId, cached);
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
      const mergedSession = session
        ? mergeStreamItemsIntoStoredSession(
            session,
            sessionCacheRef.current.get(sessionId),
            undefined,
            undefined,
            streamItemOverlayRef.current.get(sessionId),
          )
        : null;
      if (mergedSession) {
        sessionCacheRef.current.set(sessionId, mergedSession);
        rememberStreamItemOverlay(sessionId, mergedSession);
      }
      setActiveSession(mergedSession);
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

  const chooseStoragePath = async () => {
    try {
      const nextPath = await selectDirectoryFromSystem();
      if (!nextPath) {
        setStorageRootStatus('dataRoot', '已取消选择目录。');
        return;
      }

      setStorageDraft(nextPath);
      setStorageRootStatus('dataRoot', '已选择新的数据目录，保存后会复制 SQLite 与插件数据。');
    } catch (error) {
      setStorageRootStatus('dataRoot', error instanceof Error ? error.message : String(error));
    }
  };

  const saveStoragePath = () => {
    requestStorageMigration('dataRoot');
  };

  const chooseArtifactPath = async () => {
    try {
      const nextPath = await selectDirectoryFromSystem();
      if (!nextPath) {
        setStorageRootStatus('artifactRoot', '已取消选择目录。');
        return;
      }

      setArtifactDraft(nextPath);
      setStorageRootStatus('artifactRoot', '已选择新的产物目录，保存后会复制现有产物目录。');
    } catch (error) {
      setStorageRootStatus('artifactRoot', error instanceof Error ? error.message : String(error));
    }
  };

  const saveArtifactPath = () => {
    requestStorageMigration('artifactRoot');
  };

  const chooseDialogueDataPath = async () => {
    try {
      const nextPath = await selectDirectoryFromSystem();
      if (!nextPath) {
        setStorageRootStatus('dialogueRoot', '已取消选择目录。');
        return;
      }

      setDialogueDataDraft(nextPath);
      setStorageRootStatus('dialogueRoot', '已选择新的对话目录，保存后会复制 otherone localfile 数据。');
    } catch (error) {
      setStorageRootStatus('dialogueRoot', error instanceof Error ? error.message : String(error));
    }
  };

  const saveDialogueDataPath = () => {
    requestStorageMigration('dialogueRoot');
  };

  const openStorageDirectory = async (key: StorageRootKey, path: string) => {
    try {
      const opened = await openDirectoryInSystem(path);
      setStorageRootStatus(key, opened ? '已打开目录。' : '当前环境无法打开系统目录。');
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setStorageRootStatus(key, `打开目录失败：${message}`);
    }
  };

  const requestStorageMigration = (targetKey: StorageRootKey) => {
    const nextStorage = {
      dataRoot: storageDraft,
      artifactRoot: artifactDraft,
      dialogueRoot: dialogueDataDraft,
    };
    const changedKeys: StorageRootKey[] = [];

    if (nextStorage.dataRoot !== storagePath) {
      changedKeys.push('dataRoot');
    }

    if (nextStorage.artifactRoot !== artifactPath) {
      changedKeys.push('artifactRoot');
    }

    if (nextStorage.dialogueRoot !== dialogueDataPath) {
      changedKeys.push('dialogueRoot');
    }

    if (changedKeys.length === 0) {
      setStorageRootStatus(targetKey, '路径未变化，无需迁移。');
      return;
    }

    setPendingStorageMigration({ changedKeys, storage: nextStorage, targetKey });
  };

  const cancelStorageMigration = () => {
    const targetKey = pendingStorageMigration?.targetKey;
    setPendingStorageMigration(null);

    if (targetKey) {
      setStorageRootStatus(targetKey, '已取消迁移。');
    }
  };

  const confirmStorageMigration = () => {
    const pendingMigration = pendingStorageMigration;
    setPendingStorageMigration(null);

    if (!pendingMigration) {
      return;
    }

    void migrateStorageSettings(pendingMigration.storage, pendingMigration.changedKeys);
  };

  const migrateStorageSettings = async (nextStorage: StorageSettings, changedKeys: StorageRootKey[]) => {
    setIsMigratingStorage(true);
    setChangedStorageRootStatuses(changedKeys, '正在复制并校验存储数据，请不要关闭应用。');

    try {
      const settings = await migrateStorageSettingsToStorage(nextStorage);

      applyStorageSettings(settings?.storage ?? nextStorage);
      setChangedStorageRootStatuses(changedKeys, '存储迁移完成，已切换到新目录。旧目录已保留，可确认无误后手动清理。');
      setSessions(await loadSessionsFromStorage());
    } catch (error) {
      setChangedStorageRootStatuses(changedKeys, error instanceof Error ? error.message : String(error));
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

  const updateWorkflowModelId = useCallback(async (workflowModelId: string) => {
    const nextEngine = { ...engineSettings, workflowModelId };
    setEngineSettings(nextEngine);

    try {
      const settings = await saveEngineSettingsToStorage(nextEngine);
      if (settings) {
        setEngineSettings(settings.engine);
      }
      pushToast('success', 'Todo AI 模型已保存');
    } catch (error) {
      pushToast('fail', '保存 Todo AI 模型失败', error instanceof Error ? error.message : String(error));
    }
  }, [engineSettings, pushToast]);

  const dismissToast = useCallback((id: string) => {
    setToasts((current) => current.filter((toast) => toast.id !== id));
  }, []);

  const handleOpenArtifactLocation = useCallback(
    async (item: FileArtifact) => {
      if (!item.path.trim()) {
        pushToast('warn', '无法定位文件', '当前文件路径为空。');
        return;
      }

      try {
        const opened = await revealFileInSystem(item.path);
        if (!opened) {
          pushToast('warn', '无法定位文件', '当前环境无法打开系统文件管理器。');
        }
      } catch (error) {
        pushToast('error', '定位文件失败', error instanceof Error ? error.message : String(error));
      }
    },
    [pushToast],
  );

  const applyFileArtifact = useCallback((record: FileArtifactRecord) => {
    const artifact = artifactRecordToFileArtifact(record);
    const upsert = (current: FileArtifact[]) => [artifact, ...current.filter((item) => item.id !== artifact.id)];

    if (record.action === 'edited') {
      setEditedFiles(upsert);
      return;
    }

    if (record.action === 'deleted') {
      setDeletedFiles(upsert);
      return;
    }

    setAddedFiles(upsert);
  }, []);

  useEffect(() => {
    activeSessionIdRef.current = activeSession?.id ?? null;
  }, [activeSession?.id]);

  useEffect(() => {
    let cancelled = false;
    const sessionId = activeSession?.id;

    if (!sessionId) {
      setEditedFiles([]);
      setDeletedFiles([]);
      setAddedFiles([]);
      return;
    }
    const currentSessionId = sessionId;

    async function loadArtifacts() {
      try {
        const artifacts = await listFileArtifacts(currentSessionId);

        if (cancelled) {
          return;
        }

        setEditedFiles(artifacts.filter((item) => item.action === 'edited').map(artifactRecordToFileArtifact));
        setDeletedFiles(artifacts.filter((item) => item.action === 'deleted').map(artifactRecordToFileArtifact));
        setAddedFiles(artifacts.filter((item) => item.action === 'added').map(artifactRecordToFileArtifact));
      } catch (error) {
        if (!cancelled) {
          pushToast('error', '读取产出面板失败', error instanceof Error ? error.message : String(error));
        }
      }
    }

    void loadArtifacts();

    return () => {
      cancelled = true;
    };
  }, [activeSession?.id, pushToast]);

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    let cancelled = false;

    void listenToFileArtifacts((artifact) => {
      if (artifact.sessionId !== activeSessionIdRef.current) {
        return;
      }

      applyFileArtifact(artifact);
    }).then((unlisten) => {
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
  }, [applyFileArtifact]);

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

  // 从 current streaming state 中收集完整 item 顺序，
  // 然后按这个顺序把 thinking 合并回 reloaded session。
  // 因为 otherone-agent 框架不单独持久化 thinking delta，只存最终 AI 文本 entry。
  const refreshSessionFromStorage = useCallback(
    async (sessionId: string, pendingStreamItems?: MessageItem[]) => {
      try {
        const [session, storedSessions] = await Promise.all([
          readSessionFromStorage(sessionId),
          loadSessionsFromStorage(),
        ]);

        if (!session) {
          setSessionError('会话数据读取失败。');
          return;
        }

        const streamGroupId = activeStreamGroupIdRef.current.get(sessionId);
        const cachedSession = sessionCacheRef.current.get(sessionId);
        const streamItems = pendingStreamItems ?? completeStreamItemsFromGroup(cachedSession, streamGroupId);
        const streamItemOverlay = mergeStreamItemOverlays(
          streamItemOverlayRef.current.get(sessionId),
          collectStreamItemOverlay(cachedSession),
        );
        const mergedSession = mergeStreamItemsIntoStoredSession(
          session,
          cachedSession,
          streamGroupId,
          streamItems,
          streamItemOverlay,
        );

        sessionCacheRef.current.set(sessionId, mergedSession);
        rememberStreamItemOverlay(sessionId, mergedSession);
        setActiveSession((current) => {
          if (!current || current.id !== sessionId) return current;
          return mergedSession;
        });

        setSessions(storedSessions);
      } catch (error) {
        setSessionError(error instanceof Error ? error.message : String(error));
      }
    },
    [rememberStreamItemOverlay],
  );

  // ---- 流式事件缓冲区 ----
  // 将高频 delta 事件缓冲到 rAF，一次性批量 apply，
  // 将 state 更新从 30-50 次/秒降至 ~10 次/秒。
  const streamBufferScheduledRef = useRef(false);
  const streamBufferSessionRef = useRef<string | null>(null);

  const applyStreamEventsToSession = useCallback((sessionId: string, events: ChatStreamEvent[]) => {
    if (events.length === 0) return null;

    const cached = sessionCacheRef.current.get(sessionId);
    let updatedSession: SessionDetail | null = null;

    if (cached) {
      updatedSession = cached;
      for (const evt of events) {
        updatedSession = applyStreamEventToSession(
          updatedSession,
          evt,
          streamIterRef.current,
          activeStreamGroupIdRef.current,
        );
      }
      sessionCacheRef.current.set(sessionId, updatedSession);
      rememberStreamItemOverlay(sessionId, updatedSession);
    }

    setActiveSession((current) => {
      if (!current || current.id !== sessionId) return current;
      if (updatedSession) return updatedSession;

      let updatedCurrent = current;
      for (const evt of events) {
        updatedCurrent = applyStreamEventToSession(
          updatedCurrent,
          evt,
          streamIterRef.current,
          activeStreamGroupIdRef.current,
        );
      }
      sessionCacheRef.current.set(sessionId, updatedCurrent);
      rememberStreamItemOverlay(sessionId, updatedCurrent);
      return updatedCurrent;
    });

    return updatedSession;
  }, [rememberStreamItemOverlay]);

  const flushStreamBuffer = useCallback(() => {
    streamBufferScheduledRef.current = false;
    const sid = streamBufferSessionRef.current;
    if (!sid) return;

    const events = pendingStreamEventsRef.current.get(sid);
    if (!events || events.length === 0) return;
    pendingStreamEventsRef.current.set(sid, []);

    // 批量 apply 所有缓冲事件 → 仅触发一次 state 更新
    applyStreamEventsToSession(sid, events);
  }, [applyStreamEventsToSession]);

  // 稳定引用：rAF 回调中用 ref 获取最新 flush 函数
  const flushStreamBufferRef = useRef(flushStreamBuffer);
  flushStreamBufferRef.current = flushStreamBuffer;

  const scheduleStreamFlush = useCallback((sessionId: string) => {
    streamBufferSessionRef.current = sessionId;
    if (streamBufferScheduledRef.current) return; // 已经排期了
    streamBufferScheduledRef.current = true;
    requestAnimationFrame(() => {
      flushStreamBufferRef.current();
    });
  }, []);

  const applyQueuedUserPromptsBoundary = useCallback((sessionId: string) => {
    const bufferedEvents = pendingStreamEventsRef.current.get(sessionId);
    if (bufferedEvents?.length) {
      pendingStreamEventsRef.current.set(sessionId, []);
      applyStreamEventsToSession(sessionId, bufferedEvents);
    }

    const queuedAiGroupId = queuedStreamAiGroupIdRef.current.get(sessionId);
    if (!queuedAiGroupId) return;

    const updatedAt = new Date().toISOString();
    const currentAiGroupId = activeStreamGroupIdRef.current.get(sessionId);

    if (currentAiGroupId && currentAiGroupId !== queuedAiGroupId) {
      const cachedSession = sessionCacheRef.current.get(sessionId);
      if (cachedSession) {
        const completedSession = completeAiGroupInSession(cachedSession, currentAiGroupId, updatedAt);
        sessionCacheRef.current.set(sessionId, completedSession);
        rememberStreamItemOverlay(sessionId, completedSession);
        setActiveSession((current) => (current?.id === sessionId ? completedSession : current));
      } else {
        setActiveSession((current) => {
          if (!current || current.id !== sessionId) return current;
          const completedSession = completeAiGroupInSession(current, currentAiGroupId, updatedAt);
          sessionCacheRef.current.set(sessionId, completedSession);
          rememberStreamItemOverlay(sessionId, completedSession);
          return completedSession;
        });
      }
    }

    activeStreamGroupIdRef.current.set(sessionId, queuedAiGroupId);
    queuedStreamAiGroupIdRef.current.delete(sessionId);
    streamIterRef.current.set(sessionId, 0);
  }, [applyStreamEventsToSession, rememberStreamItemOverlay]);

  /**
   * 核心：流式事件处理器。
   * - delta 类事件 → 入缓冲，rAF 批量 flush
   * - terminal 事件 (complete/error/cancelled) → 立即 flush + 处理
   */
  const handleChatStreamEvent = useCallback(
    (event: ChatStreamEvent) => {
      let completedStreamItems: MessageItem[] | undefined;
      if (event.eventType === 'queued_user_prompts') {
        applyQueuedUserPromptsBoundary(event.sessionId);
        return;
      }

      const isDelta =
        event.eventType === 'assistant_delta' ||
        event.eventType === 'assistant_thinking_delta' ||
        event.eventType === 'thinking' ||
        event.eventType === 'tool_call';

      const isTerminal =
        event.eventType === 'complete' ||
        event.eventType === 'error' ||
        event.eventType === 'cancelled';

      if (isDelta) {
        // 入缓冲
        const sid = event.sessionId;
        const buf = pendingStreamEventsRef.current.get(sid) ?? [];
        buf.push(event);
        pendingStreamEventsRef.current.set(sid, buf);
        scheduleStreamFlush(sid);
        return;
      }

      if (isTerminal) {
        // terminal 事件先 flush 缓冲区，再 apply 自身
        const sid = event.sessionId;
        const activeStreamGroupId = activeStreamGroupIdRef.current.get(sid);
        const buf = pendingStreamEventsRef.current.get(sid);
        const events = [...(buf ?? []), event];
        pendingStreamEventsRef.current.set(sid, []);
        const updatedSession = applyStreamEventsToSession(sid, events);
        completedStreamItems = completeStreamItemsFromGroup(updatedSession, activeStreamGroupId);
        streamBufferScheduledRef.current = false;
      }

      // 终端后处理
      if (event.eventType === 'complete') {
        streamingSessionIdsRef.current.delete(event.sessionId);
        queuedStreamAiGroupIdRef.current.delete(event.sessionId);
        pendingStreamEventsRef.current.delete(event.sessionId);
        const completedStreamGroupId = activeStreamGroupIdRef.current.get(event.sessionId);
        void refreshSessionFromStorage(event.sessionId, completedStreamItems).finally(() => {
          if (activeStreamGroupIdRef.current.get(event.sessionId) === completedStreamGroupId) {
            activeStreamGroupIdRef.current.delete(event.sessionId);
          }
        });
      }

      if (event.eventType === 'error') {
        streamingSessionIdsRef.current.delete(event.sessionId);
        activeStreamGroupIdRef.current.delete(event.sessionId);
        queuedStreamAiGroupIdRef.current.delete(event.sessionId);
        pendingStreamEventsRef.current.delete(event.sessionId);
        pushToast('error', '对话执行失败', event.error ?? event.content);
      }

      if (event.eventType === 'cancelled') {
        streamingSessionIdsRef.current.delete(event.sessionId);
        activeStreamGroupIdRef.current.delete(event.sessionId);
        queuedStreamAiGroupIdRef.current.delete(event.sessionId);
        pendingStreamEventsRef.current.delete(event.sessionId);
        pushToast('info', '对话已被终止');
      }
    },
    [applyQueuedUserPromptsBoundary, applyStreamEventsToSession, pushToast, refreshSessionFromStorage, scheduleStreamFlush],
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

  useEffect(() => {
    return () => {
      pendingChatSendsRef.current.forEach((batch) => {
        if (batch.timer) {
          clearTimeout(batch.timer);
        }
      });
      pendingChatSendsRef.current.clear();
    };
  }, []);

  const restorePendingChatSend = (batch: PendingChatSend) => {
    const { sessionId } = batch;
    const lastPrompt = batch.prompts[batch.prompts.length - 1] ?? '';

    streamingSessionIdsRef.current.delete(sessionId);
    activeStreamGroupIdRef.current.delete(sessionId);
    queuedStreamAiGroupIdRef.current.delete(sessionId);
    streamIterRef.current.delete(sessionId);
    pendingStreamEventsRef.current.delete(sessionId);

    if (batch.previousSession) {
      sessionCacheRef.current.set(sessionId, batch.previousSession);
    } else {
      sessionCacheRef.current.delete(sessionId);
      streamItemOverlayRef.current.delete(sessionId);
    }

    setActiveSession((current) => {
      if (current?.id !== sessionId) return current;
      return batch.previousSession;
    });

    setSessions((current) => {
      const withoutPending = current.filter((session) => session.id !== sessionId);
      if (!batch.previousSummary) return withoutPending;

      const insertAt = Math.min(Math.max(batch.previousSummaryIndex, 0), withoutPending.length);
      return [
        ...withoutPending.slice(0, insertAt),
        batch.previousSummary,
        ...withoutPending.slice(insertAt),
      ];
    });

    setMessage(lastPrompt);
    setPromptPanelOpen(false);
    requestAnimationFrame(resizePrompt);

    if (!batch.previousSession) {
      setView('new');
      setActiveItem('');
    }
  };

  const startPendingChatSend = async (batch: PendingChatSend) => {
    const prompt = batch.prompts.join('\n\n');

    try {
      await sendChatMessageToStorage({
        sessionId: batch.sessionId,
        modelId: batch.modelId,
        prompt,
        prompts: batch.prompts,
        reasoningEffort: batch.reasoningEffort,
        contextCompressionEnabled: batch.contextCompressionEnabled,
        branchModeEnabled: batch.branchModeEnabled,
        targetModeEnabled: batch.targetModeEnabled,
        memoryEnabled: batch.memoryEnabled,
      });
    } catch (error) {
      streamingSessionIdsRef.current.delete(batch.sessionId);
      handleChatStreamEvent({
        sessionId: batch.sessionId,
        eventType: 'error',
        content: '',
        error: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const schedulePendingChatSend = (batch: PendingChatSend) => {
    if (batch.timer) {
      clearTimeout(batch.timer);
    }

    batch.timer = setTimeout(() => {
      const current = pendingChatSendsRef.current.get(batch.sessionId);
      if (current !== batch) return;

      pendingChatSendsRef.current.delete(batch.sessionId);
      batch.timer = null;
      void startPendingChatSend(batch);
    }, SEND_REMORSE_DELAY_MS);
  };

  const cancelPendingChatSend = (sessionId: string) => {
    const batch = pendingChatSendsRef.current.get(sessionId);
    if (!batch) return false;

    if (batch.timer) {
      clearTimeout(batch.timer);
      batch.timer = null;
    }
    pendingChatSendsRef.current.delete(sessionId);
    restorePendingChatSend(batch);
    return true;
  };

  const appendPromptToPendingChatSend = (batch: PendingChatSend, prompt: string) => {
    const createdAt = new Date().toISOString();
    const { userGroupId, userGroup } = createOptimisticUserGroup(batch.sessionId, prompt, createdAt);

    batch.prompts.push(prompt);
    batch.userGroupIds.push(userGroupId);

    const cachedSession = sessionCacheRef.current.get(batch.sessionId);
    if (cachedSession) {
      const updatedSession = insertUserGroupBeforeAiGroup(cachedSession, userGroup, batch.aiGroupId, createdAt);
      sessionCacheRef.current.set(batch.sessionId, updatedSession);
      setActiveSession((current) => (current?.id === batch.sessionId ? updatedSession : current));
    }

    setSessions((current) => {
      const existing = current.find((session) => session.id === batch.sessionId);
      if (!existing) {
        return [
          {
            id: batch.sessionId,
            title: prompt.slice(0, 24) || 'New chat',
            createdAt,
            updatedAt: createdAt,
            lastMessage: prompt,
            messageCount: 1,
            pinned: false,
            archived: false,
          },
          ...current,
        ];
      }

      return [
        { ...existing, updatedAt: createdAt, lastMessage: prompt, messageCount: existing.messageCount + 1 },
        ...current.filter((session) => session.id !== batch.sessionId),
      ];
    });

    setMessage('');
    setPromptPanelOpen(false);
    requestAnimationFrame(resizePrompt);
    schedulePendingChatSend(batch);
  };

  const appendPromptToRunningChat = async (sessionId: string, prompt: string) => {
    const createdAt = new Date().toISOString();
    const { turnId, userGroup } = createOptimisticUserGroup(sessionId, prompt, createdAt);
    const cachedSessionBeforeSend = sessionCacheRef.current.get(sessionId);
    const previousSession = cachedSessionBeforeSend ?? activeSession;
    const previousSummaryIndex = sessions.findIndex((session) => session.id === sessionId);
    const previousSummary = previousSummaryIndex >= 0 ? sessions[previousSummaryIndex] : null;

    const currentAiGroupId = activeStreamGroupIdRef.current.get(sessionId);
    const canReuseCurrentAiGroup =
      !queuedStreamAiGroupIdRef.current.has(sessionId) &&
      isEmptyRunningAiGroup(previousSession, currentAiGroupId);
    let queuedAiGroupId = canReuseCurrentAiGroup
      ? currentAiGroupId
      : queuedStreamAiGroupIdRef.current.get(sessionId);
    let createdQueuedAiGroupId = '';
    let queuedAiGroup: MessageGroup | null = null;

    if (!queuedAiGroupId) {
      queuedAiGroupId = streamAiGroupId(sessionId, turnId);
      createdQueuedAiGroupId = queuedAiGroupId;
      queuedAiGroup = createOptimisticAiGroup(queuedAiGroupId, createdAt);
      queuedStreamAiGroupIdRef.current.set(sessionId, queuedAiGroupId);
    }

    const applyOptimisticQueuedGroups = (session: SessionDetail): SessionDetail => {
      if (queuedAiGroup) {
        return {
          ...session,
          updatedAt: createdAt,
          messages: [...session.messages, userGroup, queuedAiGroup],
        };
      }

      return insertUserGroupBeforeAiGroup(session, userGroup, queuedAiGroupId, createdAt);
    };

    if (previousSession) {
      const updatedSession = applyOptimisticQueuedGroups(previousSession);
      sessionCacheRef.current.set(sessionId, updatedSession);
      setActiveSession((current) => (current?.id === sessionId ? applyOptimisticQueuedGroups(current) : current));
    }

    setSessions((current) => {
      const existing = current.find((session) => session.id === sessionId);
      if (!existing) return current;

      return [
        { ...existing, updatedAt: createdAt, lastMessage: prompt, messageCount: existing.messageCount + 1 },
        ...current.filter((session) => session.id !== sessionId),
      ];
    });

    setMessage('');
    setPromptPanelOpen(false);
    requestAnimationFrame(resizePrompt);

    try {
      await enqueueChatMessageToStorage({
        sessionId,
        prompt,
        prompts: [prompt],
      });
    } catch (error) {
      if (createdQueuedAiGroupId && queuedStreamAiGroupIdRef.current.get(sessionId) === createdQueuedAiGroupId) {
        queuedStreamAiGroupIdRef.current.delete(sessionId);
      }

      if (previousSession) {
        sessionCacheRef.current.set(sessionId, previousSession);
      }

      setActiveSession((current) => (current?.id === sessionId ? previousSession : current));
      setSessions((current) => {
        const withoutOptimistic = current.filter((session) => session.id !== sessionId);
        if (!previousSummary) return withoutOptimistic;

        const insertAt = Math.min(Math.max(previousSummaryIndex, 0), withoutOptimistic.length);
        return [
          ...withoutOptimistic.slice(0, insertAt),
          previousSummary,
          ...withoutOptimistic.slice(insertAt),
        ];
      });
      setMessage(prompt);
      pushToast('error', '发送插入消息失败', error instanceof Error ? error.message : String(error));
      requestAnimationFrame(resizePrompt);
    }
  };

  const handleCancelMessage = async () => {
    if (!activeSession) return;

    const sessionId = activeSession.id;
    if (cancelPendingChatSend(sessionId)) return;
    streamingSessionIdsRef.current.delete(sessionId);
    queuedStreamAiGroupIdRef.current.delete(sessionId);
    // 立即在前端标记流式结束

    try {
      await cancelChatMessage(sessionId);
    } catch (error) {
      pushToast('error', '终止对话失败', error instanceof Error ? error.message : String(error));
    }
  };

  const clearFrontendRuntimeData = () => {
    pendingChatSendsRef.current.forEach((batch) => {
      if (batch.timer) {
        clearTimeout(batch.timer);
      }
    });
    pendingChatSendsRef.current.clear();
    streamingSessionIdsRef.current.clear();
    sessionCacheRef.current.clear();
    streamItemOverlayRef.current.clear();
    pendingStreamEventsRef.current.clear();
    streamIterRef.current.clear();
    activeStreamGroupIdRef.current.clear();
    queuedStreamAiGroupIdRef.current.clear();
    activeSessionIdRef.current = null;

    setSessions([]);
    setActiveSession(null);
    setActiveItem('');
    setSessionError('');
    setEditingSessionId('');
    setEditingSessionTitle('');
    setEditedFiles([]);
    setDeletedFiles([]);
    setAddedFiles([]);
    setArtifactsPanelOpen(false);
    setMessage('');
    setPromptPanelOpen(false);
  };

  const requestClearAllData = () => {
    setPendingClearAllData(true);
  };

  const cancelClearAllData = () => {
    setPendingClearAllData(false);
  };

  const confirmClearAllData = async () => {
    if (isClearingAllData) return;

    setPendingClearAllData(false);
    setIsClearingAllData(true);
    setStorageMigrationStatus('正在清空本地 SQLite、插件和缓存数据，请不要关闭应用。');
    setArtifactMigrationStatus('正在清空产物目录。');
    setDialogueMigrationStatus('正在清空对话、记忆和 otherone localfile 数据。');

    try {
      const settings = await clearAllOtheroneDataFromStorage();
      if (settings) {
        applyStorageSettings(settings.storage);
        setEngineSettings(settings.engine);
        setReasoningEffort(settings.engine.defaultReasoningEffort);
      }

      clearFrontendRuntimeData();
      setProviders(defaultApiConfigs);
      setStorageStatus('本地 SQLite 配置已清空，当前显示默认配置模板。');
      setStorageMigrationStatus('本地 SQLite、插件和缓存数据已清空。');
      setArtifactMigrationStatus('产物目录已清空。');
      setDialogueMigrationStatus('对话、记忆和 otherone localfile 数据已清空。');
      pushToast('success', '所有 otherone 本地数据已清空');
      requestAnimationFrame(resizePrompt);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setStorageMigrationStatus(message);
      setArtifactMigrationStatus(message);
      setDialogueMigrationStatus(message);
      pushToast('error', '清空数据失败', message);
    } finally {
      setIsClearingAllData(false);
    }
  };

  const handleSendMessage = async () => {
    const prompt = message.trim();
    const modelId = selectedModelId || selectorOptions[0]?.value;

    if (!prompt || !modelId || modelId === 'none') {
      return;
    }

    const sessionId = activeSession?.id ?? createClientSessionId();
    const pendingBatch = pendingChatSendsRef.current.get(sessionId);
    if (pendingBatch) {
      appendPromptToPendingChatSend(pendingBatch, prompt);
      return;
    }

    // 检查目标 session 是否已在流式传输中
    if (streamingSessionIdsRef.current.has(sessionId)) {
      await appendPromptToRunningChat(sessionId, prompt);
      return;
    }

    const createdAt = new Date().toISOString();
    const { turnId, userGroupId, userGroup } = createOptimisticUserGroup(sessionId, prompt, createdAt);
    const aiGroupId = streamAiGroupId(sessionId, turnId);
    const aiGroup = createOptimisticAiGroup(aiGroupId, createdAt);

    const baseSession: SessionDetail = {
      id: sessionId,
      title: prompt.slice(0, 24) || '新对话',
      createdAt,
      updatedAt: createdAt,
      messages: [userGroup, aiGroup],
    };
    const cachedSessionBeforeSend = sessionCacheRef.current.get(sessionId);
    const previousSession = cachedSessionBeforeSend ?? activeSession;
    const previousSummaryIndex = sessions.findIndex((session) => session.id === sessionId);
    const previousSummary = previousSummaryIndex >= 0 ? sessions[previousSummaryIndex] : null;
    const cachedSession = cachedSessionBeforeSend
      ? {
          ...cachedSessionBeforeSend,
          updatedAt: createdAt,
          messages: [...cachedSessionBeforeSend.messages, userGroup, aiGroup],
        }
      : baseSession;

    sessionCacheRef.current.set(sessionId, cachedSession);
    setActiveSession((current) => {
      if (current?.id === sessionId) {
        const updatedSession = {
          ...current,
          updatedAt: createdAt,
          messages: [...current.messages, userGroup, aiGroup],
        };
        sessionCacheRef.current.set(sessionId, updatedSession);
        return updatedSession;
      }
      return cachedSession;
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
    activeStreamGroupIdRef.current.set(sessionId, aiGroupId);
    streamIterRef.current.set(sessionId, 0);

    const batch: PendingChatSend = {
      sessionId,
      aiGroupId,
      userGroupIds: [userGroupId],
      prompts: [prompt],
      timer: null,
      previousSession: previousSession ? { ...previousSession, messages: [...previousSession.messages] } : null,
      previousSummary,
      previousSummaryIndex,
      modelId,
      reasoningEffort,
      contextCompressionEnabled,
      branchModeEnabled,
      targetModeEnabled,
      memoryEnabled,
    };

    pendingChatSendsRef.current.set(sessionId, batch);
    schedulePendingChatSend(batch);
  };

  const showChatUi =
    view !== 'settings' &&
    view !== 'workflow' &&
    view !== 'plugins' &&
    view !== 'personalization' &&
    view !== 'weixinClawbot';
  const currentModelValue = selectedModelId || selectorOptions[0].value;
  const pinnedSessions = sessions.filter((session) => session.pinned);
  const regularSessions = sessions.filter((session) => !session.pinned);

  return (
    <div className={`app-shell ${sidebarCollapsed ? 'sidebar-collapsed' : ''}`} data-theme={theme}>
      <WindowTitleBar />
      <div className="app-body">
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
            <SidebarItem
              active={activeItem === 'personalization'}
              icon={<SlidersHorizontal style={iconSize} />}
              label="个性化"
              compact={sidebarCollapsed}
              onClick={() => switchView('personalization', 'personalization')}
            />
            <SidebarItem
              active={activeItem === 'weixinClawbot'}
              icon={<BotMessageSquare style={iconSize} />}
              label="微信 ClawBot"
              compact={sidebarCollapsed}
              onClick={() => switchView('weixinClawbot', 'weixinClawbot')}
            />
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
                className={`view-container chat-history-view ${view === 'chat' ? 'active' : ''}`}
              >
                {isLoadingSessionDetail ? (
                  <MessagePanel messages={[]} emptyText="正在读取会话消息..." />
                ) : (
                  <MessagePanel
                    ref={virtuosoRef}
                    messages={displayMessages}
                    emptyText={sessionError || (activeSession ? '这个会话还没有消息。' : '请选择一个本地会话。')}
                    isStreaming={isAiStreaming}
                    onBottomStateChange={handleBottomStateChange}
                    onRangeChanged={handleRangeChanged}
                  />
                )}
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
                <div className="setting-item">
                  <div className="setting-item-info">
                    <span>Todo 提醒时间</span>
                    <small>任务开始前提前 1-60 分钟发送桌面和微信 ClawBot 提醒。</small>
                  </div>
                  <div className="setting-inline-controls">
                    <input
                      className="model-input setting-number-input"
                      min={1}
                      max={60}
                      type="number"
                      value={engineSettings.todoReminderLeadMinutes}
                      onChange={(event) => {
                        const value = Number(event.target.value);
                        const nextValue = Number.isFinite(value) ? Math.max(1, Math.min(60, value)) : 3;
                        setEngineSettings((current) => ({
                          ...current,
                          todoReminderLeadMinutes: nextValue,
                        }));
                      }}
                    />
                    <span className="setting-unit-label">分钟</span>
                    <button className="setting-btn" type="button" disabled={isSavingEngine} onClick={() => void saveEngineSettings()}>
                      {isSavingEngine ? '保存中' : '保存'}
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
                  迁移会复制当前受管数据并在校验成功后切换到新目录。旧目录不会自动删除，请确认新目录数据可用后再手动清理旧数据。
                </div>

                <div className="setting-item setting-item-column">
                  <div className="setting-item-info">
                    <span>数据存储路径</span>
                    <small>localfile 会保存 Agent 上下文，SQLite 会保存配置、会话索引和应用元数据。</small>
                  </div>
                  <div className="storage-path-panel">
                    <input
                      className="storage-path-input"
                      readOnly
                      title={storageDraft}
                      value={storageDraft}
                    />
                    <div className="storage-path-actions">
                      <button className="setting-btn" type="button" disabled={isMigratingStorage} onClick={() => void chooseStoragePath()}>
                        选择目录
                      </button>
                      <button
                        className="setting-btn"
                        type="button"
                        disabled={isMigratingStorage || !storageDraft.trim()}
                        onClick={() => void openStorageDirectory('dataRoot', storageDraft)}
                      >
                        <FolderOpen style={iconSize} />
                        打开目录
                      </button>
                      <button className="setting-btn" type="button" disabled={isMigratingStorage} onClick={saveStoragePath}>
                        {isMigratingStorage ? '迁移中' : '保存并迁移'}
                      </button>
                    </div>
                    <small className="storage-path-status">
                      当前路径：{storagePath}
                      <br />
                      {storageMigrationStatus}
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
                      readOnly
                      title={artifactDraft}
                      value={artifactDraft}
                    />
                    <div className="storage-path-actions">
                      <button className="setting-btn" type="button" disabled={isMigratingStorage} onClick={() => void chooseArtifactPath()}>
                        选择目录
                      </button>
                      <button
                        className="setting-btn"
                        type="button"
                        disabled={isMigratingStorage || !artifactDraft.trim()}
                        onClick={() => void openStorageDirectory('artifactRoot', artifactDraft)}
                      >
                        <FolderOpen style={iconSize} />
                        打开目录
                      </button>
                      <button className="setting-btn" type="button" disabled={isMigratingStorage} onClick={saveArtifactPath}>
                        {isMigratingStorage ? '迁移中' : '保存并迁移'}
                      </button>
                    </div>
                    <small className="storage-path-status">
                      当前路径：{artifactPath}
                      <br />
                      {artifactMigrationStatus}
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
                      readOnly
                      title={dialogueDataDraft}
                      value={dialogueDataDraft}
                    />
                    <div className="storage-path-actions">
                      <button className="setting-btn" type="button" disabled={isMigratingStorage} onClick={() => void chooseDialogueDataPath()}>
                        选择目录
                      </button>
                      <button
                        className="setting-btn"
                        type="button"
                        disabled={isMigratingStorage || !dialogueDataDraft.trim()}
                        onClick={() => void openStorageDirectory('dialogueRoot', dialogueDataDraft)}
                      >
                        <FolderOpen style={iconSize} />
                        打开目录
                      </button>
                      <button className="setting-btn" type="button" disabled={isMigratingStorage} onClick={saveDialogueDataPath}>
                        {isMigratingStorage ? '迁移中' : '保存并迁移'}
                      </button>
                    </div>
                    <small className="storage-path-status">
                      当前路径：{dialogueDataPath}
                      <br />
                      {dialogueMigrationStatus}
                    </small>
                  </div>
                </div>

                <div className="setting-item">
                  <div className="setting-item-info">
                    <span>清空所有数据</span>
                    <small style={{ color: 'var(--danger-color)' }}>将删除所有工作流、对话历史及本地缓存</small>
                  </div>
                  <div>
                    <button
                      className="setting-btn setting-btn-danger"
                      type="button"
                      disabled={isMigratingStorage || isClearingAllData}
                      onClick={requestClearAllData}
                    >
                      {isClearingAllData ? '清除中' : '全部清除'}
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
        ) : view === 'personalization' ? (
          <PersonalizationPage
            memoryEnabled={memoryEnabled}
            memoryModelId={memoryModelId}
            modelOptions={modelOptions}
            onMemoryEnabledChange={setMemoryEnabled}
            onMemoryModelChange={setMemoryModelId}
          />
        ) : view === 'workflow' ? (
          <WorkflowPage
            modelOptions={modelOptions}
            onClose={() => switchView('new')}
            onNotice={pushToast}
            onWorkflowModelChange={(modelId) => void updateWorkflowModelId(modelId)}
            workflowModelId={engineSettings.workflowModelId}
          />
        ) : view === 'weixinClawbot' ? (
          <WeixinClawbotPage onClose={() => switchView('new')} onNotice={pushToast} />
        ) : (
          <PluginsPage onClose={() => switchView('new')} />
        )}
      </main>
      <ArtifactsPanel
        addedFiles={addedFiles}
        deletedFiles={deletedFiles}
        editedFiles={editedFiles}
        onOpenFileLocation={handleOpenArtifactLocation}
        open={artifactsPanelOpen}
      />
      </div>

      <ToastViewport messages={toasts} onDismiss={dismissToast} />
      <ConfirmDialog
        open={pendingStorageMigration !== null}
        title="确认迁移存储数据"
        description="迁移会把当前受管数据复制到新目录，校验成功后切换到新目录。旧目录不会自动删除，请确认新路径可用后再手动清理旧数据。建议先手动备份当前数据目录，再继续。"
        confirmLabel="保存并迁移"
        cancelLabel="取消"
        tone="warning"
        onCancel={cancelStorageMigration}
        onConfirm={confirmStorageMigration}
      />
      <ConfirmDialog
        open={pendingClearAllData}
        title="确认清空所有数据"
        description="这会删除本地 SQLite 配置、工作流、会话历史、记忆、插件安装文件和产物目录内容，操作不可撤销。"
        confirmLabel={isClearingAllData ? '清除中' : '全部清除'}
        cancelLabel="取消"
        tone="danger"
        onCancel={cancelClearAllData}
        onConfirm={() => void confirmClearAllData()}
      />
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
