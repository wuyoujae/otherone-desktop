import type { WorkflowTask } from '../types/workflow';

const WORKFLOW_TASK_TONE_COUNT = 8;

function taskColorSeed(task: WorkflowTask) {
  return task.id || task.seriesId || task.title || task.prompt || task.createdAt;
}

function hashString(value: string) {
  let hash = 0;

  for (let index = 0; index < value.length; index += 1) {
    hash = (hash * 31 + value.charCodeAt(index)) >>> 0;
  }

  return hash;
}

function baseToneIndex(task: WorkflowTask) {
  return hashString(taskColorSeed(task)) % WORKFLOW_TASK_TONE_COUNT;
}

export function resolveWorkflowTaskToneClasses(tasks: WorkflowTask[]) {
  let previousTone = -1;

  return tasks.map((task) => {
    let tone = baseToneIndex(task);

    if (tone === previousTone) {
      tone = (tone + 1) % WORKFLOW_TASK_TONE_COUNT;
    }

    previousTone = tone;
    return `tone-${tone + 1}`;
  });
}
