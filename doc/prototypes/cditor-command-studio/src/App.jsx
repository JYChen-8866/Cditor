import { useEffect, useMemo, useState } from "react";
import {
  Archive, CaretDown, CaretRight, CaretUp, Check, CirclesThreePlus,
  Code, Copy, DotsSixVertical, DotsThree, File, Folder, Gear, House, Link,
  ListBullets, MagnifyingGlass, Minus, Quotes, SidebarSimple, Sparkle,
  TextB, TextHOne, TextHTwo, TextItalic, TextStrikethrough, UserCircle,
} from "@phosphor-icons/react";

const pages = ["编辑器架构重构", "性能基准", "协作协议", "插件系统", "发布计划"];
const outline = [
  "1. 重构目标", "2. 关键任务清单", "3. 渲染管线示例 (TypeScript)",
  "4. 性能目标", "4.1 打开性能", "4.2 编辑性能", "4.3 内存占用",
  "5. 协作协议", "5.1 数据合并策略", "5.2 冲突处理", "5.3 离线协作流程",
  "6. 实施计划", "7. 风险与应对",
];
const commandGroups = [
  {
    title: "转换为",
    items: [
      [TextHTwo, "标题 2", "⌘ 2", true], [ListBullets, "要点列表", "⌘ ⇧ 8"],
      [ListBullets, "编号列表", "⌘ ⇧ 7"], [Quotes, "引用块", "⌘ ⇧ 9"],
      [Code, "代码块", "⌘ ⇧ `"],
    ],
  },
  { title: "插入", items: [[CirclesThreePlus, "插入下方", "⌘ ↵"], [Minus, "插入分隔线", "⌘ ⇧ -"]] },
];

function WindowBar({ sidebarOpen, setSidebarOpen }) {
  return (
    <header className="window-bar">
      <div className="traffic-lights" aria-label="窗口控制">
        <button className="traffic close" aria-label="关闭窗口" />
        <button className="traffic minimize" aria-label="最小化窗口" />
        <button className="traffic maximize" aria-label="全屏窗口" />
      </div>
      <button className="mobile-sidebar-toggle" onClick={() => setSidebarOpen(!sidebarOpen)}>
        <SidebarSimple size={17} />
      </button>
      <div className="window-path">
        <span>技术文档库</span><span className="path-separator">/</span>
        <button>编辑器架构重构 <CaretDown size={13} /></button>
      </div>
      <div className="window-spacer" />
      <div className="save-state"><i />本地已保存</div>
      <time>今天 10:24</time><kbd>⌘ K</kbd>
    </header>
  );
}

function Sidebar({ open, onToggle }) {
  const [folderOpen, setFolderOpen] = useState(true);
  const [selected, setSelected] = useState(pages[0]);
  if (!open) {
    return (
      <aside className="sidebar sidebar-collapsed">
        <div className="brand-mark">C</div>
        {[House, MagnifyingGlass, Archive, File, Gear].map((Icon, index) => (
          <button key={index} aria-label="导航"><Icon size={20} /></button>
        ))}
        <button className="collapsed-avatar" onClick={onToggle} aria-label="展开侧边栏">
          <img src="/avatars/zhang-mingyuan.png" alt="张明远" />
        </button>
      </aside>
    );
  }
  return (
    <aside className="sidebar">
      <div className="sidebar-head">
        <div className="brand-mark">C</div><strong>Cditor</strong>
        <button onClick={onToggle} aria-label="收起侧边栏"><SidebarSimple size={19} /></button>
      </div>
      <label className="search-box">
        <MagnifyingGlass size={17} /><input aria-label="搜索文档" placeholder="搜索文档、页面和模板" /><kbd>⌘F</kbd>
      </label>
      <nav className="primary-nav">
        {[[House, "首页"], [MagnifyingGlass, "搜索"], [Archive, "收件箱"], [File, "模板"]].map(([Icon, label]) => (
          <button key={label}><Icon size={19} /><span>{label}</span></button>
        ))}
      </nav>
      <div className="sidebar-divider" />
      <section className="tree-section">
        <div className="section-title">
          <strong>技术文档库</strong>
          <span><button aria-label="新建页面">+</button><button><CaretUp size={14} /></button></span>
        </div>
        <button className="tree-folder" onClick={() => setFolderOpen(!folderOpen)}>
          {folderOpen ? <CaretDown size={13} /> : <CaretRight size={13} />}<Folder size={18} /><span>架构设计</span>
        </button>
        {folderOpen && pages.map((page) => (
          <button className={`tree-page ${selected === page ? "active" : ""}`} key={page} onClick={() => setSelected(page)}>
            <File size={17} /><span>{page}</span>
          </button>
        ))}
        {["开发指南", "API 参考", "运维手册", "项目管理"].map((folder) => (
          <button className="tree-folder secondary" key={folder}>
            <CaretRight size={13} /><Folder size={18} /><span>{folder}</span>
          </button>
        ))}
      </section>
      <div className="sidebar-bottom">
        <button className="account">
          <span className="avatar-wrap"><img src="/avatars/zhang-mingyuan.png" alt="" /><i /></span>
          <span>张明远</span><CaretDown size={14} />
        </button>
        <button><Gear size={20} /><span>设置</span></button>
      </div>
    </aside>
  );
}

