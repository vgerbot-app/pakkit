/* ══════════════════════════════════════════════════════════════
   应用逻辑：路由 / 事件 / 队列调度 / 源管理
   ══════════════════════════════════════════════════════════════ */

let uidSeq = 0;

/* ── 侧边栏 ── */
function renderSidebar() {
  const items = [
    { route: 'home',       label: '发现',     icon: 'home' },
    { route: 'categories', label: '分类',     icon: 'grid' },
    { route: 'search',     label: '全部包',   icon: 'search' },
    { route: 'installed',  label: '已安装',   icon: 'check' },
    { route: 'updates',    label: '更新',     icon: 'refresh' },
    { route: 'sources',    label: '源管理',   icon: 'source' },
    { route: 'settings',   label: '设置',     icon: 'settings' },
  ];
  const active = S.route === 'category' ? 'categories' : S.route;
  $('#sidebarNav').innerHTML = items.map(i => `
    <button class="nav-item ${active === i.route ? 'active' : ''}" data-route="${i.route}" data-tip="${i.label}">
      <span class="ico">${icon(i.icon, 'w-4 h-4')}</span>
      <span class="lbl">${i.label}</span>
    </button>`).join('');
  $('#statusSources').textContent = SOURCES.length;
  $('#globalSourceLabel').textContent = S.searchSource === 'all' ? '全部源' : S.searchSource;
}

/* ── 路由 ── */
function navigate(route, opts = {}) {
  if (route === 'category') S.category = opts.id;
  else if (route !== 'category') S.category = route === 'category' ? S.category : null;
  S.route = route;
  renderSidebar();
  render();
  $('#view').scrollTop = 0;
}

function render() {
  let html;
  switch (S.route) {
    case 'categories': html = viewCategories(); break;
    case 'category':   html = viewCategory(S.category); break;
    case 'search':     html = viewSearch(); break;
    case 'installed':  html = viewInstalled(); break;
    case 'updates':    html = viewUpdates(); break;
    case 'sources':    html = viewSources(); break;
    case 'settings':   html = viewSettings(); break;
    default:           html = viewHome();
  }
  $('#view').innerHTML = html;
  hydrateIcons($('#view'));
  $('#statusText').textContent = {
    home: '就绪', categories: `分类 ${CATEGORIES.length}`, category: `分类 · ${CATEGORIES.find(c => c.id === S.category)?.name || ''}`,
    search: S.query ? `搜索：${S.query}` : '全部软件包', installed: '已安装管理', updates: '检查更新',
    sources: `源 ${SOURCES.length}`, settings: '设置',
  }[S.route] || '就绪';
}

/* ── 全局搜索输入 ── */
function bindGlobalSearch() {
  const input = $('#globalSearch');
  let t;
  input.addEventListener('input', () => {
    clearTimeout(t);
    t = setTimeout(() => {
      S.query = input.value;
      if (S.route !== 'search') navigate('search');
      else render();
    }, 180);
  });
  input.addEventListener('keydown', e => { if (e.key === 'Enter') { S.query = input.value; navigate('search'); } });
  document.addEventListener('keydown', e => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') { e.preventDefault(); input.focus(); input.select(); }
    if (e.key === 'Escape' && document.activeElement === input) input.blur();
  });
}

/* ── 安装队列调度（模拟） ── */
function enqueue(id, opts = {}) {
  const p = PACKAGES.find(x => x.id === id);
  if (!p) return;
  if (S.queue.some(q => q.id === id && (q.state === 'pending' || q.state === 'running'))) {
    toast(`${p.name} 已在队列中`, 'warn'); return;
  }
  const uid = `q${++uidSeq}`;
  S.queue.push({ uid, id: p.id, name: p.name, icon: p.icon, color: p.color, state: 'pending', progress: 0, log: '等待中…' });
  openQueue(true);
  renderQueue();
  if (S.route === 'search' || S.route === 'category' || S.route === 'home') render();
  toast(`已加入队列：${p.name}`, 'info', 2000);
  pump();
}

function pump() {
  const active = S.queue.filter(q => q.state === 'running').length;
  if (active >= S.settings.concurrency) return;
  const next = S.queue.find(q => q.state === 'pending');
  if (!next) return;
  next.state = 'running';
  next.progress = 0;
  const args = buildArgs(next);
  next.log = `winget ${args}`;
  renderQueue();

  const timer = setInterval(() => {
    next.progress += Math.round(4 + Math.random() * 12);
    if (next.progress >= 100) {
      next.progress = 100;
      clearInterval(timer);
      // 5% 概率模拟失败，展示错误态
      if (Math.random() < 0.05 && !opts_safe(next)) {
        next.state = 'error';
        next.log = '安装失败：退出码 0x8A150011 (Package install failed)。请检查权限或网络后重试。';
      } else {
        next.state = 'done';
        next.log = `安装成功 · 用时 ${(1.4 + Math.random() * 2).toFixed(1)}s`;
        const pkg = PACKAGES.find(x => x.id === next.id);
        if (pkg) pkg.installed = true;
      }
      renderQueue();
      if (['installed','home','search','category','updates'].includes(S.route)) render();
      setTimeout(pump, 250);
      return;
    }
    renderQueue();
  }, 420);
}

