export type MemoryPointKind = 'headless' | 'root' | 'point';

export type MemoryPointStatus = 'active' | 'deactivated';

export type MemoryPoint = {
  pointId: string;
  parentId: string | null;
  kind: MemoryPointKind;
  storage: string | null;
  types: string | null;
  status: MemoryPointStatus;
  createdAt: string;
  updatedAt: string;
  attributes: Record<string, unknown>;
};

export type MemoryTreeResponse = {
  storagePath: string;
  points: MemoryPoint[];
};
