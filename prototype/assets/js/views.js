/* ══════════════════════════════════════════════════════════════
   视图渲染
   ══════════════════════════════════════════════════════════════ */

const S = {                       // 应用状态
  route: 'home',
  category: null,
  query: '',
  searchSource: 'winget',
  sort: 'relevance',
  filter: 'all',                  // all | installed | notinstalled
  installedOnly: false,
  queue: [],
  theme: 'dark',
  settings: {
    defaultSource: 'winget',
    arch: 'x64',
    scope: 'user',
    acceptAgreements: true,
    silent: true,
    autoUpdate: false,
    installLocation: '',
    concurrency: 1,
    telemetry: false,
    proxy: '',
    theme: 'dark',
    accent: '#4f8cff',
    reduceMotion: false,
    lang: 'zh-CN',
    cliPath: 'C:\\Users\\me\\AppData\\Local\\Microsoft\\WindowsApps\\winget.exe',
  },
};

/* ── 通用片段 ── */

const pageHead = (title, sub, actions = '') => `
  <div class="px-6 pt-5 pb-4 flex items-end gap-4 flex-wrap">
    <div class="min-w-0">
      <h1 class="text-[21px] font-semibold tracking-tight">${title}</h1>
      ${sub ? `<p class="text-[12.5px] text-muted mt-1">${sub}</p>` : ''}
    </div>
    <div class="ml-auto flex items-center gap-2">${actions}</div>
  </div>`;

function pkgRow(p) {
  const inQueue = S.queue.some(q => q.id === p.id && q.state !== 'done');
  const meta = [
    `<span class="whitespace-nowrap">${icon('grid', 'w-3 h-3')}${p.size}</span>`,
    `<span class="whitespace-nowrap">${icon('refresh', 'w-3 h-3')}${p.updated}</span>`,
  ].join('');
  const rating = `<span class="inline-flex items-center gap-1 whitespace-nowrap" title="${p.rating} 分">
      <span class="text-warn">${icon('star', 'w-3 h-3', 'fill="currentColor"')}</span>
      <span class="text-[11.5px] text-ink2">${p.rating.toFixed(1)}</span>
    </span>`;
  return `
  <div class="card card-hover p-4 cursor-pointer group" data-pkg="${p.id}">
    <div class="flex items-start gap-3">
      ${pkgIcon(p)}
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-1.5 flex-wrap">
          <span class="text-[13.5px] font-semibold truncate">${p.name}</span>
          ${p.installed ? `<span class="badge badge-ok">已安装</span>` : ''}
          ${p.source !== 'winget' ? `<span class="badge badge-line">${p.source}</span>` : ''}
        </div>
        <p class="text-[12px] text-muted truncate mt-0.5">${p.publisher} · v${p.version}</p>
        <div class="flex items-center gap-3 mt-2 text-[11px] text-muted overflow-hidden">${meta}${rating}</div>
      </div>
    </div>
    <div class="flex items-center gap-2 mt-3 pt-3 border-t border-line/60">
      <button class="btn ${p.installed ? 'btn-soft' : 'btn-primary'} btn-sm" data-install="${p.id}"
        ${inQueue ? 'disabled' : ''}>
        ${icon(inQueue ? 'clock' : p.installed ? 'refresh' : 'download', 'w-3.5 h-3.5')}
        ${inQueue ? '队列中' : p.installed ? '重新安装' : '安装'}
      </button>
      <span class="badge badge-line">${CATEGORIES.find(c => c.id === p.cat)?.name || '未分类'}</span>
      <span class="ml-auto text-[10.5px] text-muted opacity-0 group-hover:opacity-100 transition whitespace-nowrap hidden sm:inline">查看详情 ›</span>
    </div>
  </div>`;
}

function pkgGrid(list) {
  return `<div class="grid gap-3 [grid-template-columns:repeat(auto-fill,minmax(min(100%,280px),1fr))]">${list.map(pkgRow).join('')}</div>`;
}

