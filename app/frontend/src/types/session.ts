export type SessionSummary = {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  lastMessage: string;
  messageCount: number;
  pinned: boolean;
  archived: boolean;
};

export type SessionDetail = {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  messages: MessageGroup[];
};

export type MessageRole = 'user' | 'ai';
export type MessageItemStatus = 'running' | 'completed';

export type TextMessageItem = {
  id: string;
  type: 'text';
  content: string;
  status: MessageItemStatus;
  entryId: string;
  sourceRole: string;
  createdAt: string;
  tools?: unknown;
  tokenConsumption?: number;
};

export type ToolMessageItem = {
  id: string;
  type: 'tool';
  label: string;
  status: MessageItemStatus;
  detail?: string;
  entryId: string;
  sourceRole: string;
  createdAt: string;
  tools?: unknown;
  tokenConsumption?: number;
};

export type ThinkingMessageItem = {
  id: string;
  type: 'thinking';
  label: string;
  content: string;
  status: MessageItemStatus;
  entryId: string;
  sourceRole: string;
  createdAt: string;
};

export type AgentMessageItem = {
  id: string;
  type: 'agent';
  label: string;
  status: MessageItemStatus;
  defaultOpen?: boolean;
  messages: MessageGroup[];
  entryId: string;
  sourceRole: string;
  createdAt: string;
};

export type MessageItem = TextMessageItem | ToolMessageItem | ThinkingMessageItem | AgentMessageItem;

export type MessageGroup = {
  id: string;
  role: MessageRole;
  items: MessageItem[];
};