function FormattingToolbar() {
  return (
    <div className="formatting-toolbar">
      {[[TextB, "加粗"], [TextItalic, "斜体"], [TextStrikethrough, "删除线"], [Code, "行内代码"], [Link, "链接"], [TextHOne, "文本样式"], [ListBullets, "列表"]].map(([Icon, label]) => (
        <button key={label} aria-label={label} title={label}><Icon size={19} /></button>
      ))}
      <span /><button aria-label="更多"><DotsThree size={20} weight="bold" /></button>
    </div>
  );
}

function CommandPalette({ onClose }) {
  const [query, setQuery] = useState("");
  const filtered = useMemo(() => commandGroups.map((group) => ({
    ...group, items: group.items.filter(([, label]) => label.includes(query)),
  })), [query]);
  return (
    <div className="command-palette" role="dialog" aria-label="命令面板">
      <label><input autoFocus value={query} onChange={(e) => setQuery(e.target.value)} placeholder="命令" /><kbd>⌘K</kbd></label>
      {filtered.map((group) => (
        <section key={group.title}>
          <strong>{group.title}</strong>
          {group.items.map(([Icon, label, shortcut, active]) => (
            <button key={label} className={active ? "active" : ""} onClick={onClose}>
              <Icon size={17} /><span>{label}</span><kbd>{shortcut}</kbd>
            </button>
          ))}
        </section>
      ))}
      <div className="command-actions">
        <button onClick={onClose}><UserCircle size={18} /> 添加评论 <kbd>⌘ ⇧ M</kbd></button>
        <button onClick={onClose}><Sparkle size={18} /> AI 改写 <kbd>⌘ ⇧ I</kbd></button>
      </div>
      <footer><span>↑↓ 选择</span><span>↵ 确认</span><button onClick={onClose}>esc 关闭</button></footer>
    </div>
  );
}

function ArchitectureCallout() {
  return (
    <div className="architecture-callout">
      <div className="architecture-icon">
        <span /><i className="node top" /><i className="node left" /><i className="node middle" /><i className="node right" />
      </div>
      <div><strong>架构概览</strong><p>编辑器采用分层架构：渲染层、文档模型层、服务层与基础设施层。</p><p>各层通过清晰的接口隔离，确保可测试性与可替换性。</p></div>
    </div>
  );
}

function Checklist() {
  const [items, setItems] = useState([
    ["梳理现有模块边界与依赖关系", true], ["设计新的文档模型与渲染管线", true],
    ["实现增量渲染与虚拟化策略", false], ["建立插件系统与权限模型", false], ["完善测试与性能基准", false],
  ]);
  return (
    <div className="checklist">
      {items.map(([label, checked], index) => (
        <button key={label} onClick={() => setItems((current) => current.map((item, i) => i === index ? [item[0], !item[1]] : item))}>
          <span className={checked ? "checked" : ""}>{checked && <Check size={13} weight="bold" />}</span>{label}
        </button>
      ))}
    </div>
  );
}

