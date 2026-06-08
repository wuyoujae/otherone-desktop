import {
  ChevronDown,
  ChevronRight,
  File,
  FileArchive,
  FileCode,
  FileImage,
  FileJson,
  FileSpreadsheet,
  FileText,
  FileVideo,
  Loader2,
  MessageSquare,
  Search,
  X,
} from 'lucide-react';
import type { ReactNode } from 'react';
import { useEffect, useRef, useState } from 'react';
import type { FileArtifact } from './ArtifactsPanel';
import type { SessionSummary } from '../types/session';

const iconSize = { width: 16, height: 16 };
const largeIconSize = { width: 22, height: 22 };

type SearchOverlayProps = {
  allArtifacts: FileArtifact[];
  onClose: () => void;
  onOpenSession: (sessionId: string) => void;
  open: boolean;
  sessions: SessionSummary[];
};

type SearchResult = {
  id: string;
  title: string;
  subtitle: string;
  kind: 'session' | 'artifact';
  /** For session results, the session ID. For artifacts, the artifact ID. */
  targetId: string;
  extension?: string;
};

const EXTENSION_ICON_MAP: Record<string, ReactNode> = {
  pdf: <FileText style={iconSize} />,
  pptx: <FileSpreadsheet style={iconSize} />,
  ppt: <FileSpreadsheet style={iconSize} />,
  xlsx: <FileSpreadsheet style={iconSize} />,
  xls: <FileSpreadsheet style={iconSize} />,
  csv: <FileSpreadsheet style={iconSize} />,
  docx: <FileText style={iconSize} />,
  doc: <FileText style={iconSize} />,
  txt: <FileText style={iconSize} />,
  md: <FileText style={iconSize} />,
  png: <FileImage style={iconSize} />,
  jpg: <FileImage style={iconSize} />,
  jpeg: <FileImage style={iconSize} />,
  gif: <FileImage style={iconSize} />,
  svg: <FileImage style={iconSize} />,
  webp: <FileImage style={iconSize} />,
  js: <FileCode style={iconSize} />,
  ts: <FileCode style={iconSize} />,
  jsx: <FileCode style={iconSize} />,
  tsx: <FileCode style={iconSize} />,
  py: <FileCode style={iconSize} />,
  rs: <FileCode style={iconSize} />,
  go: <FileCode style={iconSize} />,
  java: <FileCode style={iconSize} />,
  c: <FileCode style={iconSize} />,
  cpp: <FileCode style={iconSize} />,
  html: <FileCode style={iconSize} />,
  css: <FileCode style={iconSize} />,
  json: <FileJson style={iconSize} />,
  yaml: <FileCode style={iconSize} />,
  yml: <FileCode style={iconSize} />,
  toml: <FileCode style={iconSize} />,
  xml: <FileCode style={iconSize} />,
  zip: <FileArchive style={iconSize} />,
  tar: <FileArchive style={iconSize} />,
  gz: <FileArchive style={iconSize} />,
  rar: <FileArchive style={iconSize} />,
  mp4: <FileVideo style={iconSize} />,
  mov: <FileVideo style={iconSize} />,
  avi: <FileVideo style={iconSize} />,
  webm: <FileVideo style={iconSize} />,
};

function getFileIcon(extension: string): ReactNode {
  return EXTENSION_ICON_MAP[extension.toLowerCase()] ?? <File style={iconSize} />;
}

function getFileColor(extension: string): string {
  const code = ['js', 'ts', 'jsx', 'tsx', 'py', 'rs', 'go', 'java', 'c', 'cpp', 'html', 'css', 'json', 'yaml', 'yml', 'toml', 'xml'];
  const doc = ['pdf', 'docx', 'doc', 'txt', 'md'];
  const sheet = ['xlsx', 'xls', 'csv'];
  const image = ['png', 'jpg', 'jpeg', 'gif', 'svg', 'webp'];
  const archive = ['zip', 'tar', 'gz', 'rar'];

  if (code.includes(extension)) return '#60a5fa';
  if (doc.includes(extension)) return '#fbbf24';
  if (sheet.includes(extension)) return '#34d399';
  if (image.includes(extension)) return '#f472b6';
  if (archive.includes(extension)) return '#a78bfa';
  if (extension === 'pptx' || extension === 'ppt') return '#fb923c';
  return '#9ca3af';
}

