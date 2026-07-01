import type { FileArtifact } from '../components/ArtifactsPanel';
import { isDesktopRuntime } from './platform/runtime';
import { invokeDesktop, listenDesktop } from './platform/tauri';
import { canUseWebApi, listenWebApiEventStream, requestWebApi } from './platform/webApi';

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
  if (isDesktopRuntime()) {
    return invokeDesktop<FileArtifactRecord[]>('list_file_artifacts', { sessionId });
  }

  if (canUseWebApi()) {
    return requestWebApi<FileArtifactRecord[]>(`/api/sessions/${encodeURIComponent(sessionId)}/artifacts`);
  }

  return [];
}

export async function listenToFileArtifacts(onEvent: (event: FileArtifactRecord) => void) {
  if (isDesktopRuntime()) {
    return listenDesktop<FileArtifactRecord>('file_artifact_event', onEvent);
  }

  if (canUseWebApi()) {
    return listenWebApiEventStream<FileArtifactRecord>('/api/artifacts/stream', onEvent);
  }

  return () => undefined;
}