function CodeBlock() {
  const code = `export function renderPipeline(doc: Doc) {
  const tree = buildTree(doc);
  const layout = layoutTree(tree);
  for (const node of layout.visibleNodes()) {
    draw(node);
  }
}`;
  const [copied, setCopied] = useState(false);
  return (
    <div className="code-block">
      <div className="code-head"><span>TypeScript</span>
        <button onClick={async () => { await navigator.clipboard?.writeText(code); setCopied(true); setTimeout(() => setCopied(false), 1200); }}>
          {copied ? <Check size={16} /> : <Copy size={16} />}{copied ? "已复制" : "复制"}
        </button>
      </div>
      <pre><code>{code}</code></pre>
    </div>
  );
}

function Document({ commandOpen, setCommandOpen }) {
  return (
    <main className="document-shell">
      <article className="document">
        <header className="document-header">
          <h1>编辑器架构重构</h1>
          <div className="document-meta"><span>作者：张明远</span><i /><span>创建：2026-07-16</span><i /><span>更新：2026-07-16</span><i /><span>状态：草稿</span></div>
        </header>
        <p className="intro">本文档记录编辑器架构重构的目标、设计方案与实施计划。重构以提升性能、扩展性与可维护性为核心，同时保持本地优先与离线可用的特性。</p>
        <h2 id="goal">1. 重构目标</h2>
        <div className={`selected-block ${commandOpen ? "is-open" : ""}`} onClick={() => setCommandOpen(true)}>
          <button className="drag-handle"><DotsSixVertical size={19} /></button><FormattingToolbar />
          <p>在保证数据本地优先的前提下，显著提升大文档的编辑性能，优化内存占用，并建立清晰的模块边界与插件扩展机制，以支持未来更多能力的快速迭代。<span className="caret" /></p>
        </div>
        <ArchitectureCallout /><h2>2. 关键任务清单</h2><Checklist />
        <h2>3. 渲染管线示例（TypeScript）</h2><CodeBlock />
      </article>
      {commandOpen && <CommandPalette onClose={() => setCommandOpen(false)} />}
    </main>
  );
}

function Outline({ open, onToggle }) {
  if (!open) return <aside className="outline collapsed"><button onClick={onToggle}><CaretRight size={18} /></button></aside>;
  return (
    <aside className="outline">
      <div className="outline-title"><strong>文档大纲</strong><button onClick={onToggle}><CaretUp size={16} /></button></div>
      <nav>{outline.map((item, index) => (
        <a className={`${index === 0 ? "active" : ""} ${item.startsWith("4.") || item.startsWith("5.") ? "nested" : ""}`} href={index === 0 ? "#goal" : "#"} key={item}>{item}</a>
      ))}</nav>
      <div className="collaborators"><strong>协作者</strong><div>
        {[["/avatars/zhang-mingyuan.png", "张明远"], ["/avatars/li-ran.png", "李冉"], ["/avatars/wang-tao.png", "王涛"]].map(([src, name]) => (
          <span className="collaborator-avatar" key={name}><img src={src} alt={name} /><i /></span>
        ))}<button>+2</button>
      </div></div>
    </aside>
  );
}

export function App() {
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [outlineOpen, setOutlineOpen] = useState(true);
  const [commandOpen, setCommandOpen] = useState(true);
  useEffect(() => {
    const handler = (event) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault(); setCommandOpen((current) => !current);
      }
      if (event.key === "Escape") setCommandOpen(false);
    };
    window.addEventListener("keydown", handler); return () => window.removeEventListener("keydown", handler);
  }, []);
  return (
    <div className="desktop-stage">
      <section className="app-window">
        <WindowBar sidebarOpen={sidebarOpen} setSidebarOpen={setSidebarOpen} />
        <div className="app-body" style={{ "--sidebar-width": sidebarOpen ? "286px" : "72px", "--outline-width": outlineOpen ? "260px" : "42px" }}>
          <Sidebar open={sidebarOpen} onToggle={() => setSidebarOpen(!sidebarOpen)} />
          <Document commandOpen={commandOpen} setCommandOpen={setCommandOpen} />
          <Outline open={outlineOpen} onToggle={() => setOutlineOpen(!outlineOpen)} />
        </div>
      </section>
    </div>
  );
}
