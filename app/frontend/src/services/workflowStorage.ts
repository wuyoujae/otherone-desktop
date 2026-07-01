import type { WorkflowTask, WorkflowTaskStatus } from '../types/workflow';
import { isDesktopRuntime } from './platform/runtime';
import { invokeDesktop } from './platform/tauri';
import { canUseWebApi, requestWebApi } from './platform/webApi';

export async function loadWorkflowTasksFromStorage() {
  if (isDesktopRuntime()) {
    return invokeDesktop<WorkflowTask[]>('list_workflow_tasks');
  }

  if (canUseWebApi()) {
    return requestWebApi<WorkflowTask[]>('/api/workflow/tasks');
  }

  return [];
}

export async function loadWorkflowTasksForRangeFromStorage(startDate: string, endDate: string) {
  if (isDesktopRuntime()) {
    return invokeDesktop<WorkflowTask[]>('list_workflow_tasks_for_range', { request: { startDate, endDate } });
  }

  if (canUseWebApi()) {
    return requestWebApi<WorkflowTask[]>('/api/workflow/tasks', {
      query: { startDate, endDate },
    });
  }

  return [];
}

export async function createWorkflowTaskInStorage(prompt: string, modelId = '') {
  const taskPrompt = prompt.trim();
  const taskModelId = modelId.trim();

  if (!taskPrompt) {
    return null;
  }

  if (isDesktopRuntime()) {
    return invokeDesktop<WorkflowTask>('create_workflow_task', { request: { prompt: taskPrompt, modelId: taskModelId || null } });
  }

  if (canUseWebApi()) {
    return requestWebApi<WorkflowTask>('/api/workflow/tasks', {
      method: 'POST',
      body: { prompt: taskPrompt, modelId: taskModelId || null },
    });
  }

  return null;
}

export async function updateWorkflowTaskInStorage(id: string, prompt: string, modelId = '') {
  const taskId = id.trim();
  const taskPrompt = prompt.trim();
  const taskModelId = modelId.trim();

  if (!taskId || !taskPrompt) {
    return [];
  }

  if (isDesktopRuntime()) {
    return invokeDesktop<WorkflowTask[]>('update_workflow_task', {
      request: { id: taskId, prompt: taskPrompt, modelId: taskModelId || null },
    });
  }

  if (canUseWebApi()) {
    return requestWebApi<WorkflowTask[]>(`/api/workflow/tasks/${encodeURIComponent(taskId)}`, {
      method: 'PATCH',
      body: { prompt: taskPrompt, modelId: taskModelId || null },
    });
  }

  return [];
}

export async function updateWorkflowTaskStatusInStorage(id: string, status: WorkflowTaskStatus) {
  const taskId = id.trim();

  if (!taskId) {
    return null;
  }

  if (isDesktopRuntime()) {
    return invokeDesktop<WorkflowTask>('update_workflow_task_status', { request: { id: taskId, status } });
  }

  if (canUseWebApi()) {
    return requestWebApi<WorkflowTask>(`/api/workflow/tasks/${encodeURIComponent(taskId)}/status`, {
      method: 'PATCH',
      body: { status },
    });
  }

  return null;
}

export async function deleteWorkflowTaskInStorage(id: string) {
  const taskId = id.trim();

  if (!taskId) {
    return false;
  }

  if (isDesktopRuntime()) {
    await invokeDesktop<void>('delete_workflow_task', { request: { id: taskId } });
    return true;
  }

  if (canUseWebApi()) {
    await requestWebApi<void>(`/api/workflow/tasks/${encodeURIComponent(taskId)}`, {
      method: 'DELETE',
    });
    return true;
  }

  return false;
}
