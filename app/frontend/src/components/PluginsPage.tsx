import {
  Blocks,
  ChevronDown,
  ChevronRight,
  Download,
  Minus,
  Package,
  Plus,
  Puzzle,
  Search,
  Wrench,
  X,
} from 'lucide-react';
import { useState, useRef, useEffect, useMemo } from 'react';

const iconSize = { width: 16, height: 16 };

// ═══════════════════════════════════════
// Types & mock data
// ═══════════════════════════════════════

type TabKind = 'mcp' | 'skill' | 'my';

type PluginEntry = {
  id: string;
  name: string;
  description: string;
  source: 'imported' | 'created';
  logoUrl?: string;
};

type PluginsPageProps = {
  onClose: () => void;
};

// Market mock data
const MARKET_MCP: PluginEntry[] = [
  { id: 'mcp-m1', name: 'Filesystem MCP', description: '提供本地文件系统读写能力，支持目录遍历与文件操作。', source: 'imported' },
  { id: 'mcp-m2', name: 'GitHub MCP', description: '操作 GitHub 仓库、Issue 与 PR，自动化 CI 触发器。', source: 'imported' },
  { id: 'mcp-m3', name: 'GitLab MCP', description: '管理 GitLab 项目、MR 及 Pipeline，支持 Webhook 回调。', source: 'imported' },
  { id: 'mcp-m4', name: 'Slack MCP', description: '发送消息、管理频道并监听 Slack 事件。', source: 'imported' },
  { id: 'mcp-m5', name: 'Notion MCP', description: '同步 Notion 数据库、页面与块内容，支持全文检索。', source: 'imported' },
  { id: 'mcp-m6', name: 'PostgreSQL MCP', description: '执行 SQL 查询、管理 schema 并导出数据集为 CSV。', source: 'imported' },
  { id: 'mcp-m7', name: 'Docker MCP', description: '管理容器生命周期、查看日志并获取资源使用统计。', source: 'imported' },
  { id: 'mcp-m8', name: 'Linear MCP', description: '创建和管理 Linear 工单、Sprint 及团队看板。', source: 'imported' },
  { id: 'mcp-m9', name: 'Figma MCP', description: '读取设计文件、导出资源并获取组件属性。', source: 'imported' },
  { id: 'mcp-m10', name: 'Jira MCP', description: '创建 Issue、管理看板与 Sprint，自动化工作流。', source: 'imported' },
];

const MARKET_SKILL: PluginEntry[] = [
  { id: 'sk-m1', name: 'Excel 分析', description: '自动读取 Excel 并生成可视化图表与摘要报告。', source: 'imported' },
  { id: 'sk-m2', name: '代码审查', description: '基于静态分析规则对变更代码进行审查并输出修改建议。', source: 'imported' },
  { id: 'sk-m3', name: '每日简报', description: '聚合当日新闻、任务与日历事件生成自然语言简报。', source: 'imported' },
  { id: 'sk-m4', name: '图片压缩', description: '批量压缩 PNG/JPG/WebP 图片，支持自定义质量与尺寸。', source: 'imported' },
  { id: 'sk-m5', name: '邮件模板', description: '基于 MJML 生成跨平台响应式邮件 HTML 模板。', source: 'imported' },
  { id: 'sk-m6', name: 'API 文档生成', description: '从 OpenAPI 规范自动生成 Markdown 文档与示例请求。', source: 'imported' },
  { id: 'sk-m7', name: 'PPT 生成', description: '通过自然语言描述生成演示文稿，支持多主题模板。', source: 'imported' },
  { id: 'sk-m8', name: '翻译助手', description: '多语言翻译，保留 Markdown 格式与代码块。', source: 'imported' },
  { id: 'sk-m9', name: 'SEO 分析', description: '分析网页 SEO 指标并输出优化建议报告。', source: 'imported' },
  { id: 'sk-m10', name: 'PDF 导出', description: '将 Markdown 或 HTML 文档转换为排版精美的 PDF 文件。', source: 'imported' },
];

// My (installed) plugins
const MY_MCP_LIST: PluginEntry[] = [
  { id: 'my-mcp-1', name: 'Filesystem MCP', description: '提供本地文件系统读写能力，支持目录遍历、文件增删改及路径解析。', source: 'imported' },
  { id: 'my-mcp-2', name: 'GitHub MCP', description: '操作 GitHub 仓库、Issue 与 PR，自动化代码审查及 CI 触发器。', source: 'imported' },
  { id: 'my-mcp-3', name: 'NewsCatcher MCP', description: '从 NewsCatcher API 拉取全球新闻索引，支持关键词过滤与日期范围。', source: 'created' },
];

