import {
  Blocks,
  Box,
  ChevronDown,
  ChevronRight,
  Download,
  Loader2,
  Minus,
  Package,
  Plus,
  Puzzle,
  Search,
  Wrench,
  X,
} from 'lucide-react';
import { useState, useEffect, useMemo, useCallback } from 'react';
import { loadPluginList, installPlugin, uninstallPlugin, type PluginEntry } from '../services/pluginService';

const iconSize = { width: 16, height: 16 };

type TabKind = 'mcp' | 'skill' | 'plugin' | 'my';

type PluginsPageProps = { onClose: () => void };

const PLUGIN_ICON_COLORS = [
  '#60a5fa', '#fbbf24', '#34d399', '#f472b6', '#a78bfa',
  '#fb923c', '#4ade80', '#e879f9', '#38bdf8', '#f87171',
];

function getColor(name: string): string {
  let h = 0; for (let i = 0; i < name.length; i++) h = name.charCodeAt(i) + ((h << 5) - h);
  return PLUGIN_ICON_COLORS[Math.abs(h) % PLUGIN_ICON_COLORS.length];
}

// ═══════════════════════════════════════
// PluginItem
// ═══════════════════════════════════════

function PluginItem({
  plugin, compact, loading, onInstall, onUninstall,
}: {
  plugin: PluginEntry; compact?: boolean; loading?: boolean;
  onInstall: (p: PluginEntry) => void; onUninstall: (p: PluginEntry) => void;
}) {
  const accent = getColor(plugin.name);
  const isBusy = loading === true;

  return (
    <div className={`plugin-item ${compact ? 'plugin-item-compact' : ''}`}>
      <div className="plugin-item-logo" style={{ backgroundColor: `${accent}18`, color: accent }}>
        {plugin.kind === 'plugin' ? <Box style={{ width: 20, height: 20 }} /> : <Blocks style={{ width: 20, height: 20 }} />}
      </div>
      <div className="plugin-item-body">
        <span className="plugin-item-name">
          {plugin.name}
          {plugin.hasBinary && <span className="plugin-badge" title="包含可执行文件">⚙</span>}
        </span>
        <span className="plugin-item-desc">{plugin.description}</span>
      </div>
      <button
        className={`plugin-item-action ${plugin.installed ? 'state-added' : ''}`}
        type="button"
        disabled={isBusy}
        aria-label={plugin.installed ? '移除' : '安装'}
        onClick={() => plugin.installed ? onUninstall(plugin) : onInstall(plugin)}
      >
        {isBusy
          ? <Loader2 style={{ width: 16, height: 16, animation: 'spin 1s linear infinite' }} />
          : plugin.installed ? <Minus style={{ width: 16, height: 16 }} /> : <Plus style={{ width: 16, height: 16 }} />
        }
      </button>
    </div>
  );
}

// ═══════════════════════════════════════
// HeroCard
// ═══════════════════════════════════════

function HeroCard({ icon, label, desc }: { icon: React.ReactNode; label: string; desc: string }) {
  return (
    <div className="plugin-hero-card">
      <div className="plugin-hero-icon">{icon}</div>
      <div className="plugin-hero-body">
        <span className="plugin-hero-label">{label}</span>
        <span className="plugin-hero-desc">{desc}</span>
      </div>
    </div>
  );
}

// ═══════════════════════════════════════
// 插件描述组件（plugin detail，install 前展示）
// ═══════════════════════════════════════

function PluginDetail({
  plugin, loading, onInstall,
}: {
  plugin: PluginEntry; loading?: boolean; onInstall: (p: PluginEntry) => void;
}) {
  const accent = getColor(plugin.name);

  return (
    <div className="plugin-detail-card">
      <div className="plugin-detail-header">
        <div className="plugin-detail-logo" style={{ backgroundColor: `${accent}18`, color: accent }}>
          <Box style={{ width: 32, height: 32 }} />
        </div>
        <div className="plugin-detail-title">
          <h3>{plugin.name}</h3>
          <span className="plugin-detail-source">{plugin.source === 'builtin' ? '内置' : '第三方'}</span>
        </div>
      </div>
      <p className="plugin-detail-desc">{plugin.description}</p>
      <div className="plugin-detail-meta">
        {plugin.hasBinary && (
          <span className="plugin-meta-tag">📦 包含可执行文件 — 安装时将自动下载</span>
        )}
      </div>
      <button
        className="plugin-install-btn"
        type="button"
        disabled={loading}
        onClick={() => onInstall(plugin)}
      >
        {loading ? (
          <>
            <Loader2 style={{ width: 18, height: 18, animation: 'spin 1s linear infinite' }} />
            <span>正在下载安装…</span>
          </>
        ) : (
          <>
            <Download style={{ width: 18, height: 18 }} />
            <span>安装插件</span>
          </>
        )}
      </button>
    </div>
  );
}

