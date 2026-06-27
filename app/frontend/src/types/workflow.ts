export type WorkflowTaskStatus = 'pending' | 'completed';

export type WorkflowTask = {
  id: string;
  seriesId?: string | null;
  prompt: string;
  title: string;
  content: string;
  scheduledAt?: string | null;
  startAt?: string | null;
  endAt?: string | null;
  occurrenceDate?: string | null;
  timeText?: string | null;
  repeatStartDate?: string | null;
  repeatEndDate?: string | null;
  metadataJson: string;
  status: WorkflowTaskStatus;
  createdAt: string;
  updatedAt: string;
};