/* ══ 首页 ══ */
function viewHome() {
  const featured = PACKAGES.filter(p => p.rating >= 4.7).slice(0, 6);
  const recent = [...PACKAGES].sort((a, b) => b.updated.localeCompare(a.updated)).slice(0, 5);
  return `
  <div class="animate-fadeUp pb-8">
    ${pageHead('发现', '内置分类 + winget 官方源实时检索',
      `<button class="btn btn-soft btn-sm" data-route="updates">${icon('refresh', 'w-3.5 h-3.5')}检查更新</button>
       <button class="btn btn-soft btn-sm" data-action="sync-sources">${icon('source', 'w-3.5 h-3.5')}同步源</button>`)}

    <!-- Hero -->
    <div class="px-6">
      <div class="relative overflow-hidden rounded-xl3 border border-line bg-surface p-6">
        <div class="absolute inset-0 opacity-[.16]"
          style="background:radial-gradient(60% 120% at 92% -20%, var(--accent), transparent 60%), radial-gradient(50% 100% at 4% 120%, var(--accent-2), transparent 60%)"></div>
        <div class="relative flex items-center gap-6 flex-wrap">
          <div class="min-w-0 flex-1">
            <div class="inline-flex items-center gap-1.5 text-[11px] px-2 py-1 rounded-full bg-surface3 border border-line text-ink2 mb-3">
              ${icon('bolt', 'w-3 h-3')} 基于 winget CLI · 无需手动下载安装包
            </div>
            <h2 class="text-[24px] font-semibold tracking-tight leading-snug">在 Windows 上<br class="sm:hidden">优雅地安装软件</h2>
            <p class="text-[13px] text-muted mt-2 max-w-[520px] leading-relaxed">
              分类浏览、跨源搜索、批量安装与源管理。所有操作直接调用系统 winget，
              支持静默安装、范围选择与安装位置自定义。
            </p>
            <div class="flex items-center gap-2 mt-4">
              <button class="btn btn-primary" data-route="installed">${icon('check', 'w-4 h-4')}管理已安装</button>
              <button class="btn btn-outline" data-route="sources">${icon('source', 'w-4 h-4')}管理源</button>
            </div>
          </div>
          <div class="hidden sm:grid grid-cols-3 gap-3 shrink-0">
            ${[['源数量', SOURCES.length, 'source'], ['可用包', '11.8k', 'grid'], ['已安装', PACKAGES.filter(p => p.installed).length, 'check']]
              .map(([k, v, i]) => `
              <div class="w-[92px] px-3 py-3 rounded-xl bg-surface2/80 border border-line text-center">
                <span class="text-accent">${icon(i, 'w-4 h-4 mx-auto')}</span>
                <div class="text-[18px] font-semibold mt-1.5">${v}</div>
                <div class="text-[10.5px] text-muted">${k}</div>
              </div>`).join('')}
          </div>
        </div>
      </div>
    </div>

    <!-- 分类 -->
    <div class="px-6 mt-7">
      <div class="flex items-center mb-3">
        <h3 class="sec-title">常用分类</h3>
        <button class="ml-auto btn-ghost" data-route="categories">查看全部 ${icon('chevron', 'w-3 h-3 inline')}</button>
      </div>
      <div class="grid gap-3" style="grid-template-columns:repeat(auto-fill,minmax(230px,1fr))">
        ${CATEGORIES.map(c => `
          <button class="cat-tile card card-hover p-4 text-left" style="--tile:${c.tile}" data-cat="${c.id}">
            <span class="cat-ico">${icon(c.icon, 'w-5 h-5')}</span>
            <div class="mt-3 relative">
              <div class="text-[13.5px] font-semibold">${c.name}</div>
              <div class="text-[11.5px] text-muted mt-1 leading-relaxed">${c.desc}</div>
              <div class="text-[11px] text-muted mt-2.5 flex items-center gap-1.5">
                ${icon('grid', 'w-3 h-3')} ${c.tags[1]} 个包
              </div>
            </div>
          </button>`).join('')}
      </div>
    </div>

    <!-- 精选 -->
    <div class="px-6 mt-7">
      <div class="flex items-center mb-3">
        <h3 class="sec-title">高分精选</h3>
        <span class="ml-auto text-[11px] text-muted">评分 ≥ 4.7</span>
      </div>
      ${pkgGrid(featured)}
    </div>

    <!-- 最近更新 -->
    <div class="px-6 mt-7">
      <div class="flex items-center mb-3">
        <h3 class="sec-title">最近更新</h3>
        <button class="ml-auto btn-ghost" data-route="search">浏览全部 ${icon('chevron', 'w-3 h-3 inline')}</button>
      </div>
      <div class="card divide-y divide-line overflow-hidden">
        ${recent.map(p => `
          <div class="row-hover px-4 py-2.5 flex items-center gap-3 cursor-pointer" data-pkg="${p.id}">
            ${pkgIcon(p, 'w-8 h-8', 'w-4 h-4')}
            <span class="text-[12.5px] font-medium truncate">${p.name}</span>
            <span class="text-[11.5px] text-muted hidden sm:inline">${p.publisher}</span>
            <span class="ml-auto badgeline badge badge-line">v${p.version}</span>
            <span class="text-[11px] text-muted w-[86px] text-right">${p.updated}</span>
          </div>`).join('')}
      </div>
    </div>
  </div>`;
}