// 演示用：特定包必失败，方便审阅错误态（如 intel 驱动）
function opts_safe(q) { return q.id === 'Intel.IntelDriver'; }

function buildArgs(q) {
  const st = S.settings;
  const parts = ['install', '--id', q.id, '--source', st.defaultSource, '--architecture', st.arch, '--scope', st.scope];
  if (st.silent) parts.push('--silent');
  if (st.acceptAgreements) parts.push('--accept-package-agreements', '--accept-source-agreements');
  if (st.installLocation.trim()) parts.push('--location', `"${st.installLocation.trim()}"`);
  return parts.join(' ');
}

function openQueue(open) {
  const panel = $('#queuePanel');
  panel.classList.toggle('hidden', !open);
  panel.classList.toggle('w-[360px]', open);
  panel.classList.toggle('animate-slideIn', open);
}

/* ── 源管理动作 ── */
function addSourceDialog() {
  modal({
    title: '添加 winget 源', subtitle: 'winget source add', icon: 'source',
    body: `
      <div class="space-y-3.5">
        <label class="block">
          <span class="text-[12px] font-medium">源名称</span>
          <input id="srcName" class="field mt-1.5" placeholder="MyCompany" />
        </label>
        <label class="block">
          <span class="text-[12px] font-medium">源地址 (URL)</span>
          <input id="srcUrl" class="field mt-1.5 font-mono text-[12px]" placeholder="https://nexus.internal/repository/winget" />
        </label>
        <label class="block">
          <span class="text-[12px] font-medium">类型</span>
          <select id="srcType" class="field mt-1.5">
            <option>Microsoft.PreIndexed.Package</option>
            <option>Microsoft.Rest</option>
          </select>
        </label>
        <div class="flex items-start gap-2.5 p-3 rounded-xl bg-surface2 border border-line text-[11.5px] text-muted leading-relaxed">
          ${icon('shield','w-4 h-4 text-warn shrink-0 mt-px')}
          <span>添加第三方源前请确认其可信。未验证的源可能提供篡改过的安装包。</span>
        </div>
      </div>`,
    footer: `<button class="btn btn-outline" data-close>取消</button>
      <button class="btn btn-primary" id="srcAdd">${icon('plus','w-4 h-4')}添加源</button>`,
  });
  $('#srcAdd').addEventListener('click', () => {
    const name = $('#srcName').value.trim(), url = $('#srcUrl').value.trim(), type = $('#srcType').value;
    if (!name || !url) { toast('请填写源名称与地址', 'error'); return; }
    if (SOURCES.some(s => s.name === name)) { toast(`源 ${name} 已存在`, 'error'); return; }
    SOURCES.push({ name, url, type, trusted: false, updated: new Date().toISOString().slice(0, 16).replace('T', ' '), packages: 0 });
    closeModal(); render(); toast(`已添加源 ${name}`, 'ok');
  });
}

function editSourceDialog(name) {
  const s = SOURCES.find(x => x.name === name);
  modal({
    title: `编辑源 · ${name}`, subtitle: 'winget source update', icon: 'source',
    body: `
      <div class="space-y-3.5">
        <label class="block"><span class="text-[12px] font-medium">源地址 (URL)</span>
          <input id="esUrl" class="field mt-1.5 font-mono text-[12px]" value="${s.url}" /></label>
        <label class="block"><span class="text-[12px] font-medium">类型</span>
          <select id="esType" class="field mt-1.5">
            <option ${s.type.includes('PreIndexed') ? 'selected' : ''}>Microsoft.PreIndexed.Package</option>
            <option ${s.type.includes('Rest') ? 'selected' : ''}>Microsoft.Rest</option>
          </select></label>
      </div>`,
    footer: `<button class="btn btn-outline" data-close>取消</button>
      <button class="btn btn-primary" id="esSave">保存</button>`,
  });
  $('#esSave').addEventListener('click', () => {
    s.url = $('#esUrl').value.trim(); s.type = $('#esType').value;
    closeModal(); render(); toast(`已更新源 ${name}`, 'ok');
  });
}

function removeSource(name) {
  const idx = SOURCES.findIndex(s => s.name === name);
  if (idx < 0) return;
  confirmDialog({
    title: '移除源', danger: true, confirmText: '移除',
    message: `确定移除源 <b>${name}</b>？已通过该源安装的软件不受影响，但将无法再从该源更新。`,
    onConfirm: () => { SOURCES.splice(idx, 1); if (S.settings.defaultSource === name) S.settings.defaultSource = SOURCES[0]?.name || ''; render(); toast(`已移除源 ${name}`, 'ok'); },
  });
}