const MY_SKILL_LIST: PluginEntry[] = [
  { id: 'my-sk-1', name: 'Excel 分析', description: '自动读取 Excel 表格并生成可视化图表与摘要报告。', source: 'imported' },
  { id: 'my-sk-2', name: '代码审查', description: '基于静态分析规则对变更代码进行审查并输出修改建议。', source: 'imported' },
  { id: 'my-sk-3', name: '每日简报', description: '聚合当日新闻、任务与日历事件，生成自然语言工作简报。', source: 'created' },
  { id: 'my-sk-4', name: 'PDF 导出', description: '将 Markdown 或 HTML 文档转换为排版精美的 PDF 文件。', source: 'created' },
];

// ═══════════════════════════════════════
// Helpers
// ═══════════════════════════════════════

const PLUGIN_ICON_COLORS = [
  '#60a5fa', '#fbbf24', '#34d399', '#f472b6', '#a78bfa',
  '#fb923c', '#4ade80', '#e879f9', '#38bdf8', '#f87171',
];

function getPluginIconColor(name: string): string {
  let hash = 0;
  for (let i = 0; i < name.length; i++) hash = name.charCodeAt(i) + ((hash << 5) - hash);
  return PLUGIN_ICON_COLORS[Math.abs(hash) % PLUGIN_ICON_COLORS.length];
}

// ═══════════════════════════════════════
// Add / Remove button
// ═══════════════════════════════════════

type AddButtonState = 'idle' | 'added' | 'removing';
const ADD_BUTTON_HOVER_DURATION = 1200;

function AddRemoveButton({ className }: { className?: string }) {
  const [state, setState] = useState<AddButtonState>('idle');
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const cleanup = () => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  };

  useEffect(() => cleanup, []);

  const handleClick = () => {
    cleanup();
    if (state === 'idle') {
      setState('added');
      timerRef.current = setTimeout(() => setState('idle'), ADD_BUTTON_HOVER_DURATION);
    } else if (state === 'added') {
      setState('removing');
    } else {
      setState('idle');
    }
  };

  return (
    <button
      className={`plugin-item-action ${className ?? ''}`}
      type="button"
      aria-label={state === 'idle' ? '添加' : state === 'added' ? '点击移除' : '已移除，点击恢复'}
      onClick={handleClick}
      data-state={state}
    >
      {state === 'idle' && <Plus style={{ width: 16, height: 16 }} />}
      {state === 'added' && <Minus style={{ width: 16, height: 16 }} />}
      {state === 'removing' && <Plus style={{ width: 16, height: 16 }} />}
    </button>
  );
}

// ═══════════════════════════════════════
// PluginItem
// ═══════════════════════════════════════

function PluginItem({ plugin, compact }: { plugin: PluginEntry; compact?: boolean }) {
  const accent = getPluginIconColor(plugin.name);

  return (
    <div className={`plugin-item ${compact ? 'plugin-item-compact' : ''}`}>
      <div className="plugin-item-logo" style={{ backgroundColor: `${accent}18`, color: accent }}>
        {plugin.logoUrl ? (
          <img src={plugin.logoUrl} alt={plugin.name} />
        ) : (
          <Blocks style={{ width: 20, height: 20 }} />
        )}
      </div>
      <div className="plugin-item-body">
        <span className="plugin-item-name">{plugin.name}</span>
        <span className="plugin-item-desc">{plugin.description}</span>
      </div>
      <AddRemoveButton />
    </div>
  );
}

// ═══════════════════════════════════════
// Hero card (import / create)
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
// Main component
// ═══════════════════════════════════════