/* ══ 全部分类 ══ */
function viewCategories() {
  return `
  <div class="animate-fadeUp pb-8">
    ${pageHead('分类', `共 ${CATEGORIES.length} 个内置分类，数据来自 winget 源标签映射`)}
    <div class="px-6 grid gap-3" style="grid-template-columns:repeat(auto-fill,minmax(250px,1fr))">
      ${CATEGORIES.map(c => `
        <button class="cat-tile card card-hover p-4 text-left" style="--tile:${c.tile}" data-cat="${c.id}">
          <div class="flex items-start gap-3">
            <span class="cat-ico shrink-0">${icon(c.icon, 'w-5 h-5')}</span>
            <div class="min-w-0 relative">
              <div class="text-[13.5px] font-semibold">${c.name}</div>
              <div class="text-[11.5px] text-muted mt-1 leading-relaxed">${c.desc}</div>
            </div>
          </div>
          <div class="mt-3 pt-3 border-t border-line/70 flex items-center gap-2 relative">
            <span class="badge badge-line">${c.tags[0]}</span>
            <span class="ml-auto text-[11px] text-muted">${c.tags[1]} 个包</span>
          </div>
        </button>`).join('')}
    </div>
  </div>`;
}

/* ══ 分类详情 ══ */
function viewCategory(id) {
  const c = CATEGORIES.find(x => x.id === id) || CATEGORIES[0];
  let list = PACKAGES.filter(p => p.cat === c.id);
  if (!list.length) list = PACKAGES.slice(0, 6);
  const others = CATEGORIES.filter(x => x.id !== c.id).slice(0, 5);
  return `
  <div class="animate-fadeUp pb-8">
    <div class="px-6 pt-5">
      <button class="btn-ghost mb-3 -ml-2" data-route="categories">${icon('back', 'w-3.5 h-3.5 inline')} 返回分类</button>
      <div class="relative overflow-hidden rounded-xl3 border border-line p-5" style="--tile:${c.tile}">
        <div class="absolute inset-0 opacity-[.14]"
          style="background:radial-gradient(70% 130% at 100% -30%, ${c.tile}, transparent 65%)"></div>
        <div class="relative flex items-center gap-4 flex-wrap">
          <span class="cat-ico" style="width:52px;height:52px;border-radius:14px">${icon(c.icon, 'w-6 h-6')}</span>
          <div class="min-w-0">
            <h1 class="text-[20px] font-semibold tracking-tight">${c.name}</h1>
            <p class="text-[12.5px] text-muted mt-1">${c.desc}</p>
          </div>
          <div class="ml-auto flex items-center gap-2">
            <span class="badge badge-line">${c.tags[1]} 个包</span>
            <button class="btn btn-primary btn-sm" data-action="install-category">${icon('download','w-3.5 h-3.5')}安装全部</button>
          </div>
        </div>
      </div>
    </div>

    <div class="px-6 mt-4 flex items-center gap-2 flex-wrap">
      <span class="seg" data-sort>
        <button data-sort="hot" class="${S.sort === 'hot' ? 'on' : ''}">热门</button>
        <button data-sort="rating" class="${S.sort === 'rating' ? 'on' : ''}">评分</button>
        <button data-sort="updated" class="${S.sort === 'updated' ? 'on' : ''}">最新</button>
        <button data-sort="name" class="${S.sort === 'name' ? 'on' : ''}">名称</button>
      </span>
      <span class="text-[11.5px] text-muted ml-1">排序</span>
      <div class="ml-auto flex items-center gap-2">
        ${others.map(o => `<button class="badge badge-line hover:border-line2 cursor-pointer" data-cat="${o.id}">${o.name}</button>`).join('')}
      </div>
    </div>

    <div class="px-6 mt-4">
      ${pkgGrid(sortPkgs(list, S.sort))}
    </div>
  </div>`;
}

function sortPkgs(list, mode) {
  const l = [...list];
  switch (mode) {
    case 'rating':  return l.sort((a, b) => b.rating - a.rating);
    case 'updated': return l.sort((a, b) => b.updated.localeCompare(a.updated));
    case 'name':    return l.sort((a, b) => a.name.localeCompare(b.name));
    default:        return l.sort((a, b) => (b.installed - a.installed) || b.rating - a.rating);
  }
}