function syncSources(names) {
  const list = names ? names : SOURCES.map(s => s.name);
  toast(`正在更新 ${list.length} 个源…`, 'info', 1500);
  setTimeout(() => {
    SOURCES.forEach(s => { if (list.includes(s.name)) s.updated = new Date().toISOString().slice(0, 16).replace('T', ' '); });
    if (S.route === 'sources') render();
    toast(`${list.length} 个源已更新至最新索引`, 'ok');
  }, 900);
}

/* ── 卸载 ── */
function uninstall(id) {
  const p = PACKAGES.find(x => x.id === id);
  if (!p) return;
  confirmDialog({
    title: '卸载软件', danger: true, confirmText: '卸载',
    message: `确定卸载 <b>${p.name}</b>？<br/><code class="font-mono text-[11.5px] text-muted">winget uninstall --id ${p.id}</code>`,
    onConfirm: () => { p.installed = false; closeModal(); render(); toast(`已卸载 ${p.name}`, 'ok'); },
  });
}

/* ── 主题 ── */
function applyTheme() {
  const dark = S.theme === 'dark' || (S.theme === 'system' && matchMedia('(prefers-color-scheme: dark)').matches);
  document.documentElement.classList.toggle('dark', dark);
  document.documentElement.classList.toggle('light', !dark);
  document.documentElement.style.setProperty('--accent', S.settings.accent);
}
function toggleTheme() {
  const order = ['dark', 'light', 'system'];
  S.theme = order[(order.indexOf(S.theme) + 1) % 3];
  S.settings.theme = S.theme;
  applyTheme();
  toast(`主题：${{ dark: '深色', light: '浅色', system: '跟随系统' }[S.theme]}`, 'info', 1600);
  if (S.route === 'settings') render();
}

/* ── 事件委托 ── */
function bindEvents() {
  document.addEventListener('click', e => {
    const t = e.target;

    const routeEl = t.closest('[data-route]');
    if (routeEl) { navigate(routeEl.dataset.route); return; }

    const catEl = t.closest('[data-cat]');
    if (catEl) { navigate('category', { id: catEl.dataset.cat }); return; }

    const inst = t.closest('[data-install]');
    if (inst) { e.stopPropagation(); enqueue(inst.dataset.install); return; }

    const uninst = t.closest('[data-uninstall]');
    if (uninst) { e.stopPropagation(); uninstall(uninst.dataset.uninstall); return; }

    const pkgEl = t.closest('[data-pkg]');
    if (pkgEl) { openPkgDetail(pkgEl.dataset.pkg); return; }

    const copyEl = t.closest('[data-copy]');
    if (copyEl) {
      navigator.clipboard?.writeText(copyEl.dataset.copy);
      toast('命令已复制到剪贴板', 'ok', 1600);
      return;
    }

    const srcCycle = t.closest('[data-src-cycle]');
    if (srcCycle) {
      const opts = ['winget', 'msstore', 'all'];
      S.searchSource = opts[(opts.indexOf(S.searchSource) + 1) % opts.length];
      renderSidebar();
      if (S.route === 'search') render();
      toast(`搜索源：${S.searchSource === 'all' ? '全部源' : S.searchSource}`, 'info', 1600);
      return;
    }

    const sortEl = t.closest('[data-sort] button');
    if (sortEl) { S.sort = sortEl.dataset.sort; render(); return; }

    const srcSel = t.closest('[data-searchsrc]');
    if (srcSel) { S.searchSource = srcSel.dataset.searchsrc; renderSidebar(); render(); return; }

    const filterEl = t.closest('[data-filter] button');
    if (filterEl) { S.filter = filterEl.dataset.filter; render(); return; }

    const toggle = t.closest('[data-toggle]');
    if (toggle) {
      const key = toggle.dataset.toggle;
      S.settings[key] = !S.settings[key];
      toggle.classList.toggle('on', S.settings[key]);
      toggle.setAttribute('aria-checked', S.settings[key]);
      if (key === 'reduceMotion') document.documentElement.style.setProperty('scroll-behavior', S.settings.reduceMotion ? 'auto' : 'smooth');
      return;
    }

    const segSet = t.closest('[data-setseg] button');
    if (segSet) {
      const key = segSet.closest('[data-setseg]').dataset.setseg;
      const val = segSet.dataset.val;
      S.settings[key] = isNaN(+val) || val === '' ? val : +val;
      if (key === 'theme') { S.theme = val; applyTheme(); }
      render();
      return;
    }

    const qRemove = t.closest('[data-queue-remove],[data-queue-cancel]');
    if (qRemove) {
      const uid = qRemove.dataset.queueRemove || qRemove.dataset.queueCancel;
      S.queue = S.queue.filter(q => q.uid !== uid);
      renderQueue(); render(); renderSidebar();
      return;
    }

    const action = t.closest('[data-action]')?.dataset.action;
    if (action) handleAction(action, t.closest('[data-action]'));
  });

  // 设置的输入控件
  document.addEventListener('input', e => {
    const el = e.target;
    if (el.dataset?.set) { S.settings[el.dataset.set] = el.value; }
  });
  document.addEventListener('change', e => {
    const el = e.target;
    if (el.tagName === 'SELECT' && el.dataset?.set) { S.settings[el.dataset.set] = el.value; }
  });
}