export function SearchOverlay({ allArtifacts, onClose, onOpenSession, open, sessions }: SearchOverlayProps) {
  const [query, setQuery] = useState('');
  const [phase, setPhase] = useState<'idle' | 'searching' | 'done'>('idle');
  const [sessionResults, setSessionResults] = useState<SearchResult[]>([]);
  const [artifactResults, setArtifactResults] = useState<SearchResult[]>([]);
  const [sessionOpen, setSessionOpen] = useState(true);
  const [artifactOpen, setArtifactOpen] = useState(true);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const searchTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (open) {
      document.body.style.overflow = 'hidden';
      setQuery('');
      setPhase('idle');
      setSessionResults([]);
      setArtifactResults([]);
      requestAnimationFrame(() => inputRef.current?.focus());
      return () => {
        document.body.style.overflow = '';
      };
    }
  }, [open]);

  const performSearch = (keyword: string) => {
    if (searchTimerRef.current) {
      clearTimeout(searchTimerRef.current);
    }

    if (!keyword.trim()) {
      setPhase('idle');
      setSessionResults([]);
      setArtifactResults([]);
      return;
    }

    setPhase('searching');
    setSessionResults([]);
    setArtifactResults([]);

    const lower = keyword.toLowerCase();

    searchTimerRef.current = setTimeout(() => {
      const matchedSessions = sessions
        .filter(
          (session) =>
            session.title.toLowerCase().includes(lower) ||
            session.lastMessage.toLowerCase().includes(lower),
        )
        .map((session) => ({
          id: `s-${session.id}`,
          title: session.title,
          subtitle: session.lastMessage || `${session.messageCount} 条消息`,
          kind: 'session' as const,
          targetId: session.id,
        }));

      const matchedArtifacts = allArtifacts
        .filter(
          (artifact) =>
            artifact.name.toLowerCase().includes(lower) ||
            artifact.path.toLowerCase().includes(lower) ||
            artifact.extension.toLowerCase().includes(lower),
        )
        .map((artifact) => ({
          id: `a-${artifact.id}`,
          title: artifact.name,
          subtitle: artifact.path,
          kind: 'artifact' as const,
          targetId: artifact.id,
          extension: artifact.extension,
        }));

      setSessionResults(matchedSessions);
      setArtifactResults(matchedArtifacts);
      setPhase('done');
    }, 500);
  };

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    performSearch(query);
  };

  const handleKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === 'Escape') {
      if (query && phase !== 'idle') {
        setQuery('');
        setPhase('idle');
        setSessionResults([]);
        setArtifactResults([]);
      } else {
        onClose();
      }
    }
  };

  const hasSearched = phase !== 'idle';
  const totalResults = sessionResults.length + artifactResults.length;

  return (
    <div className={`search-overlay ${open ? 'is-open' : ''}`} onClick={onClose}>
      <div
        className={`search-overlay-panel ${hasSearched ? 'has-results' : ''}`}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="search-overlay-header">
          <form className="search-input-group" onSubmit={handleSubmit}>
            <button className="search-submit-btn" type="submit" aria-label="搜索">
              <Search style={largeIconSize} />
            </button>
            <input
              ref={inputRef}
              className="search-overlay-input"
              placeholder="搜索会话、产物、文件..."
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={handleKeyDown}
            />
            {query && (
              <button
                className="search-clear-btn"
                type="button"
                aria-label="清除搜索"
                onClick={() => {
                  setQuery('');
                  setPhase('idle');
                  setSessionResults([]);
                  setArtifactResults([]);
                }}
              >
                <X style={{ width: 16, height: 16 }} />
              </button>
            )}
          </form>
          <button className="search-close-btn" type="button" aria-label="关闭搜索" onClick={onClose}>
            <X style={{ width: 20, height: 20 }} />
          </button>
        </div>

        <div className={`search-results-scroll ${hasSearched ? 'is-visible' : ''}`}>
          <div className="search-results-inner">
            {phase === 'searching' && (
              <div className="search-loading">
                <Loader2 style={{ width: 28, height: 28, animation: 'spin 0.8s linear infinite' }} />
                <span>正在搜索...</span>
              </div>
            )}

            {phase === 'done' && totalResults === 0 && (
              <div className="search-empty">
                <span className="search-empty-title">未找到匹配结果</span>
                <span className="search-empty-hint">请尝试使用其他关键词搜索</span>
              </div>
            )}

            {phase === 'done' && totalResults > 0 && (
              <div className="search-result-sections">
                {/* Session results */}
                <div className={`artifact-section ${sessionOpen ? 'is-open' : ''}`}>
                  <button
                    className="artifact-section-header"
                    type="button"
                    onClick={() => setSessionOpen((v) => !v)}
                  >
                    {sessionOpen ? <ChevronDown style={iconSize} /> : <ChevronRight style={iconSize} />}
                    <span className="artifact-section-title">对话历史</span>
                    <span className="artifact-section-count">{sessionResults.length}</span>
                  </button>
                  <div className="artifact-section-body">
                    <div className="artifact-section-inner">
                      {sessionResults.length === 0 ? (
                        <div className="artifact-section-empty">暂无匹配</div>
                      ) : (
                        sessionResults.map((result) => (
                          <button
                            className="search-result-item"
                            key={result.id}
                            type="button"
                            onClick={() => {
                              onOpenSession(result.targetId);
                              onClose();
                            }}
                          >
                            <span className="artifact-item-icon" style={{ color: '#60a5fa' }}>
                              <MessageSquare style={iconSize} />
                            </span>
                            <div className="artifact-item-info">
                              <span className="artifact-item-name">{result.title}</span>
                              <span className="artifact-item-path">{result.subtitle}</span>
                            </div>
                          </button>
                        ))
                      )}
                    </div>
                  </div>
                </div>

                {/* Artifact results */}
                <div className={`artifact-section ${artifactOpen ? 'is-open' : ''}`}>
                  <button
                    className="artifact-section-header"
                    type="button"
                    onClick={() => setArtifactOpen((v) => !v)}
                  >
                    {artifactOpen ? <ChevronDown style={iconSize} /> : <ChevronRight style={iconSize} />}
                    <span className="artifact-section-title">产物</span>
                    <span className="artifact-section-count">{artifactResults.length}</span>
                  </button>
                  <div className="artifact-section-body">
                    <div className="artifact-section-inner">
                      {artifactResults.length === 0 ? (
                        <div className="artifact-section-empty">暂无匹配</div>
                      ) : (
                        artifactResults.map((result) => (
                          <div className="search-result-item" key={result.id}>
                            <span className="artifact-item-icon" style={{ color: getFileColor(result.extension ?? '') }}>
                              {getFileIcon(result.extension ?? '')}
                            </span>
                            <div className="artifact-item-info">
                              <span className="artifact-item-name">{result.title}</span>
                              <span className="artifact-item-path">{result.subtitle}</span>
                            </div>
                          </div>
                        ))
                      )}
                    </div>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