/* ══ 搜索 ══ */
function viewSearch() {
  const q = S.query.trim().toLowerCase();
  let list = PACKAGES;
  if (q) list = list.filter(p =>
    [p.name, p.id, p.publisher, p.desc, ...(p.tags || [])].join(' ').toLowerCase().includes(q));
  if (S.filter === 'installed')    list = list.filter(p => p.installed);
  if (S.filter === 'notinstalled') list = list.filter(p => !p.installed);
  list = sortPkgs(list, S.sort === 'relevance' ? 'hot' : S.sort);

  return `
  <div class="animate-fadeUp pb-8">
    ${pageHead(q ? `搜索 “${S.query}”` : '全部软件包',
      `${list.length} 个结果 · 来源 <b class="text-ink2">${S.searchSource}</b>`,
      `<button class="btn btn-soft btn-sm" data-action="advanced-search">${icon('filter','w-3.5 h-3.5')}高级筛选</button>`)}

    <div class="px-6">
      <div class="card p-3 flex items-center gap-2 flex-wrap">
        <span class="seg" data-srccycle>
          ${SOURCES.map(s => `<button data-searchsrc="${s.name}" class="${S.searchSource === s.name ? 'on' : ''}">${s.name}</button>`).join('')}
          <button data-searchsrc="all" class="${S.searchSource === 'all' ? 'on' : ''}">全部源</button>
        </span>
        <span class="w-px h-6 bg-line"></span>
        <span class="seg" data-filter>
          <button data-filter="all" class="${S.filter === 'all' ? 'on' : ''}">全部</button>
          <button data-filter="installed" class="${S.filter === 'installed' ? 'on' : ''}">已安装</button>
          <button data-filter="notinstalled" class="${S.filter === 'notinstalled' ? 'on' : ''}">未安装</button>
        </span>
        <span class="seg ml-auto" data-sort>
          <button data-sort="relevance" class="${S.sort === 'relevance' ? 'on' : ''}">相关度</button>
          <button data-sort="rating" class="${S.sort === 'rating' ? 'on' : ''}">评分</button>
          <button data-sort="updated" class="${S.sort === 'updated' ? 'on' : ''}">最新</button>
        </span>
      </div>
      <p class="text-[11.5px] text-muted mt-2.5 flex items-center gap-1.5">
        ${icon('terminal', 'w-3.5 h-3.5')}
        <code class="font-mono px-1.5 py-0.5 rounded bg-surface2 border border-line">winget search ${q ? `"${S.query}"` : ''} --source ${S.searchSource === 'all' ? '(全部)' : S.searchSource}</code>
      </p>
    </div>

    <div class="px-6 mt-4">
      ${list.length ? pkgGrid(list) : `
        <div class="card p-12 text-center">
          <span class="w-14 h-14 rounded-2xl bg-surface3 border border-line grid place-items-center mx-auto text-muted">${icon('search','w-6 h-6')}</span>
          <p class="text-[14px] font-medium mt-4">未找到匹配的软件包</p>
          <p class="text-[12.5px] text-muted mt-1.5">尝试更换关键词，或切换到“全部源”再搜索一次</p>
          <div class="flex items-center gap-2 justify-center mt-5">
            <button class="btn btn-soft btn-sm" data-action="search-all-sources">在全部源中搜索</button>
            <button class="btn btn-outline btn-sm" data-route="sources">管理源</button>
          </div>
        </div>`}
    </div>
  </div>`;
}

/* ══ 已安装 / 更新 ══ */
function viewInstalled() {
  const list = PACKAGES.filter(p => p.installed);
  return `
  <div class="animate-fadeUp pb-8">
    ${pageHead('已安装', `${list.length} 个软件通过 winget 管理`,
      `<button class="btn btn-soft btn-sm" data-action="export">${icon('file','w-3.5 h-3.5')}导出清单</button>
       <button class="btn btn-soft btn-sm" data-action="upgrade-all">${icon('refresh','w-3.5 h-3.5')}全部升级</button>`)}
    <div class="px-6">
      <div class="card overflow-hidden">
        <div class="grid grid-cols-[1fr_120px_100px_150px] gap-3 px-4 py-2.5 border-b border-line text-[11px] sec-title bg-surface2/60">
          <span>软件包</span><span>版本</span><span class="text-right">大小</span><span class="text-right">操作</span>
        </div>
        ${list.map(p => `
          <div class="row-hover grid grid-cols-[1fr_120px_100px_150px] gap-3 px-4 py-2.5 items-center border-b border-line/60 last:border-0">
            <div class="flex items-center gap-3 min-w-0 cursor-pointer" data-pkg="${p.id}">
              ${pkgIcon(p, 'w-9 h-9', 'w-4 h-4')}
              <div class="min-w-0">
                <div class="text-[12.5px] font-medium truncate">${p.name}</div>
                <div class="text-[11px] text-muted truncate font-mono">${p.id}</div>
              </div>
            </div>
            <code class="text-[11.5px] text-ink2 font-mono">${p.version}</code>
            <span class="text-[11.5px] text-muted text-right">${p.size}</span>
            <div class="flex items-center gap-1.5 justify-end">
              <button class="btn-sm btn btn-soft" data-install="${p.id}">${icon('download','w-3 h-3')}升级</button>
              <button class="icon-btn" data-uninstall="${p.id}" title="卸载">${icon('trash','w-4 h-4')}</button>
            </div>
          </div>`).join('')}
      </div>
      <p class="text-[11.5px] text-muted mt-3 flex items-center gap-1.5">${icon('info','w-3.5 h-3.5')} 卸载操作将调用 <code class="font-mono">winget uninstall</code>，可能需要管理员权限</p>
    </div>
  </div>`;
}

