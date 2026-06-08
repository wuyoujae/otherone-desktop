import {
  BrainCircuit,
  Check,
  ChevronDown,
  Copy,
  Edit2,
  FolderSearch,
  GitBranch,
  Loader2,
} from 'lucide-react';
import { type ReactNode, useState } from 'react';
import type {
  AgentMessageItem,
  MessageGroup,
  MessageItem,
  MessageRole,
  ThinkingMessageItem,
  ToolMessageItem,
} from '../types/session';

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
  return (
    <div className={`message-panel ${nested ? 'message-panel-nested' : ''}`}>
      {messages.map((message) => (
        <MessageGroupView key={message.id} message={message} nested={nested} />
      ))}
    </div>
  );
}

function MessageGroupView({ message, nested }: { message: MessageGroup; nested: boolean }) {
  return (
    <article className={`message-group ${message.role === 'user' ? 'user-message' : 'ai-message'}`}>
      <div className="message-items-wrapper">
        {message.items.map((item) => (
          <MessageItemView key={item.id} item={item} />
        ))}
      </div>
      {!nested && <MessageActions role={message.role} />}
    </article>
  );
}

function MessageItemView({ item }: { item: MessageItem }) {
  if (item.type === 'text') {
    return <div className="message-item-text">{renderTextContent(item.content)}</div>;
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
  const [isOpen, setIsOpen] = useState(false);

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
  const [isOpen, setIsOpen] = useState(true);
  const Icon = item.status === 'completed' ? Check : BrainCircuit;

  return (
    <div className={`message-item-collapsible message-item-thinking state-${item.status} ${isOpen ? 'is-open' : ''}`}>
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
        <div className="collapsible-content thinking-collapsible-content">
          <div className="thinking-log">{item.content || '正在整理思考过程...'}</div>
        </div>
      </CollapsibleBody>
    </div>
  );
}

function AgentItemView({ item }: { item: AgentMessageItem }) {
  const [isOpen, setIsOpen] = useState(Boolean(item.defaultOpen));
  const Icon = item.status === 'completed' ? Check : Loader2;

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
  const Icon = item.status === 'completed' ? Check : FolderSearch;

  return (
    <span className={`message-tool-row state-${item.status} ${embedded ? 'is-embedded' : ''}`}>
      <span className="tool-icon">
        <Icon style={iconSize} />
      </span>
      <span className="tool-text">{item.label}</span>
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

function renderTextContent(content: string) {
  const blocks = content.split(/\n{2,}/).map((block) => block.trim()).filter(Boolean);

  if (blocks.length === 0) {
    return <p></p>;
  }

  return blocks.map((block, index) => <p key={`${index}-${block.slice(0, 12)}`}>{block}</p>);
}