// ═══════════════════════════════════════
// Main
// ═══════════════════════════════════════

export function PluginsPage({ onClose }: PluginsPageProps) {
  const [tab, setTab] = useState<TabKind>('plugin');
  const [search, setSearch] = useState('');
  const [plugins, setPlugins] = useState<PluginEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [installing, setInstalling] = useState<Set<string>>(new Set());
  const [mcpOpen, setMcpOpen] = useState(true);
  const [skillOpen, setSkillOpen] = useState(true);
  const [pluginOpen, setPluginOpen] = useState(true);

  const load = useCallback(async () => {
    try { setLoading(true); setError(''); setPlugins(await loadPluginList()); }
    catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setLoading(false); }
  }, []);

  useEffect(() => { void load(); }, [load]);

  const handleInstall = useCallback(async (p: PluginEntry) => {
    setInstalling(prev => new Set(prev).add(p.id));
    try { await installPlugin(p.name, p.kind); await load(); }
    catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setInstalling(prev => { const n = new Set(prev); n.delete(p.id); return n; }); }
  }, [load]);

  const handleUninstall = useCallback(async (p: PluginEntry) => {
    setInstalling(prev => new Set(prev).add(p.id));
    try { await uninstallPlugin(p.name, p.kind); await load(); }
    catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setInstalling(prev => { const n = new Set(prev); n.delete(p.id); return n; }); }
  }, [load]);

  const market = useMemo(() => plugins.filter(p => p.kind === tab || (tab === 'mcp' && p.kind === 'mcp')), [plugins, tab]);
  const installed = useMemo(() => plugins.filter(p => p.installed), [plugins]);

  const filtered = useMemo(() => {
    if (!search.trim()) return market;
    const q = search.toLowerCase();
    return market.filter(p => p.name.toLowerCase().includes(q) || p.description.toLowerCase().includes(q));
  }, [market, search]);

  const tabLabel = tab === 'mcp' ? 'MCP' : tab === 'skill' ? 'Skill' : '插件';

  return (
    <div className="plugins-page">
      {/* ---- Top nav ---- */}
      <div className="plugins-nav">
        <div className="plugins-nav-left">
          <div className="plugins-view-toggle">
            <button className={`plugins-toggle-btn ${tab === 'plugin' ? 'active' : ''}`} type="button" onClick={() => setTab('plugin')}>
              <Box style={iconSize} /><span>插件</span>
            </button>
            <button className={`plugins-toggle-btn ${tab === 'skill' ? 'active' : ''}`} type="button" onClick={() => setTab('skill')}>
              <Wrench style={iconSize} /><span>Skill</span>
            </button>
            <button className={`plugins-toggle-btn ${tab === 'mcp' ? 'active' : ''}`} type="button" onClick={() => setTab('mcp')}>
              <Puzzle style={iconSize} /><span>MCP</span>
            </button>
            <button className={`plugins-toggle-btn ${tab === 'my' ? 'active' : ''}`} type="button" onClick={() => setTab('my')}>
              <Package style={iconSize} /><span>我的插件</span>
            </button>
          </div>
        </div>

        <div className="plugins-nav-right">
          {tab !== 'my' && (
            <button className="plugins-nav-btn" type="button" onClick={() => load()}>
              <Plus style={{ width: 16, height: 16 }} /><span>刷新</span>
            </button>
          )}
          <button className="plugins-close-btn" type="button" aria-label="关闭" onClick={onClose}>
            <X style={{ width: 18, height: 18 }} />
          </button>
        </div>
      </div>

      {error && (
        <div className="plugins-error-bar">
          <span>{error}</span>
          <button type="button" onClick={() => { setError(''); load(); }}>重试</button>
        </div>
      )}

      {loading && <div className="plugins-loading">正在加载插件列表…</div>}

      {/* ---- Market ---- */}
      {!loading && (tab === 'plugin' || tab === 'skill' || tab === 'mcp') && (
        <div className="plugins-market">
          <div className="plugins-market-search">
            <Search style={{ width: 18, height: 18, color: 'var(--text-muted)' }} />
            <input className="plugins-search-input" placeholder={`搜索 ${tabLabel}…`} value={search}
              onChange={e => setSearch(e.target.value)} />
            {search && <button className="plugins-search-clear" type="button" onClick={() => setSearch('')}><X style={{ width: 14, height: 14 }} /></button>}
          </div>

          <div className="plugins-market-grid">
            {!search.trim() && (
              <>
                <HeroCard icon={<Download style={{ width: 22, height: 22 }} />} label={`导入 ${tabLabel}`}
                  desc={`从 URL 或文件导入 ${tabLabel}`} />
                <HeroCard icon={<Plus style={{ width: 22, height: 22 }} />} label={`创建 ${tabLabel}`}
                  desc={`基于模板创建新的 ${tabLabel}`} />
              </>
            )}

            {filtered.length === 0 ? (
              <div className="plugins-market-empty"><span>{search ? '未找到匹配的插件' : `暂无 ${tabLabel}`}</span></div>
            ) : tab === 'plugin' ? (
              // ── Plugin 卡片：大卡片详情样式 ──
              filtered.map(p => (
                <PluginDetail key={p.id} plugin={p} loading={installing.has(p.id)}
                  onInstall={handleInstall} />
              ))
            ) : (
              // ── Skill / MCP：紧凑列表样式 ──
              filtered.map(p => (
                <PluginItem key={p.id} plugin={p} compact loading={installing.has(p.id)}
                  onInstall={handleInstall} onUninstall={handleUninstall} />
              ))
            )}
          </div>
        </div>
      )}

      {/* ---- My plugins ---- */}
      {!loading && tab === 'my' && (
        <div className="plugins-body">
          <MySection title="插件" kind="plugin" open={pluginOpen} onToggle={() => setPluginOpen(v => !v)}
            plugins={installed} onInstall={handleInstall} onUninstall={handleUninstall} installing={installing} />
          <MySection title="Skill" kind="skill" open={skillOpen} onToggle={() => setSkillOpen(v => !v)}
            plugins={installed} onInstall={handleInstall} onUninstall={handleUninstall} installing={installing} />
          <MySection title="MCP" kind="mcp" open={mcpOpen} onToggle={() => setMcpOpen(v => !v)}
            plugins={installed} onInstall={handleInstall} onUninstall={handleUninstall} installing={installing} />
        </div>
      )}
    </div>
  );
}

function MySection({
  title, kind, open, onToggle, plugins, onInstall, onUninstall, installing,
}: {
  title: string; kind: string; open: boolean; onToggle: () => void;
  plugins: PluginEntry[]; onInstall: (p: PluginEntry) => void; onUninstall: (p: PluginEntry) => void;
  installing: Set<string>;
}) {
  const items = plugins.filter(p => p.kind === kind);
  return (
    <div className={`artifact-section ${open ? 'is-open' : ''}`}>
      <button className="artifact-section-header" type="button" onClick={onToggle}>
        {open ? <ChevronDown style={iconSize} /> : <ChevronRight style={iconSize} />}
        <span className="artifact-section-title">{title}</span>
        <span className="artifact-section-count">{items.length}</span>
      </button>
      <div className="artifact-section-body">
        <div className="artifact-section-inner">
          {items.length === 0 ? (
            <div className="plugin-item" style={{ opacity: 0.6 }}>暂无已安装的 {title}</div>
          ) : items.map(p => (
            <PluginItem key={p.id} plugin={p} loading={installing.has(p.id)}
              onInstall={onInstall} onUninstall={onUninstall} />
          ))}
        </div>
      </div>
    </div>
  );
}