function viewUpdates() {
  const updates = PACKAGES.filter(p => p.installed).slice(0, 4)
    .map((p, i) => ({ ...p, newVersion: `${p.version.split('.').slice(0, -1).join('.')}.${(+p.version.split('.').pop() || 0) + 1}`, delta: ['12.4 MB','3.1 MB','58.2 MB','1.2 MB'][i] }));
  return `
  <div class="animate-fadeUp pb-8">
    ${pageHead('更新', `${updates.length} 个可用更新`,
      `<button class="btn btn-primary btn-sm" data-action="upgrade-all">${icon('download','w-3.5 h-3.5')}全部更新</button>`)}
    <div class="px-6 space-y-2">
      ${updates.map(p => `
        <div class="card px-4 py-3 flex items-center gap-3.5">
          ${pkgIcon(p)}
          <div class="min-w-0 flex-1">
            <div class="text-[13.5px] font-semibold truncate">${p.name}</div>
            <div class="flex items-center gap-2 mt-1 text-[11.5px]">
              <code class="font-mono text-muted line-through">${p.version}</code>
              ${icon('chevron','w-3 h-3 text-muted')}
              <code class="font-mono text-ok">${p.newVersion}</code>
              <span class="badge badge-accent">+${p.delta}</span>
            </div>
          </div>
          <button class="btn btn-primary btn-sm" data-install="${p.id}">${icon('download','w-3.5 h-3.5')}更新</button>
        </div>`).join('')}
    </div>
  </div>`;
}

/* ══ 软件包详情（抽屉） ══ */
function openPkgDetail(id) {
  const p = PACKAGES.find(x => x.id === id);
  if (!p) return;
  const related = PACKAGES.filter(x => x.cat === p.cat && x.id !== p.id).slice(0, 3);
  const cat = CATEGORIES.find(c => c.id === p.cat);
  modal({
    title: p.name, subtitle: `${p.id} · ${cat?.name || ''}`, size: 'modal-lg', icon: p.icon,
    body: `
      <div class="flex items-start gap-4 flex-wrap">
        ${pkgIcon(p, 'w-16 h-16', 'w-8 h-8')}
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-2 flex-wrap">
            ${p.installed ? `<span class="badge badge-ok">已安装</span>` : `<span class="badge badge-line">未安装</span>`}
            <span class="badge badge-accent">${p.source}</span>
            ${(p.tags || []).map(t => `<span class="badge badge-line">${t}</span>`).join('')}
          </div>
          <p class="text-[12.5px] text-ink2 leading-relaxed mt-2.5">${p.desc}</p>
          <div class="mt-3">${stars(p.rating)}</div>
        </div>
      </div>

      <div class="grid grid-cols-2 sm:grid-cols-4 gap-2.5 mt-5">
        ${[['版本', p.version], ['发布者', p.publisher], ['体积', p.size], ['许可', p.license],
           ['更新日期', p.updated], ['累计安装', p.installs], ['来源', p.source], ['分类', cat?.name || '—']]
          .map(([k, v]) => `
          <div class="px-3 py-2.5 rounded-xl bg-surface2 border border-line">
            <div class="text-[10.5px] text-muted">${k}</div>
            <div class="text-[12.5px] font-medium mt-0.5 truncate" title="${v}">${v}</div>
          </div>`).join('')}
      </div>

      <div class="mt-5">
        <h4 class="sec-title mb-2">等价命令</h4>
        <div class="flex items-center gap-2 px-3 py-2.5 rounded-xl bg-surface2 border border-line font-mono text-[12px]">
          <span class="text-accent">$</span>
          <span class="truncate">winget install --id ${p.id} --source ${p.source} --accept-package-agreements</span>
          <button class="icon-btn ml-auto" data-copy="winget install --id ${p.id} --source ${p.source}" title="复制">${icon('file','w-4 h-4')}</button>
        </div>
      </div>

      ${related.length ? `
      <div class="mt-5">
        <h4 class="sec-title mb-2">同类推荐</h4>
        <div class="space-y-2">
          ${related.map(r => `
            <div class="row-hover flex items-center gap-3 px-3 py-2 rounded-xl border border-line cursor-pointer" data-pkg="${r.id}">
              ${pkgIcon(r, 'w-8 h-8', 'w-4 h-4')}
              <span class="text-[12.5px] font-medium truncate">${r.name}</span>
              <span class="ml-auto text-[11px] text-muted">${r.publisher}</span>
            </div>`).join('')}
        </div>
      </div>` : ''}
    `,
    footer: `
      <button class="btn btn-outline" data-copy="winget install --id ${p.id}">${icon('file','w-4 h-4')}复制命令</button>
      ${p.installed ? `<button class="btn btn-danger" data-uninstall="${p.id}">${icon('trash','w-4 h-4')}卸载</button>` : ''}
      <button class="btn btn-primary" data-install="${p.id}">${icon('download','w-4 h-4')}${p.installed ? '重新安装' : '安装'}</button>
    `,
  });
}

