import type { FileArtifact } from '../components/ArtifactsPanel';

const isTauriRuntime = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export type FileArtifactAction = 'edited' | 'added' | 'deleted';

export type FileArtifactRecord = FileArtifact & {
  sessionId: string;
  action: FileArtifactAction;
  toolName: string;
  filePath: string;
  patchJson: string;
  createdAt: string;
};

export async function listFileArtifacts(sessionId: string) {
  if (!isTauriRuntime()) {
    return [];
  }

  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<FileArtifactRecord[]>('list_file_artifacts', { sessionId });
}

export async function listenToFileArtifacts(onEvent: (event: FileArtifactRecord) => void) {
  if (!isTauriRuntime()) {
    return () => undefined;
  }

  const { listen } = await import('@tauri-apps/api/event');
  const unlisten = await listen<FileArtifactRecord>('file_artifact_event', (event) => {
    onEvent(event.payload);
  });
  return unlisten;
}
