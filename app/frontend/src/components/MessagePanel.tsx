import {
  BrainCircuit,
  Check,
  ChevronDown,
  Copy,
  Edit2,
  FolderSearch,
  GitBranch,
} from 'lucide-react';
import { type ReactNode, forwardRef, memo, useEffect, useMemo, useRef, useState } from 'react';
import { Virtuoso, type VirtuosoHandle } from 'react-virtuoso';
import type {
  AgentMessageItem,
  MessageGroup,
  MessageItem,
  MessageRole,
  ThinkingMessageItem,
  ToolMessageItem,
} from '../types/session';
import { parseMarkdown, MarkdownRenderer, useStreamingMarkdown } from '../utils/markdown';

const iconSize = { width: 14, height: 14 };

function getMessagePanelRowClass(message: MessageGroup, index: number, messages: MessageGroup[]) {
  const classes = ['message-panel-row'];
  const previous = messages[index - 1];

  if (message.role === 'user') {
    classes.push('chat-turn');
  }

  if (previous?.role === message.role) {
    classes.push('message-panel-row-compact');
  } else if (previous) {
    classes.push('message-panel-row-spaced');
  }

  return classes.join(' ');
}

// ============================================================
// MessagePanel — 使用 react-virtuoso 虚拟滚动，仅渲染可见区域的消息
// ============================================================

type MessagePanelProps = {
  messages: MessageGroup[];
  emptyText?: string;
  isStreaming?: boolean;
  onBottomStateChange?: (atBottom: boolean) => void;
  onRangeChanged?: (range: { startIndex: number; endIndex: number }) => void;
};

export const MessagePanel = memo(
  forwardRef<VirtuosoHandle, MessagePanelProps>(function MessagePanel(
    { messages, emptyText = '这个会话还没有消息。', isStreaming = false, onBottomStateChange, onRangeChanged },
    ref,
  ) {
    const lastAiGroupId = useMemo(
      () => [...messages].reverse().find((message) => message.role === 'ai')?.id,
      [messages],
    );

    if (messages.length === 0) {
      return <div className="message-panel-empty">{emptyText}</div>;
    }

    return (
      <Virtuoso
        ref={ref}
        className="message-panel-virtuoso"
        style={{ height: '100%' }}
        data={messages}
        followOutput={isStreaming ? 'smooth' : undefined}
        computeItemKey={(index, item) => item.id ?? index}
        itemContent={(index, message) => (
          <div
            id={message.role === 'user' ? `turn-${message.id}` : undefined}
            className={getMessagePanelRowClass(message, index, messages)}
          >
            <MessageGroupView
              message={message}
              nested={false}
              holdActions={isStreaming && message.role === 'ai' && message.id === lastAiGroupId}
            />
          </div>
        )}
        atBottomStateChange={onBottomStateChange}
        rangeChanged={onRangeChanged}
        increaseViewportBy={{ top: 400, bottom: 400 }}
        components={{
          Header: () => <div style={{ height: 24 }} />,
          Footer: () => (
            <>
              {isStreaming && <GeneratingIndicator />}
              <div style={{ height: 160 }} />
            </>
          ),
        }}
      />
    );
  }),
);

// ============================================================
// MessagePanelContent — 仅用于嵌套 Agent 子消息，不做虚拟滚动
// ============================================================

export function MessagePanelContent({ messages, nested = false }: { messages: MessageGroup[]; nested?: boolean }) {
  const lastAiGroup = [...messages].reverse().find((g) => g.role === 'ai');
  const isStreaming =
    !nested &&
    lastAiGroup != null &&
    lastAiGroup.items.some((item) => item.status === 'running');

  return (
    <div className={`message-panel ${nested ? 'message-panel-nested' : ''}`}>
      {messages.map((message) => {
        if (message.role === 'user') {
          return (
            <div key={message.id} id={`turn-${message.id}`} className="chat-turn">
              <MessageGroupView message={message} nested={nested} />
            </div>
          );
        }
        return <MessageGroupView key={message.id} message={message} nested={nested} />;
      })}
      {isStreaming && <GeneratingIndicator />}
    </div>
  );
}