/* ══ 源管理 ══ */
function viewSources() {
  return `
  <div class="animate-fadeUp pb-8">
    ${pageHead('源管理', 'winget 从多个源检索软件包，可添加企业内部源',
      `<button class="btn btn-soft btn-sm" data-action="sync-sources">${icon('refresh','w-3.5 h-3.5')}全部更新</button>
       <button class="btn btn-primary btn-sm" data-action="add-source">${icon('plus','w-3.5 h-3.5')}添加源</button>`)}

    <div class="px-6 space-y-3">
      ${SOURCES.map(s => `
        <div class="card p-4">
          <div class="flex items-start gap-4 flex-wrap">
            <span class="w-11 h-11 shrink-0 rounded-xl grid place-items-center border border-line bg-surface3 text-accent">${icon('source','w-5 h-5')}</span>
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2 flex-wrap">
                <span class="text-[14px] font-semibold">${s.name}</span>
                <span class="badge ${s.trusted ? 'badge-ok' : 'badge-warn'}">${s.trusted ? '受信任' : '未验证'}</span>
                ${s.name === S.settings.defaultSource ? `<span class="badge badge-accent">默认</span>` : ''}
              </div>
              <div class="text-[11.5px] text-muted mt-1 font-mono truncate">${s.url}</div>
              <div class="flex items-center gap-3 mt-2 text-[11px] text-muted flex-wrap">
                <span class="flex items-center gap-1">${icon('grid','w-3 h-3')} ${s.packages.toLocaleString()} 个包</span>
                <span class="flex items-center gap-1">${icon('clock','w-3 h-3')} 同步于 ${s.updated}</span>
                <span class="flex items-center gap-1">${icon('shield','w-3 h-3')} ${s.type}</span>
              </div>
            </div>
            <div class="flex items-center gap-1.5">
              <button class="btn btn-soft btn-sm" data-action="sync-source" data-name="${s.name}">${icon('refresh','w-3 h-3')}更新</button>
              ${s.name !== S.settings.defaultSource ? `<button class="btn btn-outline btn-sm" data-action="set-default-src" data-name="${s.name}">设为默认</button>` : ''}
              <button class="icon-btn" data-action="edit-source" data-name="${s.name}" title="编辑">${icon('settings','w-4 h-4')}</button>
              <button class="icon-btn hover:!text-danger" data-action="remove-source" data-name="${s.name}" title="移除">${icon('trash','w-4 h-4')}</button>
            </div>
          </div>
        </div>`).join('')}

      <div class="card p-4 border-dashed">
        <div class="flex items-center gap-3 text-[12.5px] text-muted">
          ${icon('info','w-4 h-4 text-accent')}
          添加源需提供名称、URL 与类型，等价于 <code class="font-mono px-1.5 py-0.5 rounded bg-surface2 border border-line">winget source add --name &lt;n&gt; --arg &lt;url&gt;</code>
        </div>
      </div>
    </div>
  </div>`;
}

