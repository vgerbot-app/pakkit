/* ══════════════════════════════════════════════════════════════
   UI 基础组件：图标 / 提示 / 模态
   ══════════════════════════════════════════════════════════════ */

const $  = (sel, root = document) => root.querySelector(sel);
const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];

/** 生成 SVG 图标 */
function icon(name, cls = 'w-4 h-4', extra = '') {
  const body = ICONS[name] || '';
  return `<svg viewBox="0 0 24 24" class="${cls}" fill="none" stroke="currentColor"
    stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" ${extra}>${body}</svg>`;
}

/** 挂载所有 [data-icon] 占位 */
function hydrateIcons(root = document) {
  $$('[data-icon]', root).forEach(el => {
    el.innerHTML = icon(el.dataset.icon, el.dataset.iconClass || 'w-4 h-4');
  });
}

/** 轻提示 */
function toast(msg, kind = 'info', ms = 2600) {
  const colors = { info: 'border-line', ok: 'border-ok/50', warn: 'border-warn/50', error: 'border-danger/50' };
  const icoName = { info: 'info', ok: 'check', warn: 'info', error: 'x' }[kind];
  const icoColor = { info: 'text-accent', ok: 'text-ok', warn: 'text-warn', error: 'text-danger' }[kind];
  const el = document.createElement('div');
  el.className = `pointer-events-auto flex items-start gap-2.5 max-w-[360px] px-3.5 py-2.5 rounded-xl
    bg-surface border ${colors[kind]} shadow-pop animate-slideIn`;
  el.innerHTML = `<span class="${icoColor} mt-px shrink-0">${icon(icoName, 'w-4 h-4')}</span>
    <span class="text-[12.5px] leading-relaxed text-ink2">${msg}</span>`;
  $('#toastRoot').appendChild(el);
  setTimeout(() => { el.style.transition = 'opacity .25s, transform .25s'; el.style.opacity = '0'; el.style.transform = 'translateX(16px)'; }, ms - 250);
  setTimeout(() => el.remove(), ms);
}

/** 模态框 */
function modal({ title, subtitle = '', body, footer, size = '' , icon: ico = null }) {
  closeModal();
  const wrap = document.createElement('div');
  wrap.id = 'modalWrap';
  wrap.innerHTML = `
    <div class="backdrop" data-close></div>
    <div class="modal-layer">
    <div class="modal ${size} animate-fadeUp" role="dialog" aria-modal="true">
      <div class="flex items-center gap-3 px-5 py-3.5 border-b border-line shrink-0">
        ${ico ? `<span class="w-8 h-8 rounded-lg bg-surface3 border border-line grid place-items-center text-accent">${icon(ico, 'w-4 h-4')}</span>` : ''}
        <div class="min-w-0">
          <h3 class="text-[14px] font-semibold truncate">${title}</h3>
          ${subtitle ? `<p class="text-[11.5px] text-muted truncate">${subtitle}</p>` : ''}
        </div>
        <button data-close class="icon-btn ml-auto" title="关闭">${icon('x', 'w-4 h-4')}</button>
      </div>
      <div class="flex-1 overflow-y-auto px-5 py-4">${body}</div>
      ${footer ? `<div class="shrink-0 px-5 py-3 border-t border-line flex items-center gap-2 justify-end bg-surface2/60">${footer}</div>` : ''}
    </div>
    </div>`;
  $('#modalRoot').appendChild(wrap);
  $$('[data-close]', wrap).forEach(el => el.addEventListener('click', closeModal));
  document.addEventListener('keydown', escClose);
  return wrap;
}
function closeModal() {
  $('#modalWrap')?.remove();
  document.removeEventListener('keydown', escClose);
}
function escClose(e) { if (e.key === 'Escape') closeModal(); }

/** 确认框 */
function confirmDialog({ title, message, confirmText = '确认', danger = false, onConfirm }) {
  const wrap = modal({
    title, icon: danger ? 'trash' : 'info',
    body: `<p class="text-[13px] leading-relaxed text-ink2">${message}</p>`,
    footer: `<button class="btn btn-outline" data-close>取消</button>
      <button class="btn ${danger ? 'btn-danger' : 'btn-primary'}" id="cfmOk">${confirmText}</button>`,
  });
  $('#cfmOk', wrap).addEventListener('click', () => { closeModal(); onConfirm?.(); });
}

/** 星级 */
function stars(rating) {
  const full = Math.round(rating);
  return `<span class="flex items-center gap-0.5" title="${rating} 分">
    ${Array.from({ length: 5 }, (_, i) =>
      `<span class="${i < full ? 'text-warn' : 'text-line2'}">${icon('star', 'w-3 h-3', i < full ? 'fill="currentColor"' : '')}</span>`
    ).join('')}
    <span class="ml-1 text-[11px] text-muted">${rating.toFixed(1)}</span>
  </span>`;
}

/** 包图标（带底色） */
function pkgIcon(p, size = 'w-11 h-11', icoSize = 'w-5.5 h-5.5') {
  return `<span class="${size} shrink-0 rounded-xl grid place-items-center border border-line"
    style="background:${p.color}1f;color:${p.color}">${icon(p.icon, icoSize)}</span>`;
}
