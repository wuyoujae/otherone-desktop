import type { WorkflowTask, WorkflowTaskStatus } from '../types/workflow';

const isTauriRuntime = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export async function loadWorkflowTasksFromStorage() {
  if (!isTauriRuntime()) {
    return [];
  }

  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<WorkflowTask[]>('list_workflow_tasks');
}

export async function loadWorkflowTasksForRangeFromStorage(startDate: string, endDate: string) {
  if (!isTauriRuntime()) {
    return [];
  }

  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<WorkflowTask[]>('list_workflow_tasks_for_range', { request: { startDate, endDate } });
}

export async function createWorkflowTaskInStorage(prompt: string) {
  const taskPrompt = prompt.trim();

  if (!taskPrompt || !isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<WorkflowTask>('create_workflow_task', { request: { prompt: taskPrompt } });
}

export async function updateWorkflowTaskInStorage(id: string, prompt: string) {
  const taskId = id.trim();
  const taskPrompt = prompt.trim();

  if (!taskId || !taskPrompt || !isTauriRuntime()) {
    return [];
  }

  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<WorkflowTask[]>('update_workflow_task', { request: { id: taskId, prompt: taskPrompt } });
}

export async function updateWorkflowTaskStatusInStorage(id: string, status: WorkflowTaskStatus) {
  const taskId = id.trim();

  if (!taskId || !isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<WorkflowTask>('update_workflow_task_status', { request: { id: taskId, status } });
}

export async function deleteWorkflowTaskInStorage(id: string) {
  const taskId = id.trim();

  if (!taskId || !isTauriRuntime()) {
    return false;
  }

  const { invoke } = await import('@tauri-apps/api/core');
  await invoke<void>('delete_workflow_task', { request: { id: taskId } });
  return true;
}