// ============================================================
// MessageGroupView
// ============================================================

const MessageGroupView = memo(function MessageGroupView({
  message,
  nested,
  holdActions = false,
}: {
  message: MessageGroup;
  nested: boolean;
  holdActions?: boolean;
}) {
  const isStreaming = message.role === 'ai' && message.items.some((item) => item.status === 'running');

  return (
    <article className={`message-group ${message.role === 'user' ? 'user-message' : 'ai-message'}`}>
      <div className="message-items-wrapper">
        {message.items.map((item) => (
          <MessageItemView key={item.id} item={item} />
        ))}
      </div>
      {!nested && !isStreaming && !holdActions && <MessageActions role={message.role} />}
    </article>
  );
});

// ============================================================
// MessageItemView
// ============================================================

const MessageItemView = memo(function MessageItemView({ item }: { item: MessageItem }) {
  const isStreaming = item.type === 'text' && item.status === 'running';

  if (item.type === 'text') {
    return (
      <div className="message-item-text">
        <RenderedTextContent content={item.content} isStreaming={isStreaming} />
      </div>
    );
  }

  if (item.type === 'agent') {
    return <AgentItemView item={item} />;
  }

  if (item.type === 'thinking') {
    return <ThinkingItemView item={item} />;
  }

  if (item.detail) {
    return <CollapsibleToolItem item={item} />;
  }

  return <ToolRow item={item} />;
});

// ============================================================
// RenderedTextContent — 用 memo 避免已完成的文本消息重复解析 markdown
// ============================================================

const RenderedTextContent = memo(function RenderedTextContent({
  content,
  isStreaming,
}: {
  content: string;
  isStreaming: boolean;
}) {
  const streamingNodes = useStreamingMarkdown(content, isStreaming);

  if (!content) return <p></p>;

  return <>{streamingNodes}</>;
});

// ============================================================
// CollapsibleToolItem
// ============================================================

function CollapsibleToolItem({ item }: { item: ToolMessageItem }) {
  const isRunning = item.status === 'running';
  const [isOpen, setIsOpen] = useState(isRunning);
  const prevRunningRef = useRef(isRunning);
  useEffect(() => {
    if (prevRunningRef.current && !isRunning) {
      setIsOpen(false);
    }
    prevRunningRef.current = isRunning;
  }, [isRunning]);

  return (
    <div className={`message-item-collapsible state-${item.status} ${isOpen ? 'is-open' : ''}`}>
      <button className="collapsible-trigger" type="button" onClick={() => setIsOpen((current) => !current)}>
        <ToolRow item={item} embedded />
        <ChevronDown className="chevron-icon" style={iconSize} />
      </button>
      <CollapsibleBody isOpen={isOpen}>
        <div className="collapsible-content">
          <pre className="tool-log">{item.detail}</pre>
        </div>
      </CollapsibleBody>
    </div>
  );
}

// ============================================================
// ThinkingItemView
// ============================================================

function ThinkingItemView({ item }: { item: ThinkingMessageItem }) {
  const isRunning = item.status === 'running';
  const [isOpen, setIsOpen] = useState(isRunning);
  const Icon = isRunning ? BrainCircuit : Check;

  const prevRunningRef = useRef(isRunning);
  useEffect(() => {
    if (prevRunningRef.current && !isRunning) {
      setIsOpen(false);
    }
    prevRunningRef.current = isRunning;
  }, [isRunning]);

  const thinkingBlocks = useMemo(() => {
    if (item.status === 'completed' && item.content) {
      return parseMarkdown(item.content);
    }
    return null;
  }, [item.status, item.content]);

  return (
    <div className={`message-item-collapsible message-item-thinking state-${item.status} ${isOpen ? 'is-open' : ''}`}>
      <button className="collapsible-trigger" type="button" onClick={() => setIsOpen((current) => !current)}>
        <span className="message-tool-row">
          <span className="tool-icon">
            <Icon style={iconSize} />
          </span>
          <span className="tool-text">
            {isRunning ? <BlurWord text={item.label} /> : item.label}
          </span>
        </span>
        <ChevronDown className="chevron-icon" style={iconSize} />
      </button>
      <CollapsibleBody isOpen={isOpen}>
        <div className="collapsible-content thinking-collapsible-content">
          {isRunning ? (
            <div className="thinking-log"><BlurWord text={item.content || ''} /></div>
          ) : thinkingBlocks ? (
            <MarkdownRenderer blocks={thinkingBlocks} />
          ) : (
            <div className="thinking-log">{item.content || '正在整理思考过程...'}</div>
          )}
        </div>
      </CollapsibleBody>
    </div>
  );
}

