import { Preview, renderHtmlFrames } from './preview.js';

const T = window.__TAURI__;
const { invoke, convertFileSrc } = T.core;
const { listen } = T.event;
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
const esc = (s) => String(s ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

const I = {
  undo: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 14L4 9l5-5"/><path d="M4 9h10a6 6 0 010 12h-3"/></svg>',
  redo: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 14l5-5-5-5"/><path d="M20 9H10a6 6 0 000 12h3"/></svg>',
  play: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M7 4.5v15l13-7.5z"/></svg>',
  pause: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M6 4h4v16H6zM14 4h4v16h-4z"/></svg>',
  start: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M6 5h2v14H6zM20 5v14L9 12z"/></svg>',
  prev: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M15 5v14L6 12z"/></svg>',
  next: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M9 5v14l9-7z"/></svg>',
};

let S = null; // latest snapshot from Rust
let preview;
const ui = { t: 0, playing: false, sel: null, pps: 60, snap: true, presets: [], renderModal: false, base: 0, base0: 0 };
const waves = new Map();

const P = () => S.project;
const media = (id) => P().media.find((m) => m.id === id);
const fmt = (t) => {
  t = Math.max(0, t);
  const m = Math.floor(t / 60), s = t - m * 60;
  return String(m).padStart(2, '0') + ':' + s.toFixed(2).padStart(5, '0');
};
const frameDur = () => 1 / (S ? P().settings.fps : 30);

async function op(name, args = {}) {
  try {
    return await invoke('op', { name, args });
  } catch (e) {
    toast(String(e));
    return undefined;
  }
}
function toast(msg) {
  const d = document.createElement('div');
  d.className = 'toast';
  d.textContent = msg;
  $('#toasts').appendChild(d);
  setTimeout(() => d.remove(), 3500);
}
function findClip(id) {
  for (const t of P().tracks) {
    const c = t.clips.find((c) => c.id === id);
    if (c) return { c, t };
  }
  return null;
}

// ---------------- modal helpers ----------------
function modal(html, wide) {
  const m = $('#modal');
  m.innerHTML = `<div class="dlg" ${wide ? 'style="width:min(760px,94vw)"' : ''}>${html}</div>`;
  m.hidden = false;
  return m;
}
function closeModal() {
  $('#modal').hidden = true;
  $('#modal').innerHTML = '';
}
function ask(title, value = '') {
  return new Promise((res) => {
    const m = modal(`<h3>${esc(title)}</h3><input type="text" id="askIn" value="${esc(value)}"><div class="row gap" style="margin-top:14px;justify-content:flex-end"><button class="soft" id="askNo">Cancel</button><button class="primary" id="askOk">OK</button></div>`);
    const inp = $('#askIn', m);
    inp.focus();
    inp.select();
    const done = (v) => { closeModal(); res(v); };
    $('#askOk', m).onclick = () => done(inp.value);
    $('#askNo', m).onclick = () => done(null);
    inp.onkeydown = (e) => { if (e.key === 'Enter') done(inp.value); if (e.key === 'Escape') done(null); };
  });
}
function confirmBox(title, text, okLabel = 'Delete') {
  return new Promise((res) => {
    const m = modal(`<h3>${esc(title)}</h3><p class="muted">${esc(text)}</p><div class="row gap" style="margin-top:14px;justify-content:flex-end"><button class="soft" id="cNo">Cancel</button><button class="primary" id="cOk">${esc(okLabel)}</button></div>`);
    $('#cOk', m).onclick = () => { closeModal(); res(true); };
    $('#cNo', m).onclick = () => { closeModal(); res(false); };
  });
}
function menu(x, y, items) {
  $$('.menu').forEach((m) => m.remove());
  const m = document.createElement('div');
  m.className = 'menu';
  items.forEach(([label, fn, cls]) => {
    const b = document.createElement('button');
    b.textContent = label;
    if (cls) b.className = cls;
    b.onclick = () => { m.remove(); fn(); };
    m.appendChild(b);
  });
  document.body.appendChild(m);
  m.style.left = Math.min(x, innerWidth - m.offsetWidth - 8) + 'px';
  m.style.top = Math.min(y, innerHeight - m.offsetHeight - 8) + 'px';
  setTimeout(() => addEventListener('pointerdown', function h(e) { if (!m.contains(e.target)) { m.remove(); removeEventListener('pointerdown', h, true); } }, true), 0);
}

// ---------------- state ----------------
function ago(sec) {
  const d = Math.max(0, Date.now() / 1000 - sec);
  if (d < 60) return 'just now';
  if (d < 3600) return Math.floor(d / 60) + ' min ago';
  if (d < 86400) return Math.floor(d / 3600) + ' h ago';
  if (d < 86400 * 30) return Math.floor(d / 86400) + ' days ago';
  return new Date(sec * 1000).toLocaleDateString();
}
async function newProject() {
  const n = await ask('Name your project', 'Untitled');
  if (n === null) return;
  ui.sel = null;
  pause();
  await op('new_project', { name: n.trim() || 'Untitled' });
  setTime(0);
}
async function openPath(path) {
  ui.sel = null;
  pause();
  const r = await op('open_project', { path });
  if (r) setTime(0);
}
async function openDialog() {
  const p = await T.dialog.open({ filters: [{ name: 'Cutlight project', extensions: ['cutlight'] }] });
  if (p) openPath(p);
}
async function renderHome() {
  const list = (await op('list_projects')) || [];
  const grid = $('#homeGrid');
  grid.innerHTML = '';
  const nw = document.createElement('div');
  nw.className = 'pcard new';
  nw.innerHTML = '<div class="pv">+</div><div class="pm"><div class="pn">New project</div><div class="ps">Start from an empty timeline</div></div>';
  nw.onclick = newProject;
  grid.appendChild(nw);
  list.forEach((e) => {
    const c = document.createElement('div');
    c.className = 'pcard';
    const bg = e.thumb ? `background-image:url('${convertFileSrc(e.thumb)}')` : '';
    const ratio = e.width && e.height ? `${e.width}×${e.height}` : '';
    c.innerHTML = `<div class="pv" style="${bg}">${e.thumb ? '' : esc((e.name || '?').slice(0, 1).toUpperCase())}${e.duration ? `<span class="tag">${dur(e.duration)}</span>` : ''}</div>
      <button class="px" title="Remove from list">✕</button>
      <div class="pm"><div class="pn">${esc(e.name)}</div><div class="ps">${ago(e.updated)} · ${e.clips} clip${e.clips === 1 ? '' : 's'} · ${ratio}</div></div>`;
    c.onclick = () => openPath(e.path);
    $('.px', c).onclick = async (ev) => {
      ev.stopPropagation();
      if (await confirmBox('Remove project?', `“${e.name}” will be removed from this list${e.path.includes('projects') ? ' and its project file deleted (your media files are not touched)' : ''}.`, 'Remove')) {
        await op('remove_recent', { path: e.path, deleteFile: true });
        renderHome();
      }
    };
    grid.appendChild(c);
  });
}
async function connectClaude(btn) {
  const msg = $('#connectMsg');
  if (btn) btn.disabled = true;
  msg.textContent = 'Connecting…';
  try {
    const r = await invoke('connect_claude');
    toast('Connected. Restart your Claude Code session to see the Cutlight tools.');
    msg.textContent = r.output || 'Connected.';
  } catch (e) {
    toast(String(e).slice(0, 200));
    msg.textContent = String(e);
  } finally {
    if (btn) btn.disabled = false;
  }
}

let homeShown = false;
function onState() {
  const home = $('#home');
  if (!S.open) {
    if (ui.playing) pause();
    home.hidden = false;
    renderMcp();
    renderHome(); // also refreshes previews as they finish generating
    homeShown = true;
    return;
  }
  if (!home.hidden) { home.hidden = true; homeShown = false; }
  const p = P();
  if (document.activeElement !== $('#projName')) $('#projName').value = p.name;
  $('#bUndo').disabled = !S.canUndo;
  $('#bRedo').disabled = !S.canRedo;
  $('#saveState').textContent = 'Saved automatically';
  if (document.activeElement !== $('#script')) $('#script').value = p.script;
  if (ui.sel && !findClip(ui.sel)) ui.sel = null;
  preview.setProject(p);
  document.documentElement.style.setProperty('--ar', p.settings.width / p.settings.height);
  renderMedia();
  renderTranscript();
  renderInspector();
  renderExport();
  renderTimeline();
  renderMcp();
  renderRenderModal();
  $('#tcTotal').textContent = '/ ' + fmt(S.duration);
  preview.update(ui.t, ui.playing);
}

function renderMcp() {
  const port = S.mcpPort;
  ['#mcpDot', '#mcpDotTop', '#mcpDotHome'].forEach((s) => $(s).classList.toggle('ok', !!port));
  const url = `http://127.0.0.1:${port || 47321}/mcp`;
  $('#mcpCmd').textContent = `claude mcp add --transport http cutlight ${url}`;
  $('#mcpJson').textContent = JSON.stringify({ mcpServers: { cutlight: { type: 'http', url } } }, null, 2);
}

// ---------------- media pool ----------------
function dur(s) { return s ? fmt(s).replace(/^00:/, '0:') : ''; }
function renderMedia() {
  const box = $('#mediaList');
  const list = P().media;
  $('#dropHint').style.display = list.length ? 'none' : '';
  box.innerHTML = '';
  box.style.cssText = 'display:flex;flex-direction:column;gap:8px';
  list.forEach((m) => {
    const d = document.createElement('div');
    d.className = 'media';
    d.draggable = true;
    const th = m.strip ? `background-image:url('${convertFileSrc(m.strip)}');background-size:auto 100%;background-position:left` : '';
    const glyph = m.kind === 'audio' ? '♪' : m.status === 'error' ? '!' : '';
    d.innerHTML = `<div class="th" style="${th}">${m.strip ? '' : glyph}</div>
      <div class="meta"><div class="nm" title="${esc(m.path)}">${esc(m.name)}</div>
      <div class="sub">${m.status === 'working' ? '<span class="spin"></span> preparing…' : m.status === 'error' ? esc(m.error || 'error') : `${dur(m.duration)}${m.width ? ` · ${m.width}×${m.height}` : ''}`}</div></div>
      <button class="soft small add" title="Add to timeline">+</button>`;
    d.ondragstart = (e) => { e.dataTransfer.setData('application/x-cutlight', m.id); e.dataTransfer.effectAllowed = 'copy'; };
    d.ondblclick = () => op('add_clip', { mediaId: m.id });
    $('.add', d).onclick = (e) => {
      e.stopPropagation();
      const r = e.target.getBoundingClientRect();
      const items = [['Add to timeline', () => op('add_clip', { mediaId: m.id })]];
      if (m.hasVideo && m.hasAudio) items.push(['Add video only (no sound)', () => op('add_clip', { mediaId: m.id, mode: 'video' })], ['Add audio only', () => op('add_clip', { mediaId: m.id, mode: 'audio' })]);
      items.push(['Remove from project', async () => { if (await confirmBox('Remove media?', `“${m.name}” and every clip using it will be removed.`, 'Remove')) op('remove_media', { id: m.id }); }, 'danger']);
      menu(r.left, r.bottom, items);
    };
    box.appendChild(d);
  });
  const sel = $('#trMedia');
  const cur = sel.value;
  sel.innerHTML = list.filter((m) => m.hasAudio).map((m) => `<option value="${m.id}">${esc(m.name)}</option>`).join('');
  if (cur) sel.value = cur;
}

async function importPaths(paths, addToTimeline) {
  for (const path of paths) {
    const r = await op('import_media', { path });
    if (r && addToTimeline) await op('add_clip', { mediaId: r.mediaId });
  }
}
async function pickImport() {
  const sel = await T.dialog.open({
    multiple: true,
    filters: [{ name: 'Media', extensions: ['mp4', 'mov', 'mkv', 'webm', 'avi', 'm4v', 'wmv', 'flv', 'mpg', 'mpeg', 'ts', 'mts', '3gp', 'mp3', 'wav', 'm4a', 'aac', 'flac', 'ogg', 'opus', 'wma', 'aiff', 'png', 'jpg', 'jpeg', 'webp', 'gif', 'bmp'] }],
  });
  if (sel) await importPaths(Array.isArray(sel) ? sel : [sel], false);
}

// ---------------- script / transcript ----------------
function renderTranscript() {
  const box = $('#transcript');
  const segs = P().transcript;
  if (!segs.length) { box.innerHTML = '<div class="muted small">No transcript yet. Transcribe a clip with Whisper, or ask Claude to.</div>'; return; }
  box.innerHTML = '';
  segs.forEach((s) => {
    const d = document.createElement('div');
    d.className = 'seg';
    d.innerHTML = `<span class="t">${fmt(s.start)}</span><div contenteditable="plaintext-only" spellcheck="false">${esc(s.text)}</div>`;
    $('.t', d).onclick = () => setTime(s.start);
    const ed = $('div', d);
    ed.onblur = () => { if (ed.textContent !== s.text) op('update_segment', { id: s.id, text: ed.textContent }); };
    box.appendChild(d);
  });
}

// ---------------- inspector ----------------
function slider(label, key, c, min, max, step, show) {
  const v = c[key];
  return `<div class="field"><label>${label}</label><input type="range" data-k="${key}" min="${min}" max="${max}" step="${step}" value="${v}"><span class="v">${show ? show(v) : v}</span></div>`;
}
function renderInspector() {
  const box = $('#pane-clip');
  const f = ui.sel && findClip(ui.sel);
  if (!f) {
    const p = P();
    const n = p.tracks.reduce((a, t) => a + t.clips.length, 0);
    box.innerHTML = `<div class="empty">Select a clip to edit it</div><div class="muted small center">${n} clips · ${fmt(S.duration)}<br>Space play · S split · Del delete</div>`;
    return;
  }
  const { c, t } = f;
  const m = c.mediaId && media(c.mediaId);
  const idx = t.clips.indexOf(c);
  const visual = c.kind !== 'audio';
  const hasAudio = c.kind === 'audio' || (c.kind === 'video' && m && m.hasAudio);
  let h = `<h4>${esc(c.label || c.kind)}</h4><div class="muted small">${c.kind} · ${fmt(c.start)} → ${fmt(c.start + c.duration)}</div>`;
  h += `<div class="card">`;
  h += slider('Speed', 'speed', c, 0.25, 4, 0.05, (v) => v.toFixed(2) + '×');
  if (hasAudio) h += slider('Volume', 'volume', c, 0, 2, 0.05, (v) => Math.round(v * 100) + '%');
  h += slider('Fade in', 'fadeIn', c, 0, 3, 0.1, (v) => v.toFixed(1) + 's');
  h += slider('Fade out', 'fadeOut', c, 0, 3, 0.1, (v) => v.toFixed(1) + 's');
  h += `</div>`;
  if (visual && c.kind !== 'html') {
    h += `<div class="card"><div class="field"><label>Framing</label><div class="seg-btns" data-fit>${['fit', 'fill'].map((x) => `<button data-v="${x}" class="${c.fit === x ? 'on' : ''}">${x === 'fit' ? 'Fit' : 'Fill'}</button>`).join('')}</div><span></span></div>`;
    h += slider('Opacity', 'opacity', c, 0, 1, 0.05, (v) => Math.round(v * 100) + '%');
    h += slider('Scale', 'scale', c, 0.1, 2, 0.05, (v) => Math.round(v * 100) + '%');
    h += slider('Position X', 'x', c, 0, 1, 0.01, (v) => Math.round(v * 100) + '%');
    h += slider('Position Y', 'y', c, 0, 1, 0.01, (v) => Math.round(v * 100) + '%');
    h += `</div>`;
  }
  if (t.kind === 'video' && idx > 0) {
    const tr = c.transition;
    h += `<div class="card"><div class="field"><label>Transition</label><select id="trKind">${[['none', 'None'], ['dissolve', 'Dissolve'], ['fade-black', 'Dip to black'], ['slide-left', 'Slide ←'], ['slide-right', 'Slide →'], ['slide-up', 'Slide ↑'], ['slide-down', 'Slide ↓']].map(([k, l]) => `<option value="${k}" ${(tr ? tr.kind : 'none') === k ? 'selected' : ''}>${l}</option>`).join('')}</select><span></span></div>`;
    if (tr) h += `<div class="field"><label>Length</label><input type="range" id="trDur" min="0.1" max="2" step="0.05" value="${tr.duration}"><span class="v">${tr.duration.toFixed(2)}s</span></div>`;
    h += `</div>`;
  }
  h += `<div class="row gap" style="flex-wrap:wrap">`;
  if (c.kind === 'video') {
    if (hasAudio && c.audio) h += `<button class="soft small" data-act="detach_audio">Detach audio</button><button class="soft small" data-act="remove_audio">Delete audio</button>`;
    else if (m && m.hasAudio) h += `<button class="soft small" data-act="restore_audio">Restore audio</button>`;
    h += `<button class="soft small" data-act="remove_video">Delete video (keep audio)</button>`;
  }
  if (c.kind === 'html') h += `<button class="soft small" data-act="edit_html">Edit HTML</button>`;
  h += `<button class="soft small danger" data-act="delete">Delete clip</button></div>`;
  box.innerHTML = h;

  $$('input[type=range][data-k]', box).forEach((r) => {
    const v = $('.v', r.parentElement);
    r.oninput = () => { v.textContent = r.value; };
    r.onchange = () => op('set_clip', { id: c.id, [r.dataset.k]: parseFloat(r.value) });
  });
  $$('[data-fit] button', box).forEach((b) => (b.onclick = () => op('set_clip', { id: c.id, fit: b.dataset.v })));
  const tk = $('#trKind', box);
  if (tk) tk.onchange = () => op('set_transition', { clipId: c.id, kind: tk.value, duration: c.transition ? c.transition.duration : 0.5 });
  const td = $('#trDur', box);
  if (td) td.onchange = () => op('set_transition', { clipId: c.id, kind: c.transition.kind, duration: parseFloat(td.value) });
  $$('[data-act]', box).forEach((b) => (b.onclick = async () => {
    const a = b.dataset.act;
    if (a === 'delete') op('remove_clip', { id: c.id });
    else if (a === 'restore_audio') op('set_clip', { id: c.id, audio: true });
    else if (a === 'edit_html') editHtml(c);
    else op(a, { id: c.id });
  }));
}
function editHtml(c) {
  const m = modal(`<h3>HTML animation</h3><textarea id="htmlSrc" style="height:340px;font:12px var(--mono)" spellcheck="false">${esc(c.html || '')}</textarea><p class="muted small">Self-contained, transparent background. Animate with CSS or define <code>window.__seek(t)</code>. Claude can write this for you.</p><div class="row gap" style="justify-content:flex-end"><button class="soft" id="hNo">Cancel</button><button class="primary" id="hOk">Save</button></div>`, true);
  $('#hNo', m).onclick = closeModal;
  $('#hOk', m).onclick = async () => { const v = $('#htmlSrc', m).value; closeModal(); await op('set_clip', { id: c.id, html: v }); };
}

// ---------------- export pane ----------------
function renderExport() {
  const box = $('#pane-export');
  if (box.contains(document.activeElement) && document.activeElement.tagName !== 'BUTTON') return;
  const s = P().settings;
  const groups = {};
  ui.presets.forEach((p) => (groups[p.group] ||= []).push(p));
  const cur = ui.presets.find((p) => p.id === s.preset);
  const warn = cur && cur.maxSeconds && S.duration > cur.maxSeconds ? `<div class="small" style="color:#b5483c">Longer than ${cur.maxSeconds}s limit for ${esc(cur.label)}.</div>` : '';
  const seg = (key, opts) => `<div class="seg-btns" data-s="${key}">${opts.map(([v, l]) => `<button data-v="${v}" class="${String(s[key]) === String(v) ? 'on' : ''}">${l}</button>`).join('')}</div>`;
  const sel = (key, opts) => `<select data-sel="${key}">${opts.map((o) => `<option value="${o}" ${String(s[key]) === String(o) ? 'selected' : ''}>${o}</option>`).join('')}</select>`;
  box.innerHTML = `
    <h4>Format</h4>
    <div class="card">
      <select id="preset">${Object.entries(groups).map(([g, l]) => `<optgroup label="${esc(g)}">${l.map((p) => `<option value="${p.id}" ${s.preset === p.id ? 'selected' : ''}>${esc(p.label)} · ${p.width}×${p.height}</option>`).join('')}</optgroup>`).join('')}<option value="custom" ${s.preset === 'custom' ? 'selected' : ''}>Custom</option></select>
      <div class="muted small" style="margin-top:6px">${s.width}×${s.height} · ${s.fps} fps${cur ? ' · ' + esc(cur.note) : ''}</div>${warn}
      <div class="row gap" style="margin-top:8px"><input type="number" data-num="width" value="${s.width}"><span class="muted">×</span><input type="number" data-num="height" value="${s.height}"><input type="number" data-num="fps" value="${s.fps}" title="fps" style="width:70px"></div>
    </div>
    <h4>Quality</h4>
    <div class="card">${seg('quality', [['draft', 'Draft'], ['good', 'Good'], ['high', 'High'], ['max', 'Max']])}
      <div class="muted small" style="margin-top:6px">Draft renders from the proxies, fastest. Max is smallest-loss, slowest.</div>
      <div class="field" style="margin-top:8px"><label>Video kbps</label><input type="number" data-num="videoBitrateK" value="${s.videoBitrateK}" placeholder="0 = auto"><span></span></div>
    </div>
    <h4>Audio</h4>
    <div class="card">
      <div class="field"><label>Bitrate</label>${sel('audioBitrateK', [96, 128, 192, 256, 320])}<span class="muted small">kbps</span></div>
      <div class="field"><label>Sample rate</label>${sel('sampleRate', [44100, 48000])}<span class="muted small">Hz</span></div>
      <div class="field"><label>Channels</label>${seg('channels', [[2, 'Stereo'], [1, 'Mono']])}<span></span></div>
      <label class="row gap small" style="margin-top:6px"><input type="checkbox" data-chk="normalizeAudio" ${s.normalizeAudio ? 'checked' : ''}> Normalize loudness (−14 LUFS)</label>
    </div>
    <div class="card">
      <label class="row gap small"><input type="checkbox" data-chk="hardware" ${s.hardware ? 'checked' : ''}> Use GPU encoder when available</label>
      <div class="field" style="margin-top:6px"><label>Container</label>${sel('format', ['mp4', 'mov'])}<span></span></div>
    </div>
    <button class="primary" id="bExport2">Export video</button>`;
  $('#preset', box).onchange = (e) => op('set_settings', { preset: e.target.value });
  $$('[data-s] button', box).forEach((b) => (b.onclick = () => { const k = b.parentElement.dataset.s; const v = b.dataset.v; op('set_settings', { [k]: isNaN(v) ? v : Number(v) }); }));
  $$('[data-sel]', box).forEach((e) => (e.onchange = () => op('set_settings', { [e.dataset.sel]: isNaN(e.value) ? e.value : Number(e.value) })));
  $$('[data-num]', box).forEach((e) => (e.onchange = () => op('set_settings', { [e.dataset.num]: Number(e.value) })));
  $$('[data-chk]', box).forEach((e) => (e.onchange = () => op('set_settings', { [e.dataset.chk]: e.checked })));
  $('#bExport2', box).onclick = doExport;
}

async function doExport() {
  if (S.duration <= 0) return toast('Add something to the timeline first');
  const ext = P().settings.format || 'mp4';
  const path = await T.dialog.save({ defaultPath: `${P().name}.${ext}`, filters: [{ name: 'Video', extensions: [ext] }] });
  if (!path) return;
  try { await invoke('render_start', { path }); } catch (e) { toast(String(e)); }
}
function renderRenderModal() {
  const r = S.render;
  if (r.state === 'running') ui.renderModal = true;
  if (!ui.renderModal) return;
  if (r.state === 'idle') { ui.renderModal = false; return; }
  const pct = Math.round(r.progress * 100);
  const running = r.state === 'running';
  const title = running ? 'Rendering…' : r.state === 'done' ? 'Export finished' : r.state === 'cancelled' ? 'Cancelled' : 'Export failed';
  modal(`<h3>${title}</h3><div class="muted small" style="word-break:break-all">${esc(running ? r.message : r.state === 'done' ? r.output : r.message)}</div>
    <div class="bar"><i style="width:${r.state === 'done' ? 100 : pct}%"></i></div>
    <div class="row between"><span class="muted small">${running ? pct + '%' : ''}</span>${running ? '<button class="soft" id="rCancel">Cancel</button>' : '<button class="primary" id="rClose">Close</button>'}</div>`);
  const c = $('#rCancel'); if (c) c.onclick = () => invoke('render_cancel');
  const x = $('#rClose'); if (x) x.onclick = () => { ui.renderModal = false; closeModal(); };
}

// ---------------- timeline ----------------
const sc = () => $('#scroll');
function timeAtX(clientX) { const r = $('#canvas').getBoundingClientRect(); return Math.max(0, (clientX - r.left) / ui.pps); }
function snapTime(t, skipId) {
  if (!ui.snap) return t;
  const pts = [0, ui.t];
  P().tracks.forEach((tr) => tr.clips.forEach((c) => { if (c.id !== skipId) pts.push(c.start, c.start + c.duration); }));
  const tol = 8 / ui.pps;
  let best = t, d = tol;
  pts.forEach((p) => { if (Math.abs(p - t) < d) { d = Math.abs(p - t); best = p; } });
  return best;
}
function drawWave(canvas, m, c) {
  const data = waves.get(m.id);
  if (!data) return;
  const w = Math.min(4000, Math.max(1, Math.round(c.duration * ui.pps)));
  const h = canvas.parentElement.clientHeight || 40;
  canvas.width = w; canvas.height = h;
  const g = canvas.getContext('2d');
  g.fillStyle = c.kind === 'audio' ? getComputedStyle(document.documentElement).getPropertyValue('--aud-b') : 'rgba(255,255,255,.75)';
  const mid = h / 2;
  for (let x = 0; x < w; x++) {
    const t0 = c.inPoint + (x / w) * c.duration * c.speed, t1 = c.inPoint + ((x + 1) / w) * c.duration * c.speed;
    let mx = 0;
    for (let i = Math.floor(t0 * 20); i <= Math.floor(t1 * 20) && i < data.length; i++) mx = Math.max(mx, data[i]);
    const a = (mx / 255) * mid * 0.95;
    g.fillRect(x, mid - a, 1, Math.max(1, a * 2));
  }
}
async function ensureWave(m) {
  if (!m || !m.wave || waves.has(m.id)) return;
  waves.set(m.id, null);
  try {
    const buf = await (await fetch(convertFileSrc(m.wave))).arrayBuffer();
    waves.set(m.id, new Uint8Array(buf));
    renderTimeline();
  } catch { waves.delete(m.id); }
}

function renderTimeline() {
  const p = P();
  const pps = ui.pps;
  const scr = sc();
  const left = scr.scrollLeft, top = scr.scrollTop;
  const total = Math.max(S.duration + 10, scr.clientWidth / pps);
  $('#canvas').style.width = total * pps + 'px';

  // ruler
  const steps = [0.1, 0.25, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600];
  const step = steps.find((s) => s * pps >= 70) || 600;
  let rh = '';
  for (let t = 0; t <= total; t += step) rh += `<div class="tick maj" style="left:${t * pps}px"><span>${step < 1 ? t.toFixed(2) : fmt(t).replace(/\.00$/, '')}</span></div><div class="tick" style="left:${(t + step / 2) * pps}px"></div>`;
  $('#ruler').innerHTML = rh;

  // heads
  const heads = $('#heads');
  heads.innerHTML = '';
  const lanes = $('#lanes');
  lanes.innerHTML = '';
  p.tracks.forEach((tr) => {
    const h = document.createElement('div');
    h.className = 'head ' + tr.kind;
    h.innerHTML = `<div class="nm" title="Double-click to rename">${esc(tr.name)}</div><div class="tb">
      <button data-a="muted" class="${tr.muted ? 'on' : ''}" title="Mute">M</button>
      ${tr.kind === 'video' ? `<button data-a="hidden" class="${tr.hidden ? 'on' : ''}" title="Hide">H</button>` : ''}
      <button data-a="locked" class="${tr.locked ? 'on' : ''}" title="Lock">L</button>
      <button data-a="del" title="Remove track">✕</button></div>`;
    $('.nm', h).ondblclick = async () => { const n = await ask('Track name', tr.name); if (n) op('set_track', { id: tr.id, name: n }); };
    $$('button', h).forEach((b) => (b.onclick = async () => {
      const a = b.dataset.a;
      if (a === 'del') { if (!tr.clips.length || await confirmBox('Remove track?', `${tr.name} and its ${tr.clips.length} clip(s) will be removed.`, 'Remove')) op('remove_track', { id: tr.id }); }
      else op('set_track', { id: tr.id, [a]: !tr[a] });
    }));
    heads.appendChild(h);

    const lane = document.createElement('div');
    lane.className = 'lane ' + tr.kind + (tr.locked ? ' locked' : '');
    lane.dataset.track = tr.id;
    lane.ondragover = (e) => { if (e.dataTransfer.types.includes('application/x-cutlight')) { e.preventDefault(); lane.classList.add('over'); } };
    lane.ondragleave = () => lane.classList.remove('over');
    lane.ondrop = (e) => {
      e.preventDefault();
      lane.classList.remove('over');
      const id = e.dataTransfer.getData('application/x-cutlight');
      const m = media(id);
      if (!m) return;
      const start = snapTime(timeAtX(e.clientX));
      const args = { mediaId: id, trackId: tr.id, start };
      if (tr.kind === 'audio') args.mode = 'audio';
      else if (m.kind === 'audio') return toast('Audio files go on audio tracks');
      op('add_clip', args);
    };
    lane.onpointerdown = (e) => { if (e.target === lane) { ui.sel = null; renderInspector(); markSel(); setTime(timeAtX(e.clientX)); } };
    tr.clips.forEach((c) => lane.appendChild(clipEl(c, tr)));
    lanes.appendChild(lane);
  });
  heads.scrollTop = top;
  scr.scrollLeft = left;
  placePlayhead();
}

function clipEl(c, tr) {
  const pps = ui.pps;
  const m = c.mediaId && media(c.mediaId);
  const d = document.createElement('div');
  d.className = `clip ${c.kind}${ui.sel === c.id ? ' sel' : ''}`;
  d.dataset.id = c.id;
  d.style.left = c.start * pps + 'px';
  d.style.width = Math.max(4, c.duration * pps) + 'px';
  if (m && m.strip && c.kind !== 'audio') {
    d.style.backgroundImage = `url("${convertFileSrc(m.strip)}")`;
    d.style.backgroundSize = c.kind === 'image' ? '100% 100%' : `${(m.duration / c.speed) * pps}px 100%`;
    d.style.backgroundPosition = c.kind === 'image' ? '0 0' : `${-(c.inPoint / c.speed) * pps}px 0`;
  }
  let inner = '';
  if (c.transition) inner += `<div class="tr" style="width:${c.transition.duration * pps}px" title="${esc(c.transition.kind)}"></div>`;
  inner += `<div class="lab">${c.kind === 'html' ? '&lt;/&gt; ' : ''}${esc(c.label)}</div>`;
  if (m && (c.kind === 'audio' || (c.kind === 'video' && c.audio && m.hasAudio))) {
    inner += '<canvas></canvas>';
    ensureWave(m);
  }
  const badges = [];
  if (Math.abs(c.speed - 1) > 0.01) badges.push(c.speed.toFixed(2).replace(/\.?0+$/, '') + '×');
  if (c.kind === 'video' && m && m.hasAudio && !c.audio) badges.push('no audio');
  if (badges.length) inner += `<div class="badge">${badges.join(' · ')}</div>`;
  inner += '<div class="h l"></div><div class="h r"></div>';
  d.innerHTML = inner;
  const cv = $('canvas', d);
  if (cv) {
    if (c.kind === 'video') { cv.style.top = '55%'; cv.style.height = '45%'; }
    requestAnimationFrame(() => drawWave(cv, m, c));
  }
  d.onpointerdown = (e) => clipPointerDown(e, c, tr, d);
  d.oncontextmenu = (e) => {
    e.preventDefault();
    ui.sel = c.id; markSel(); renderInspector();
    const items = [['Split at playhead', () => op('split_clip', { id: c.id, at: ui.t })]];
    if (c.kind === 'video') items.push(['Detach audio', () => op('detach_audio', { id: c.id })], ['Delete audio', () => op('remove_audio', { id: c.id })], ['Delete video (keep audio)', () => op('remove_video', { id: c.id })]);
    items.push(['Delete', () => op('remove_clip', { id: c.id }), 'danger'], ['Ripple delete', () => op('remove_clip', { id: c.id, ripple: true }), 'danger']);
    menu(e.clientX, e.clientY, items);
  };
  return d;
}
function markSel() { $$('.clip').forEach((e) => e.classList.toggle('sel', e.dataset.id === ui.sel)); }

function clipPointerDown(e, c, tr, d) {
  if (e.button !== 0) return;
  e.stopPropagation();
  ui.sel = c.id;
  markSel();
  renderInspector();
  if (tr.locked) return;
  const pps = ui.pps;
  const mode = e.target.classList.contains('l') ? 'l' : e.target.classList.contains('r') ? 'r' : 'move';
  const x0 = e.clientX;
  let moved = false, newStart = c.start, newEnd = c.start + c.duration, laneId = tr.id;
  d.setPointerCapture(e.pointerId);
  const mv = (ev) => {
    const dx = (ev.clientX - x0) / pps;
    if (!moved && Math.abs(ev.clientX - x0) < 3 && mode === 'move' && ev.clientY === e.clientY) return;
    moved = true;
    if (mode === 'move') {
      newStart = Math.max(0, snapTime(c.start + dx, c.id));
      const e2 = snapTime(newStart + c.duration, c.id);
      if (e2 !== newStart + c.duration && Math.abs(e2 - (newStart + c.duration)) < Math.abs(newStart - (c.start + dx))) newStart = Math.max(0, e2 - c.duration);
      d.style.left = newStart * pps + 'px';
      d.classList.add('dragging');
      d.style.pointerEvents = 'none';
      const under = document.elementFromPoint(ev.clientX, ev.clientY)?.closest('.lane');
      d.style.pointerEvents = '';
      $$('.lane.over').forEach((l) => l.classList.remove('over'));
      if (under && under.dataset.track !== tr.id) {
        const t2 = P().tracks.find((x) => x.id === under.dataset.track);
        if (t2 && t2.kind === tr.kind) { laneId = t2.id; under.classList.add('over'); }
      } else laneId = tr.id;
    } else if (mode === 'l') {
      newStart = clamp(snapTime(c.start + dx, c.id), 0, c.start + c.duration - 0.05);
      d.style.left = newStart * pps + 'px';
      d.style.width = (c.start + c.duration - newStart) * pps + 'px';
    } else {
      newEnd = Math.max(c.start + 0.05, snapTime(c.start + c.duration + dx, c.id));
      d.style.width = (newEnd - c.start) * pps + 'px';
    }
  };
  const up = async () => {
    d.removeEventListener('pointermove', mv);
    d.removeEventListener('pointerup', up);
    $$('.lane.over').forEach((l) => l.classList.remove('over'));
    if (!moved) return;
    if (mode === 'move') await op('move_clip', { id: c.id, start: newStart, trackId: laneId });
    else if (mode === 'l') await op('trim_clip', { id: c.id, side: 'start', at: newStart });
    else await op('trim_clip', { id: c.id, side: 'end', at: newEnd });
    renderTimeline();
  };
  d.addEventListener('pointermove', mv);
  d.addEventListener('pointerup', up);
}

function placePlayhead() {
  $('#playhead').style.left = ui.t * ui.pps + 'px';
}

// ---------------- transport ----------------
function setTime(t, fromClock) {
  ui.t = Math.max(0, t);
  $('#tc').textContent = fmt(ui.t);
  placePlayhead();
  if (!fromClock && ui.playing) { ui.base = performance.now(); ui.base0 = ui.t; }
  if (S) preview.update(ui.t, ui.playing);
  if (ui.playing) {
    const scr = sc(), x = ui.t * ui.pps;
    if (x > scr.scrollLeft + scr.clientWidth - 40) scr.scrollLeft = x - 80;
  }
}
function play() {
  if (ui.playing) return;
  if (S.duration <= 0) return;
  if (ui.t >= S.duration - 0.05) ui.t = 0;
  ui.playing = true;
  ui.base = performance.now();
  ui.base0 = ui.t;
  $('#bPlay').innerHTML = I.pause;
  requestAnimationFrame(loop);
}
function pause() {
  ui.playing = false;
  $('#bPlay').innerHTML = I.play;
  preview.stopAll();
  preview.update(ui.t, false);
}
function loop() {
  if (!ui.playing) return;
  const t = ui.base0 + (performance.now() - ui.base) / 1000;
  if (t >= S.duration) { setTime(S.duration, true); pause(); return; }
  setTime(t, true);
  requestAnimationFrame(loop);
}

function setZoom(v) {
  const scr = sc();
  const center = (scr.scrollLeft + scr.clientWidth / 2) / ui.pps;
  ui.pps = clamp(v, 10, 400);
  $('#zoom').value = ui.pps;
  renderTimeline();
  scr.scrollLeft = center * ui.pps - scr.clientWidth / 2;
}

// ---------------- wiring ----------------
function init() {
  preview = new Preview($('#stage'), convertFileSrc);
  $('#bUndo').innerHTML = I.undo; $('#bRedo').innerHTML = I.redo;
  $('#bStart').innerHTML = I.start; $('#bPrev').innerHTML = I.prev; $('#bNext').innerHTML = I.next; $('#bPlay').innerHTML = I.play;

  $('#bUndo').onclick = () => op('undo'); $('#bRedo').onclick = () => op('redo');
  $('#bHome').onclick = async () => { pause(); await op('close_project'); };
  $('#hOpen').onclick = openDialog;
  $('#bSave').onclick = saveProject;
  $('#bConnect').onclick = (e) => connectClaude(e.currentTarget);
  $('#bConnectTop').onclick = (e) => connectClaude(e.currentTarget);
  $('#hConnect').onclick = (e) => connectClaude(e.currentTarget);
  $('#projName').onchange = (e) => op('set_settings', { name: e.target.value });
  $('#script').onchange = (e) => op('set_script', { text: e.target.value });
  $('#bImport').onclick = pickImport;
  $('#bTranscribe').onclick = async () => {
    const id = $('#trMedia').value; if (!id) return toast('Import media with audio first');
    toast('Transcribing…'); const r = await op('transcribe', { mediaId: id, language: $('#trLang').value }); if (r) toast(`Transcribed ${r.count} segments`);
  };
  $$('.copy').forEach((e) => (e.onclick = () => { navigator.clipboard.writeText(e.textContent); toast('Copied'); }));

  const tabs = (nav, prefix) => $$('button', $(nav)).forEach((b) => (b.onclick = () => {
    $$('button', $(nav)).forEach((x) => x.classList.toggle('on', x === b));
    $$('.pane', $(nav).parentElement).forEach((p) => p.classList.toggle('on', p.id === `pane-${b.dataset.tab}`));
  }));
  tabs('#leftTabs'); tabs('#rightTabs');

  $('#bPlay').onclick = () => (ui.playing ? pause() : play());
  $('#bStart').onclick = () => setTime(0);
  $('#bPrev').onclick = () => setTime(ui.t - frameDur());
  $('#bNext').onclick = () => setTime(ui.t + frameDur());

  $('#tSplit').onclick = split;
  $('#tDelete').onclick = () => ui.sel && op('remove_clip', { id: ui.sel });
  $('#tRipple').onclick = () => ui.sel && op('remove_clip', { id: ui.sel, ripple: true });
  $('#tAddV').onclick = () => op('add_track', { kind: 'video' });
  $('#tAddA').onclick = () => op('add_track', { kind: 'audio' });
  $('#tOrganize').onclick = (e) => { const r = e.target.getBoundingClientRect(); menu(r.left, r.top - 100, [['Close all gaps', () => op('organize', { mode: 'close_gaps' })], ['Tidy (gaps + empty tracks)', () => op('organize', { mode: 'tidy' })], ['Move everything to start', () => op('organize', { mode: 'align_start' })]]); };
  $('#tSnap').onchange = (e) => (ui.snap = e.target.checked);
  $('#zoom').oninput = (e) => setZoom(+e.target.value);
  $('#zIn').onclick = () => setZoom(ui.pps * 1.3); $('#zOut').onclick = () => setZoom(ui.pps / 1.3);
  $('#zFit').onclick = fitZoom;

  // scrubbing on ruler
  $('#ruler').onpointerdown = (e) => {
    const move = (ev) => setTime(timeAtX(ev.clientX));
    move(e); $('#ruler').setPointerCapture(e.pointerId);
    $('#ruler').onpointermove = move;
    $('#ruler').onpointerup = () => { $('#ruler').onpointermove = null; };
  };
  $('#scroll').addEventListener('wheel', (e) => { if (e.ctrlKey) { e.preventDefault(); setZoom(ui.pps * (e.deltaY < 0 ? 1.15 : 1 / 1.15)); } }, { passive: false });
  $('#scroll').addEventListener('scroll', () => ($('#heads').scrollTop = $('#scroll').scrollTop));
  new ResizeObserver(() => S && preview.update(ui.t, ui.playing)).observe($('#stageWrap'));

  addEventListener('keydown', onKey);
  addEventListener('dragover', (e) => { if (e.dataTransfer.types.includes('Files')) e.preventDefault(); });

  listen('state', (e) => { S = e.payload; onState(); });
  listen('toast', (e) => toast(e.payload));
  listen('tauri://drag-drop', (e) => { document.body.classList.remove('dropping'); importPaths(e.payload.paths || [], true); });
  listen('tauri://drag-enter', () => document.body.classList.add('dropping'));
  listen('tauri://drag-leave', () => document.body.classList.remove('dropping'));
  listen('html-render', async (e) => {
    const { key, html, width, height, fps, frames } = e.payload;
    try {
      await renderHtmlFrames({ html, width, height, fps, frames }, (buf) => invoke('html_frame', buf, { headers: { 'x-key': key } }));
      await invoke('html_finish', { key });
    } catch (err) { await invoke('html_finish', { key, error: String(err) }); }
  });
  listen('ui-eval', async (e) => {
    const { id, js } = e.payload;
    try {
      const AF = Object.getPrototypeOf(async function () {}).constructor;
      let fn;
      try { fn = new AF(/\breturn\b/.test(js) ? js : 'return (' + js + ')'); } catch { fn = new AF(js); }
      let value = await fn();
      try { JSON.stringify(value); } catch { value = String(value); }
      await invoke('ui_reply', { id, value: value === undefined ? null : value });
    } catch (err) { await invoke('ui_reply', { id, error: String(err) }); }
  });

  window.cutlight = {
    state: () => ({ t: ui.t, playing: ui.playing, selected: ui.sel, pxPerSec: ui.pps, duration: S.duration, scrollLeft: sc().scrollLeft, leftTab: $('#leftTabs .on').dataset.tab, rightTab: $('#rightTabs .on').dataset.tab, render: S.render }),
    control(a, v) {
      if (a === 'play') play(); else if (a === 'pause') pause();
      else if (a === 'seek') setTime(Number(v));
      else if (a === 'select') { ui.sel = v; markSel(); renderInspector(); const f = findClip(v); if (f) setTime(f.c.start); }
      else if (a === 'zoom') setZoom(Number(v));
      else if (a === 'fit_timeline') fitZoom();
      else if (a === 'tab') { const b = $(`[data-tab="${v}"]`); if (b) b.click(); else throw new Error('unknown tab'); }
      else throw new Error('unknown action');
      return window.cutlight.state();
    },
    click(sel) {
      let el;
      if (sel.startsWith('text=')) { const t = sel.slice(5).toLowerCase(); el = $$('button,[role=button],a').find((b) => b.textContent.trim().toLowerCase() === t); }
      else el = $(sel);
      if (!el) throw new Error('element not found: ' + sel);
      el.click();
      return true;
    },
  };

  invoke('op', { name: 'list_presets', args: {} }).then((l) => { ui.presets = l; if (S) renderExport(); });
  invoke('get_state').then((s) => { S = s; onState(); setTime(0); });
}

function fitZoom() {
  const w = sc().clientWidth - 40;
  setZoom(w / Math.max(S.duration, 5));
  sc().scrollLeft = 0;
}
async function saveProject() {
  // projects autosave; this writes the project to a file of your choice and continues editing there
  const path = await T.dialog.save({ defaultPath: `${P().name}.cutlight`, filters: [{ name: 'Cutlight project', extensions: ['cutlight'] }] });
  if (path) op('save_project', { path }).then(() => toast('Saved'));
}
function split() {
  const args = { at: ui.t };
  const f = ui.sel && findClip(ui.sel);
  if (f && ui.t > f.c.start && ui.t < f.c.start + f.c.duration) args.id = ui.sel;
  op('split_clip', args);
}
function onKey(e) {
  if (!S || !S.open) return;
  const tag = e.target.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || e.target.isContentEditable) return;
  const k = e.key.toLowerCase(), mod = e.ctrlKey || e.metaKey;
  if (mod && k === 'z') { e.preventDefault(); op(e.shiftKey ? 'redo' : 'undo'); }
  else if (mod && k === 'y') { e.preventDefault(); op('redo'); }
  else if (mod && k === 's') { e.preventDefault(); toast('Projects save automatically'); }
  else if (mod && k === 'e') { e.preventDefault(); doExport(); }
  else if (k === ' ') { e.preventDefault(); ui.playing ? pause() : play(); }
  else if (k === 's' && !mod) split();
  else if (k === 'delete' || k === 'backspace') { if (ui.sel) op('remove_clip', { id: ui.sel, ripple: e.shiftKey }); }
  else if (k === 'arrowleft') setTime(ui.t - (e.shiftKey ? 1 : frameDur()));
  else if (k === 'arrowright') setTime(ui.t + (e.shiftKey ? 1 : frameDur()));
  else if (k === 'home') setTime(0);
  else if (k === 'end') setTime(S.duration);
  else if (k === '=' || k === '+') setZoom(ui.pps * 1.3);
  else if (k === '-') setZoom(ui.pps / 1.3);
}

init();
