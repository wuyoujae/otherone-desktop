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
} from 'lucide-react';
import type { ReactNode } from 'react';
import { useState } from 'react';

const iconSize = { width: 16, height: 16 };

export type FileArtifact = {
  id: string;
  name: string;
  path: string;
  /** File extension without dot, e.g. "pdf", "tsx", "png" */
  extension: string;
  timestamp?: string;
};

type ArtifactSection = {
  id: string;
  title: string;
  items: FileArtifact[];
};

type ArtifactsPanelProps = {
  addedFiles: FileArtifact[];
  deletedFiles: FileArtifact[];
  editedFiles: FileArtifact[];
  open: boolean;
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

export function ArtifactsPanel({ addedFiles, deletedFiles, editedFiles, open }: ArtifactsPanelProps) {
  const sections: ArtifactSection[] = [
    { id: 'edited', title: '编辑文件', items: editedFiles },
    { id: 'deleted', title: '删除文件', items: deletedFiles },
    { id: 'added', title: '新增文件', items: addedFiles },
  ];

  return (
    <aside className={`artifacts-panel ${open ? 'is-open' : ''}`}>
      <div className="artifacts-panel-body">
        {sections.map((section) => (
          <ArtifactSection key={section.id} section={section} getFileIcon={getFileIcon} getFileColor={getFileColor} />
        ))}
      </div>
    </aside>
  );
}

type ArtifactSectionProps = {
  section: ArtifactSection;
  getFileIcon: (ext: string) => ReactNode;
  getFileColor: (ext: string) => string;
};

function ArtifactSection({ section, getFileIcon, getFileColor }: ArtifactSectionProps) {
  const [open, setOpen] = useState(true);

  return (
    <div className={`artifact-section ${open ? 'is-open' : ''}`}>
      <button className="artifact-section-header" type="button" onClick={() => setOpen((v) => !v)}>
        {open ? <ChevronDown style={iconSize} /> : <ChevronRight style={iconSize} />}
        <span className="artifact-section-title">{section.title}</span>
        <span className="artifact-section-count">{section.items.length}</span>
      </button>
      <div className="artifact-section-body">
        <div className="artifact-section-inner">
          {section.items.length === 0 ? (
            <div className="artifact-section-empty">暂无</div>
          ) : (
            section.items.map((item) => (
              <div className="artifact-item" key={item.id}>
                <span className="artifact-item-icon" style={{ color: getFileColor(item.extension) }}>
                  {getFileIcon(item.extension)}
                </span>
                <div className="artifact-item-info">
                  <span className="artifact-item-name">{item.name}</span>
                  <span className="artifact-item-path">{item.path}</span>
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