// ============================================================
// AgentItemView
// ============================================================

function AgentItemView({ item }: { item: AgentMessageItem }) {
  const isRunning = item.status === 'running';
  const [isOpen, setIsOpen] = useState(isRunning || Boolean(item.defaultOpen));
  const Icon = isRunning ? FolderSearch : Check;
  const prevRunningRef = useRef(isRunning);
  useEffect(() => {
    if (prevRunningRef.current && !isRunning) {
      setIsOpen(false);
    }
    prevRunningRef.current = isRunning;
  }, [isRunning]);

  return (
    <div className={`message-item-collapsible message-item-agent state-${item.status} ${isOpen ? 'is-open' : ''}`}>
      <button className="collapsible-trigger" type="button" onClick={() => setIsOpen((current) => !current)}>
        <span className="message-tool-row">
          <span className="tool-icon">
            <Icon style={iconSize} />
          </span>
          <span className="tool-text">{item.label}</span>
        </span>
        <ChevronDown className="chevron-icon" style={iconSize} />
      </button>
      <CollapsibleBody isOpen={isOpen}>
        <div className="collapsible-content agent-collapsible-content">
          <MessagePanelContent messages={item.messages} nested />
        </div>
      </CollapsibleBody>
    </div>
  );
}

// ============================================================
// ToolRow
// ============================================================

function ToolRow({ item, embedded = false }: { item: ToolMessageItem; embedded?: boolean }) {
  const isRunning = item.status === 'running';
  const Icon = isRunning ? FolderSearch : Check;

  return (
    <span className={`message-tool-row state-${item.status} ${embedded ? 'is-embedded' : ''}`}>
      <span className="tool-icon">
        <Icon style={iconSize} />
      </span>
      <span className="tool-text">
        {isRunning ? <BlurWord text={item.label} /> : item.label}
      </span>
    </span>
  );
}

// ============================================================
// CollapsibleBody
// ============================================================

function CollapsibleBody({ isOpen, children }: { isOpen: boolean; children: ReactNode }) {
  return (
    <div className="collapsible-body-wrapper" aria-hidden={!isOpen}>
      <div className="collapsible-body">{children}</div>
    </div>
  );
}

// ============================================================
// BlurWord — 流式入场动画
// ============================================================

function BlurWord({ text }: { text: string }) {
  const tokens = text.split(/(\s+)/);
  return (
    <>
      {tokens.map((token, index) => (
        <span key={index} className="blur-word">
          {token}
        </span>
      ))}
    </>
  );
}

// ============================================================
// MessageActions
// ============================================================

function MessageActions({ role }: { role: MessageRole }) {
  return (
    <div className="message-actions">
      <button className="message-action-btn" type="button" title="编辑" aria-label="编辑消息">
        <Edit2 style={iconSize} />
      </button>
      <button className="message-action-btn" type="button" title="复制" aria-label="复制消息">
        <Copy style={iconSize} />
      </button>
      {role === 'ai' && (
        <button className="message-action-btn" type="button" title="创建分支" aria-label="创建分支">
          <GitBranch style={iconSize} />
        </button>
      )}
    </div>
  );
}

// ============================================================
// GeneratingIndicator
// ============================================================

function GeneratingIndicator() {
  return (
    <div className="generating-indicator">
      <span className="generating-text">Generating</span>
      <span className="generating-dots">
        <span className="generating-dot">.</span>
        <span className="generating-dot">.</span>
        <span className="generating-dot">.</span>
      </span>
    </div>
  );
}