export function PluginsPage({ onClose }: PluginsPageProps) {
  const [tab, setTab] = useState<TabKind>('mcp');
  const [createMenuOpen, setCreateMenuOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [mcpOpen, setMcpOpen] = useState(true);
  const [skillOpen, setSkillOpen] = useState(true);
  const createMenuRef = useRef<HTMLDivElement | null>(null);

  const marketPlugins = tab === 'mcp' ? MARKET_MCP : MARKET_SKILL;

  const filteredMarket = useMemo(() => {
    if (!searchQuery.trim()) return marketPlugins;
    const lower = searchQuery.toLowerCase();
    return marketPlugins.filter(
      (p) => p.name.toLowerCase().includes(lower) || p.description.toLowerCase().includes(lower),
    );
  }, [marketPlugins, searchQuery]);

  useEffect(() => {
    const handler = (e: PointerEvent) => {
      if (createMenuRef.current && !createMenuRef.current.contains(e.target as Node)) {
        setCreateMenuOpen(false);
      }
    };
    document.addEventListener('pointerdown', handler);
    return () => document.removeEventListener('pointerdown', handler);
  }, []);

  const kindLabel = tab === 'mcp' ? 'MCP' : 'Skill';

  return (
    <div className="plugins-page">
      {/* ---- Top nav ---- */}
      <div className="plugins-nav">
        <div className="plugins-nav-left">
          <div className="plugins-view-toggle">
            <button
              className={`plugins-toggle-btn ${tab === 'mcp' ? 'active' : ''}`}
              type="button"
              onClick={() => setTab('mcp')}
            >
              <Puzzle style={iconSize} />
              <span>MCP</span>
            </button>
            <button
              className={`plugins-toggle-btn ${tab === 'skill' ? 'active' : ''}`}
              type="button"
              onClick={() => setTab('skill')}
            >
              <Wrench style={iconSize} />
              <span>Skill</span>
            </button>
            <button
              className={`plugins-toggle-btn ${tab === 'my' ? 'active' : ''}`}
              type="button"
              onClick={() => setTab('my')}
            >
              <Package style={iconSize} />
              <span>我的插件</span>
            </button>
          </div>
        </div>

        <div className="plugins-nav-right">
          {tab !== 'my' && (
            <div className="plugins-create-wrap" ref={createMenuRef}>
              <button
                className="plugins-nav-btn plugins-create-btn"
                type="button"
                onClick={() => setCreateMenuOpen((v) => !v)}
              >
                <Plus style={{ width: 16, height: 16 }} />
                <span>创建</span>
              </button>
              <div className={`plugins-create-menu ${createMenuOpen ? 'is-open' : ''}`}>
                <button
                  className="plugins-create-menu-item"
                  type="button"
                  onClick={() => setCreateMenuOpen(false)}
                >
                  <Puzzle style={{ width: 14, height: 14 }} />
                  <span>创建 MCP</span>
                </button>
                <button
                  className="plugins-create-menu-item"
                  type="button"
                  onClick={() => setCreateMenuOpen(false)}
                >
                  <Wrench style={{ width: 14, height: 14 }} />
                  <span>创建 Skill</span>
                </button>
              </div>
            </div>
          )}

          <button className="plugins-close-btn" type="button" aria-label="关闭" onClick={onClose}>
            <X style={{ width: 18, height: 18 }} />
          </button>
        </div>
      </div>

      {/* ---- Market views ---- */}
      {(tab === 'mcp' || tab === 'skill') && (
        <div className="plugins-market">
          <div className="plugins-market-search">
            <Search style={{ width: 18, height: 18, color: 'var(--text-muted)' }} />
            <input
              className="plugins-search-input"
              placeholder={`搜索 ${kindLabel} 插件…`}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
            />
            {searchQuery && (
              <button className="plugins-search-clear" type="button" onClick={() => setSearchQuery('')}>
                <X style={{ width: 14, height: 14 }} />
              </button>
            )}
          </div>

          <div className="plugins-market-grid">
            {/* Hero cards — always first, not affected by search filtering */}
            {!searchQuery.trim() && (
              <>
                <HeroCard
                  icon={<Download style={{ width: 22, height: 22 }} />}
                  label={`导入新的 ${kindLabel}`}
                  desc={`从市场或 URL 导入第三方 ${kindLabel} 插件`}
                />
                <HeroCard
                  icon={<Plus style={{ width: 22, height: 22 }} />}
                  label={`创建新的 ${kindLabel}`}
                  desc={`基于模板创建你自己的 ${kindLabel} 插件`}
                />
              </>
            )}

            {filteredMarket.length === 0 ? (
              <div className="plugins-market-empty">
                <span>未找到匹配的插件</span>
                <span>尝试其他关键词搜索</span>
              </div>
            ) : (
              filteredMarket.map((plugin) => (
                <PluginItem key={plugin.id} plugin={plugin} compact />
              ))
            )}
          </div>
        </div>
      )}

      {/* ---- My plugins view ---- */}
      {tab === 'my' && (
        <div className="plugins-body">
          {/* Installed MCP */}
          <div className={`artifact-section ${mcpOpen ? 'is-open' : ''}`}>
            <button
              className="artifact-section-header"
              type="button"
              onClick={() => setMcpOpen((v) => !v)}
            >
              {mcpOpen ? <ChevronDown style={iconSize} /> : <ChevronRight style={iconSize} />}
              <span className="artifact-section-title">MCP</span>
              <span className="artifact-section-count">{MY_MCP_LIST.length}</span>
            </button>
            <div className="artifact-section-body">
              <div className="artifact-section-inner">
                {MY_MCP_LIST.map((plugin) => (
                  <PluginItem key={plugin.id} plugin={plugin} />
                ))}
              </div>
            </div>
          </div>

          {/* Installed Skill */}
          <div className={`artifact-section ${skillOpen ? 'is-open' : ''}`}>
            <button
              className="artifact-section-header"
              type="button"
              onClick={() => setSkillOpen((v) => !v)}
            >
              {skillOpen ? <ChevronDown style={iconSize} /> : <ChevronRight style={iconSize} />}
              <span className="artifact-section-title">Skill</span>
              <span className="artifact-section-count">{MY_SKILL_LIST.length}</span>
            </button>
            <div className="artifact-section-body">
              <div className="artifact-section-inner">
                {MY_SKILL_LIST.map((plugin) => (
                  <PluginItem key={plugin.id} plugin={plugin} />
                ))}
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