/* ══ 设置 ══ */
function viewSettings() {
  const st = S.settings;
  const section = (title, desc, body) => `
    <section class="card p-5">
      <h3 class="text-[14px] font-semibold">${title}</h3>
      ${desc ? `<p class="text-[12px] text-muted mt-1">${desc}</p>` : ''}
      <div class="mt-4 space-y-1">${body}</div>
    </section>`;

  const row = (label, hint, ctrl) => `
    <div class="flex items-center gap-4 py-2.5 border-b border-line/60 last:border-0">
      <div class="min-w-0 flex-1">
        <div class="text-[13px] font-medium">${label}</div>
        ${hint ? `<div class="text-[11.5px] text-muted mt-0.5">${hint}</div>` : ''}
      </div>
      <div class="shrink-0">${ctrl}</div>
    </div>`;

  const sw = (key, on) => `<button class="switch ${on ? 'on' : ''}" data-toggle="${key}" role="switch" aria-checked="${on}"></button>`;

  return `
  <div class="animate-fadeUp pb-8">
    ${pageHead('设置', '所有选项将映射为 winget 命令行参数')}
    <div class="px-6 grid gap-4 xl:grid-cols-2 items-start">

      ${section('安装默认值', '新安装任务默认使用的参数',
        row('默认来源', '搜索与安装优先使用的源',
          `<select class="field w-[190px]" data-set="defaultSource">
            ${SOURCES.map(s => `<option ${st.defaultSource === s.name ? 'selected' : ''}>${s.name}</option>`).join('')}
          </select>`) +
        row('系统架构', '传递给 --architecture', `
          <span class="seg" data-setseg="arch">
            ${['x64','x86','arm64'].map(a => `<button data-val="${a}" class="${st.arch === a ? 'on' : ''}">${a}</button>`).join('')}
          </span>`) +
        row('安装范围', '--scope', `
          <span class="seg" data-setseg="scope">
            <button data-val="user" class="${st.scope === 'user' ? 'on' : ''}">当前用户</button>
            <button data-val="machine" class="${st.scope === 'machine' ? 'on' : ''}">全局(需提权)</button>
          </span>`) +
        row('自定义安装位置', '--location，留空则使用默认路径',
          `<input class="field w-[280px]" placeholder="D:\\Software" value="${st.installLocation}" data-set="installLocation" />`) +
        row('自动接受协议', '--accept-package-agreements --accept-source-agreements', sw('acceptAgreements', st.acceptAgreements)) +
        row('静默安装', '--silent', sw('silent', st.silent))
      )}

      ${section('更新与队列', '安装任务的调度方式',
        row('自动检查更新', '启动时检查已安装软件的新版本', sw('autoUpdate', st.autoUpdate)) +
        row('并发任务数', '建议 1（winget 存在安装锁）', `
          <span class="seg" data-setseg="concurrency">
            ${[1,2,3].map(n => `<button data-val="${n}" class="${st.concurrency === n ? 'on' : ''}">${n}</button>`).join('')}
          </span>`)
      )}

      ${section('外观', '界面主题与强调色',
        row('主题', '深色 / 浅色 / 跟随系统', `
          <span class="seg" data-setseg="theme">
            ${[['dark','深色'],['light','浅色'],['system','跟随系统']].map(([v,l]) =>
              `<button data-val="${v}" class="${st.theme === v ? 'on' : ''}">${l}</button>`).join('')}
          </span>`) +
        row('强调色', '主色调', `
          <div class="flex items-center gap-2">
            ${['#4f8cff','#a06bff','#16a34a','#e11d48','#d97706'].map(c => `
              <button class="w-7 h-7 rounded-lg border-2 transition ${st.accent === c ? 'border-ink scale-110' : 'border-transparent'}"
                style="background:${c}" data-action="set-accent" data-color="${c}"></button>`).join('')}
          </div>`) +
        row('减少动画', '禁用过渡与位移效果', sw('reduceMotion', st.reduceMotion))
      )}

      ${section('系统与高级', 'winget 可执行文件与网络',
        row('winget 路径', '通常无需修改，自动探测', `
          <div class="flex items-center gap-2">
            <input class="field w-[300px] font-mono text-[11.5px]" value="${st.cliPath}" data-set="cliPath" />
            <button class="btn btn-soft btn-sm" data-action="detect-winget">${icon('refresh','w-3 h-3')}检测</button>
          </div>`) +
        row('代理服务器', '留空则使用系统代理', `<input class="field w-[280px]" placeholder="http://127.0.0.1:7890" value="${st.proxy}" data-set="proxy" />`) +
        row('界面语言', '', `
          <select class="field w-[190px]" data-set="lang">
            <option value="zh-CN" ${st.lang === 'zh-CN' ? 'selected' : ''}>简体中文</option>
            <option value="en-US" ${st.lang === 'en-US' ? 'selected' : ''}>English</option>
          </select>`) +
        row('使用统计', '匿名上报崩溃与功能使用情况（不收集包名）', sw('telemetry', st.telemetry))
      )}

      ${section('关于', '',
        `<div class="flex items-center gap-4 py-3">
          <span class="w-12 h-12 rounded-xl bg-gradient-to-br from-accent to-accent2 grid place-items-center text-onaccent">${icon('download','w-6 h-6')}</span>
          <div>
            <div class="text-[14px] font-semibold">WingetGUI <span class="text-muted text-[12px] font-normal">v0.1.0-prototype</span></div>
            <div class="text-[11.5px] text-muted mt-0.5">Rust + GPUI · MIT License</div>
          </div>
          <div class="ml-auto flex gap-2">
            <button class="btn btn-outline btn-sm" data-action="check-update-app">${icon('refresh','w-3 h-3')}检查更新</button>
            <button class="btn btn-outline btn-sm">${icon('external','w-3 h-3')}项目主页</button>
          </div>
        </div>
        <div class="flex items-center gap-3 py-2.5 border-t border-line/60 text-[12px]">
          <span class="text-muted">当前 winget 版本</span>
          <code class="ml-auto font-mono px-2 py-1 rounded bg-surface2 border border-line">v1.8.1911</code>
        </div>`)}
    </div>
  </div>`;
}

