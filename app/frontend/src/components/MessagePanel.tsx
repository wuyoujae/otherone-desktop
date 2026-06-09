import {
  BrainCircuit,
  Check,
  ChevronDown,
  Copy,
  Edit2,
  FolderSearch,
  GitBranch,
} from 'lucide-react';
import { type ReactNode, useEffect, useMemo, useRef, useState } from 'react';
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

type MessagePanelProps = {
  messages: MessageGroup[];
  emptyText?: string;
};

export function MessagePanel({ messages, emptyText = '这个会话还没有消息。' }: MessagePanelProps) {
  if (messages.length === 0) {
    return <div className="message-panel-empty">{emptyText}</div>;
  }

  return <MessagePanelContent messages={messages} />;
}

function MessagePanelContent({ messages, nested = false }: { messages: MessageGroup[]; nested?: boolean }) {
  // 检测最后一个 AI 消息是否在流式传输中
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

function MessageGroupView({ message, nested }: { message: MessageGroup; nested: boolean }) {
  const isStreaming = message.role === 'ai' && message.items.some((item) => item.status === 'running');

  return (
    <article className={`message-group ${message.role === 'user' ? 'user-message' : 'ai-message'}`}>
      <div className="message-items-wrapper">
        {message.items.map((item) => (
          <MessageItemView key={item.id} item={item} />
        ))}
      </div>
      {!nested && !isStreaming && <MessageActions role={message.role} />}
    </article>
  );
}

function MessageItemView({ item }: { item: MessageItem }) {
  const isStreaming = item.type === 'text' && item.status === 'running';

  if (item.type === 'text') {
    return <div className="message-item-text">{renderTextContent(item.content, isStreaming)}</div>;
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
}

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

function ThinkingItemView({ item }: { item: ThinkingMessageItem }) {
  const isRunning = item.status === 'running';
  const [isOpen, setIsOpen] = useState(isRunning);
  const Icon = isRunning ? BrainCircuit : Check;

  // 当 status 从 running 变成 completed 时自动合上
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

function CollapsibleBody({ isOpen, children }: { isOpen: boolean; children: ReactNode }) {
  return (
    <div className="collapsible-body-wrapper" aria-hidden={!isOpen}>
      <div className="collapsible-body">{children}</div>
    </div>
  );
}

/** 流式入场动画组件 — 将文字按 token 拆分，每个词独立 blur-word 动画 */
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

function RenderedTextContent({ content, isStreaming }: { content: string; isStreaming: boolean }) {
  const streamingNodes = useStreamingMarkdown(content, isStreaming);

  if (!content) return <p></p>;

  return <>{streamingNodes}</>;
}

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

function renderTextContent(content: string, isStreaming = false) {
  return <RenderedTextContent content={content} isStreaming={isStreaming} />;
}