function handleAction(action, el) {
  switch (action) {
    case 'toggle-theme': toggleTheme(); break;
    case 'close-queue':  openQueue(false); break;
    case 'queue-clear-done':
      S.queue = S.queue.filter(q => q.state !== 'done'); renderQueue(); render(); break;
    case 'add-source':   addSourceDialog(); break;
    case 'edit-source':  editSourceDialog(el.dataset.name); break;
    case 'remove-source': removeSource(el.dataset.name); break;
    case 'sync-source':  syncSources([el.dataset.name]); break;
    case 'sync-sources': syncSources(); break;
    case 'set-default-src':
      S.settings.defaultSource = el.dataset.name; render(); toast(`默认源已设为 ${el.dataset.name}`, 'ok'); break;
    case 'uninstall-pkg': uninstall(el.dataset.name); break;
    case 'upgrade-all':
      PACKAGES.filter(p => p.installed).slice(0, 4).forEach(p => enqueue(p.id));
      toast('已将全部更新加入队列', 'info'); break;
    case 'install-category':
      PACKAGES.filter(p => p.cat === S.category && !p.installed).forEach(p => enqueue(p.id));
      break;
    case 'export': {
      const list = PACKAGES.filter(p => p.installed).map(p => p.id);
      modal({
        title: '导出已安装清单', subtitle: '可复制保存为 winget import 配置', icon: 'file',
        body: `<p class="text-[12px] text-muted mb-3">等价命令：<code class="font-mono px-1.5 py-0.5 rounded bg-surface2 border border-line">winget export -o packages.json</code></p>
          <pre class="text-[11.5px] font-mono leading-relaxed p-3.5 rounded-xl bg-surface2 border border-line overflow-auto text-ink2">{
  "$schema": "https://aka.ms/winget-packages.schema.2.0.json",
  "Sources": [ { "Packages": [
${list.map(id => `    { "PackageIdentifier": "${id}" }`).join(',\n')}
  ], "SourceDetails": { "Name": "winget", "Argument": "https://cdn.winget.microsoft.com/cache" } } ]
}</pre>`,
        footer: `<button class="btn btn-outline" data-close>关闭</button>
          <button class="btn btn-primary" data-copy='{"$schema":"winget-packages"}' data-close>${icon('file','w-4 h-4')}复制 JSON</button>`,
      });
      break;
    }
    case 'advanced-search':
      modal({
        title: '高级筛选', subtitle: '映射为 winget search 参数', icon: 'filter',
        body: `<div class="space-y-3.5">
          ${[['--moniker', '别名匹配'], ['--tag', '按标签过滤'], ['--exact', '精确匹配 ID'], ['--command', '按提供的命令搜索']]
            .map(([f, d]) => `
            <label class="flex items-center gap-3 p-3 rounded-xl border border-line cursor-pointer hover:border-line2">
              <input type="checkbox" class="accent-[var(--accent)]" checked />
              <span class="font-mono text-[12px] text-accent">${f}</span>
              <span class="text-[12px] text-muted ml-auto">${d}</span>
            </label>`).join('')}
        </div>`,
        footer: `<button class="btn btn-outline" data-close>取消</button>
          <button class="btn btn-primary" data-close onclick="toast('已应用筛选条件','ok')">应用筛选</button>`,
      });
      break;
    case 'search-all-sources':
      S.searchSource = 'all'; renderSidebar(); render(); toast('已切换为全部源', 'info'); break;
    case 'detect-winget': toast('已检测到 winget v1.8.1911', 'ok'); break;
    case 'check-update-app': toast('WingetGUI 已是最新版本 (v0.1.0-prototype)', 'ok'); break;
    case 'set-accent':
      S.settings.accent = el.dataset.color; applyTheme(); render();
      toast('强调色已更新', 'ok', 1500); break;
  }
}

/* ── 启动 ── */
function init() {
  applyTheme();
  renderSidebar();
  hydrateIcons(document);
  bindGlobalSearch();
  bindEvents();
  renderQueue();
  matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => { if (S.theme === 'system') applyTheme(); });
  render();
}
document.addEventListener('DOMContentLoaded', init);