/* ══ 安装队列（侧栏面板） ══ */
function renderQueue() {
  const el = $('#queueList');
  const badge = $('#queueBadge');
  const active = S.queue.filter(q => q.state === 'running' || q.state === 'pending').length;
  badge.textContent = S.queue.length;
  badge.classList.toggle('hidden', S.queue.length === 0);
  badge.classList.toggle('!bg-muted', active === 0);
  $('#queueCount').textContent = S.queue.length;
  $('#statusQueue').textContent = S.queue.length;

  if (!S.queue.length) {
    el.innerHTML = `<div class="h-full grid place-items-center text-center px-6">
      <div>
        <span class="w-12 h-12 rounded-2xl bg-surface3 border border-line grid place-items-center mx-auto text-muted">${icon('queue','w-5 h-5')}</span>
        <p class="text-[12.5px] text-muted mt-3">队列为空</p>
        <p class="text-[11px] text-muted/70 mt-1">在软件列表点击“安装”加入队列</p>
      </div></div>`;
    return;
  }

  const stateMap = {
    pending: ['待安装', 'badge-line',   'text-muted'],
    running: ['安装中', 'badge-accent', 'text-accent'],
    done:    ['已完成', 'badge-ok',     'text-ok'],
    error:   ['失败',   'badge-danger', 'text-danger'],
  };

  el.innerHTML = S.queue.map(q => {
    const [label, badgeCls, icoCls] = stateMap[q.state];
    return `
    <div class="card p-3 animate-fadeUp">
      <div class="flex items-center gap-2.5">
        ${pkgIcon(q, 'w-9 h-9', 'w-4 h-4')}
        <div class="min-w-0 flex-1">
          <div class="text-[12.5px] font-semibold truncate">${q.name}</div>
          <div class="text-[11px] text-muted font-mono truncate">${q.id}</div>
        </div>
        <span class="badge ${badgeCls}">${label}</span>
        ${q.state === 'done' || q.state === 'error'
          ? `<button class="icon-btn" data-queue-remove="${q.uid}" title="移除">${icon('x','w-4 h-4')}</button>`
          : `<button class="icon-btn" data-queue-cancel="${q.uid}" title="取消">${icon('x','w-4 h-4')}</button>`}
      </div>
      ${q.state === 'running' || q.state === 'pending' ? `
        <div class="mt-2.5 bar ${q.state === 'pending' ? '' : 'bar-indeterminate'}"><i style="width:${q.progress || 0}%"></i></div>
        <div class="flex items-center gap-2 mt-1.5 text-[10.5px] text-muted">
          <span class="${icoCls}">${icon(q.state === 'pending' ? 'clock' : 'download','w-3 h-3')}</span>
          <span class="truncate">${q.log || '等待中…'}</span>
          ${q.state === 'running' ? `<span class="ml-auto">${q.progress || 0}%</span>` : ''}
        </div>` : ''}
      ${q.state === 'error' ? `<p class="text-[11px] text-danger mt-2 leading-relaxed">${q.log}</p>` : ''}
      ${q.state === 'done' ? `<p class="text-[11px] text-muted mt-2 flex items-center gap-1">${icon('check','w-3 h-3 text-ok')} ${q.log}</p>` : ''}
    </div>`;
  }).join('');
}
