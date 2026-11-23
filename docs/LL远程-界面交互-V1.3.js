/* Documentation prototype only: synthetic authorization references; no native calls or saved real passwords. */
const DEMO_STORAGE_KEY = 'llremote-plan-v13-demo';
const defaultDevices = [
  { id: 'home', name: '大娃家', status: 'online', favorite: true },
  { id: 'code-friend', name: '朋友的电脑', status: 'unknown', favorite: false, source: 'code', peerCode: '45881236', lastConnectedAt: '2026-11-20 20:16', mouseMode: 'office', credentialAvailable: true, credentialRef: 'demo-credential-code-friend', credentialState: 'valid' },
  { id: 'desktop', name: 'DESKTOP-E4NTI0J', status: 'pending', favorite: false },
  { id: 'rog', name: 'ROG', status: 'offline', favorite: true },
  { id: 'xiaoxiao', name: '晓晓家', status: 'offline', favorite: false }
];
const defaultHistory = [{ deviceId: 'code-friend', connectedAt: '2026-11-20 20:16', method: '识别码', result: 'success' }];
function loadDemoData() {
  try {
    const data = JSON.parse(localStorage.getItem(DEMO_STORAGE_KEY));
    if (!Array.isArray(data?.devices) || !Array.isArray(data?.history)) return null;
    data.devices = data.devices.filter(d => typeof d.id === 'string' && /^[\w-]{1,100}$/.test(d.id) && typeof d.name === 'string' && d.name.trim());
    if (!data.devices.some(d => d.id === 'home')) return null;
    data.devices.forEach(d => { if (d.source === 'code') d.status = 'unknown'; });
    data.history = data.history.filter(h => data.devices.some(d => d.id === h.deviceId) && typeof h.connectedAt === 'string');
    return data;
  } catch { return null; }
}
const restoredDemo = loadDemoData();
const devices = restoredDemo?.devices || JSON.parse(JSON.stringify(defaultDevices));
const history = restoredDemo?.history || JSON.parse(JSON.stringify(defaultHistory));
const state = {
  view: 'home', device: devices[0], requiresSetup: false, setupDone: true,
  username: 'sunshine', sunshineRunning: false, quota: 'unknown', quotaMinutes: 750,
  preferences: { noSleep: true, autostart: false, displayAwake: false, tray: true, absoluteMouse: true, autoRetry: true, acceptControl: false },
  session: { active: false, resolution: '1920x1080', bitrate: 15, fps: 60, volume: 75, muted: false, mode: 'fullscreen', mouseMode: 'office', monitoring: false },
  lastAdvanced: null
};
const content = document.getElementById('pageContent');
let toastTimer;
function persistDemoData() {
  try { localStorage.setItem(DEMO_STORAGE_KEY, JSON.stringify({ devices, history })); return true; }
  catch { return false; }
}
function hasSavedCredential(device) { return Boolean(device.credentialAvailable && device.credentialState === 'valid' && typeof device.credentialRef === 'string' && device.credentialRef.startsWith('demo-credential-')); }
function wallpaperMarkup(device, cssClass = 'cover-art') {
  if (typeof device.wallpaper === 'string' && /^data:image\/(jpeg|png|webp);base64,/.test(device.wallpaper)) return '<img class="' + cssClass + '" src="' + escapeHtml(device.wallpaper) + '" alt="">';
  return '<svg class="' + cssClass + '" viewBox="0 0 1000 400" preserveAspectRatio="xMidYMid slice" aria-hidden="true"><use href="#landscape"/></svg>';
}
function escapeHtml(text) { return String(text).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c])); }
function icon(name) { return '<svg class="icon" aria-hidden="true"><use href="#icon-' + name + '"/></svg>'; }
function toast(message) {
  const el = document.getElementById('toast'); el.textContent = message; el.classList.add('visible');
  clearTimeout(toastTimer); toastTimer = setTimeout(() => el.classList.remove('visible'), 3200);
}
function heading(title, description = '') { return '<h1 class="page-title">' + title + '</h1><p class="page-description">' + description + '</p>'; }
function advanced(app) { return '<div class="advanced-row"><span>完整的应用设置请前往</span><button data-advanced="' + app + '">高级设置</button></div>'; }
function section(title, rows, id = '') { return '<section class="page-section"' + (id ? ' id="' + id + '"' : '') + '><h2>' + title + '</h2>' + rows + '</section>'; }
function row(label, control, detail = '') { return '<div class="setting-row"><div><div class="setting-name">' + label + '</div>' + (detail ? '<div class="setting-detail">' + detail + '</div>' : '') + '</div><div class="setting-control">' + control + '</div></div>'; }
function toggle(key) { return '<button class="toggle ' + (state.preferences[key] ? 'on' : '') + '" data-toggle="' + key + '" role="switch" aria-label="' + key + '" aria-checked="' + state.preferences[key] + '"><span></span></button>'; }
function select(id, values, value) { return '<select id="' + id + '">' + values.map(item => { const option = Array.isArray(item) ? item : [item, item]; return '<option value="' + escapeHtml(option[0]) + '"' + (option[0] === value ? ' selected' : '') + '>' + escapeHtml(option[1]) + '</option>'; }).join('') + '</select>'; }
function button(text, action, style = 'secondary') { return '<button class="' + style + '" data-action="' + action + '">' + text + '</button>'; }
function message(text, kind = '') { return '<div class="inline-message ' + kind + '">' + text + '</div>'; }
function saveActions() { return '<div class="page-actions">' + button('保存设置', 'save', 'primary') + '</div><div id="pageFeedback"></div>'; }
function showFeedback(text, kind = 'success') {
  let el = document.getElementById('pageFeedback');
  if (!el) { el = document.createElement('div'); el.id = 'pageFeedback'; content.appendChild(el); }
  el.innerHTML = message(text, kind);
}
function renderNavigation() {
  document.getElementById('deviceNavigation').innerHTML = devices.map(d => '<button class="device-button ' + (['home', 'history'].includes(state.view) && state.device.id === d.id ? 'selected' : '') + '" data-device="' + escapeHtml(d.id) + '" aria-controls="pageContent">' + (d.status === 'online' ? '<span class="status-dot online"></span>' : '') + '<span class="device-icon"><i></i><i></i><i></i><i></i></span><span class="device-nav-name">' + escapeHtml(d.name) + '</span>' + (d.source === 'code' ? '<span class="source-badge">识别码</span>' : '') + '</button>').join('');
  document.querySelectorAll('[data-view]').forEach(el => { const active = el.dataset.view === state.view; el.classList.toggle('selected', active); if (active) el.setAttribute('aria-current', 'page'); else el.removeAttribute('aria-current'); });
}
function renderHome() {
  const d = state.device, status = { online: '在线', pending: '未就绪', offline: '离线', unknown: '未知' }[d.status] || '未知';
  return '<div class="device-header"><span class="online-pill ' + (d.status !== 'online' ? 'offline' : '') + '"><i></i>' + status + '</span><h1>' + escapeHtml(d.name) + '</h1><button class="more" id="deviceMenuButton" data-action="edit-device" aria-label="编辑设备">⋮</button></div>' +
    (d.source === 'code' ? '<div class="device-source-line"><span>识别码 ' + escapeHtml(d.peerCode) + ' · 最近连接 ' + escapeHtml(d.lastConnectedAt || '暂无记录') + '（示例）</span><button data-view="history">连接记录 →</button></div>' : '') +
    '<section class="desktop-card"><div class="cover" id="desktopCover" role="button" tabindex="0" data-action="connect" aria-label="进入 ' + escapeHtml(d.name) + ' 的桌面">' + wallpaperMarkup(d) + '<span class="enter">进入桌面 <span class="arrow">→</span></span></div><div class="tools"><button class="tool" data-view="moonlight">' + icon('monitor') + '画质设置</button><button class="tool" data-action="input-settings">' + icon('audio') + '输入与声音</button><button class="tool" data-view="diagnostics">' + icon('stats') + '连接诊断</button></div></section>' +
    '<h2 class="quick-title">快速启动</h2><section class="quick-area"><button class="quick-card" data-action="connect"><span class="quick-icon">' + icon('monitor') + '</span>Desktop</button><button class="quick-card" data-action="connect"><span class="quick-icon">' + icon('assist') + '</span>Steam Big Picture</button><button class="quick-card" data-view="quickAdd"><span class="add-circle">+</span>添加</button></section><div class="host-info"><i></i><span>防休眠默认开启 · 开机自启动默认关闭</span></div>';
}
function renderMoonlight() {
  return advanced('moonlight') + heading('Moonlight · 常用设置', '画质、窗口、输入和声音在此调整；每台设备可以保留自己的偏好。') +
    section('画面与串流', row('分辨率', select('moonResolution', ['1280 × 720', '1920 × 1080', '2560 × 1440', '3840 × 2160'], '1920 × 1080')) + row('帧率', select('moonFps', ['30 FPS', '60 FPS', '90 FPS', '120 FPS', '144 FPS'], '60 FPS')) + row('码率', '<div class="unit-field"><input class="text-input small" id="moonBitrate" type="number" value="15" min="1" max="500"><span>Mbps</span></div>') + row('视频编码', select('moonCodec', ['自动选择', 'H.264', 'HEVC', 'AV1'], '自动选择')) + row('窗口模式', select('moonWindow', ['全屏', '窗口', '无边框'], '窗口'))) +
    section('输入与声音', row('鼠标控制', select('moonMouseMode', [['game', '游戏鼠标'], ['office', '办公鼠标']], state.device.mouseMode || 'office'), '游戏鼠标使用相对坐标，办公鼠标使用远程桌面绝对鼠标优化。') + row('系统快捷键', select('moonKeys', ['仅全屏捕获', '始终捕获', '不捕获'], '仅全屏捕获')) + row('音频输出', select('moonAudio', ['系统默认设备', '耳机（示例）', '扬声器（示例）'], '系统默认设备')), 'inputSection') + saveActions();
}
function renderJiaoyue() {
  return advanced('jiaoyue') + heading('皎月连 · 常用设置', '使用皎月连账号和组网连接，原软件主界面可从高级设置打开。') +
    section('账号与网络', row('账号状态', '<span class="value-text good">已登录（示例）</span>' + '<button class="tiny-button" data-view="account">查看账号</button>') + row('主要连接方式', '<span class="value-text">组网模式</span>') + row('当前主机', select('natHost', ['大娃家', 'DESKTOP-E4NTI0J'], '大娃家') + button('连接', 'connect', 'small-button')) + row('网络抖动重试', toggle('autoRetry'), '有次数上限，密码或授权失败时停止。')) +
    section('本机接受控制', row('启用接受控制', toggle('acceptControl'), '配合本机 Sunshine 准备被控服务。') + row('本机识别码', '<span class="value-text">1234 5678（示例）</span>' + button('复制', 'mock', 'small-button')) + row('连接他人电脑', '<button class="tiny-button" data-view="assist">识别码 + 密码连接 →</button>')) + saveActions();
}
function renderSunshine() {
  return advanced('sunshine') + heading('Sunshine · 常用设置', '管理本机采集服务、管理账号和首次配对；原配置界面默认简体中文。') +
    section('管理账号与服务', row('管理账号', '<span class="value-text">' + escapeHtml(state.username) + '（示例）</span>' + '<button class="tiny-button" data-view="setup">设置 / 验证账号</button>') + row('服务状态', '<span class="value-text ' + (state.sunshineRunning ? 'good' : '') + '">' + (state.sunshineRunning ? '已启动' : '未启动') + '（示例）</span>' + button(state.sunshineRunning ? '停止' : '启动', 'sun-service', 'small-button')) + row('界面语言', '<span class="value-text">简体中文</span>')) +
    section('配对与采集', row('确认四位 PIN', '<input class="text-input small" id="sunPin" maxlength="4" inputmode="numeric" placeholder="四位 PIN">' + button('确认', 'pin', 'small-button'), '使用首次设置的本机 Sunshine 管理凭据。') + row('采集显示器', select('sunDisplay', ['默认显示器', '显示器 1（示例）', '显示器 2（示例）'], '默认显示器')) + row('音频设备', select('sunAudio', ['系统默认设备', '扬声器（示例）'], '系统默认设备')) + row('已发布应用', '<span class="value-text">Desktop · Steam Big Picture</span>')) + saveActions();
}
function renderSettings() {
  return heading('设置', 'LL远程的运行和界面偏好。设置按钮始终保留在左侧导航底部。') +
    section('运行与启动', row('防止电脑自动休眠', toggle('noSleep'), '默认开启，应用运行及托盘运行期间生效。') + row('开机自启动', toggle('autostart'), '默认关闭；开启后在当前用户登录 Windows 时后台启动。') + row('保持显示器常亮', toggle('displayAwake'), '默认关闭，与防止系统休眠分别控制。') + row('关闭窗口进入托盘', toggle('tray'), '默认开启；托盘提供明确的完全退出操作。')) +
    section('安装与组件', row('交付方式', '<span class="value-text">完整 EXE 安装版 / ZIP 便携版</span>', '两个包都自带 Sunshine、Moonlight、皎月连及依赖。') + row('组件管理', '<span class="value-text">由 LL远程统一初始化</span>') + row('首次使用向导', '<button class="tiny-button" data-view="setup">设置 Sunshine 管理账号 →</button>')) + saveActions();
}
function renderDevices(favorites = false) {
  const list = favorites ? devices.filter(d => d.favorite) : devices;
  return heading(favorites ? '收藏设备' : '全部设备', '设备目录包含识别码历史设备。历史与收藏分别保存，缓存不代表实时在线。') + '<div class="search-bar"><input class="text-input" id="deviceSearch" placeholder="搜索设备名称、备注或识别码">' + button('刷新', 'mock') + '</div><table class="device-table"><thead><tr><th>设备名称</th><th>状态</th><th>最近连接</th><th>操作</th></tr></thead><tbody>' + list.map(d => '<tr data-search="' + escapeHtml((d.name + ' ' + (d.peerCode || '')).toLowerCase()) + '"><td class="device-table-name">' + escapeHtml(d.name) + (d.source === 'code' ? '<small class="history-source">识别码 ' + escapeHtml(d.peerCode) + '</small>' : '') + '</td><td>' + ({ online: '在线', pending: '服务未就绪', offline: '离线', unknown: '未知' }[d.status] || '未知') + '</td><td>' + escapeHtml(d.lastConnectedAt || '—') + '</td><td><button class="tiny-button" data-device="' + escapeHtml(d.id) + '">查看设备 →</button></td></tr>').join('') + '</tbody></table><div id="pageFeedback"></div>';
}
function renderAccount() {
  let value = '暂无法读取', css = 'unknown', detail = '时长数据需要真实接口返回；可前往官网签到或查询。';
  if (state.quota === 'known') { value = Math.floor(state.quotaMinutes / 60) + ' 小时 ' + state.quotaMinutes % 60 + ' 分钟'; css = ''; detail = '示例数据，正式软件显示内核或官方接口返回的余额。'; }
  if (state.quota === 'zero') { value = '可用时长不足'; css = 'zero'; detail = '领取时长后返回软件刷新，成功后再尝试连接。'; }
  return heading('皎月连账号', '顶部账号按钮保留原位置，点击后在这里查看账号与可用连接时长。') +
    section('账号信息', row('当前账号', '<span class="value-text">LL演示账号</span>') + row('登录状态', '<span class="value-text good">已登录（示例）</span>')) +
    section('连接时长', row('剩余可用时长', '<span id="quotaValue" class="account-time ' + css + '">' + value + '</span>', detail) + row('更新时间', '<span class="value-text">' + (state.quota === 'unknown' ? '尚未取得数据' : '示例：刚刚更新') + '</span>') + row('官网签到', '<a class="secondary" href="https://www.natpierce.cn/pc/sign/index.html" target="_blank" rel="noopener noreferrer">前往官网签到 ↗</a>')) +
    '<div class="page-actions">' + button('刷新时长', 'refresh-quota') + button('退出登录', 'logout') + '</div><div id="pageFeedback"></div>';
}
function renderSetup() {
  return '<div class="setup-stepper"><span><b>1 Sunshine 管理账号</b></span><span>2 皎月连登录</span><span>3 设备连接</span></div>' + heading('首次使用 · 设置 Sunshine 账号', '先创建或验证本机管理账号，便于后续确认 PIN。原 Sunshine 配置界面默认中文。') +
    message('此处仅演示首次配置流程，请勿填写真实密码；草图不会向 Sunshine 写入账号。') +
    '<div class="inline-tabs"><button class="active" data-setup-mode="create">创建管理账号</button><button data-setup-mode="existing">使用已有账号</button></div><form class="setup-form" id="setupForm"><label class="form-label" for="setupUser">用户名</label><input class="text-input" id="setupUser" value="sunshine" required autocomplete="off"><label class="form-label" for="setupPassword">密码</label><input class="text-input" type="password" id="setupPassword" required autocomplete="new-password"><div id="confirmPasswordRow"><label class="form-label" for="setupConfirm">确认密码</label><input class="text-input" type="password" id="setupConfirm" required autocomplete="new-password"></div><p class="form-note">Sunshine 管理密码与皎月连账号密码、连接密码、PIN 分别使用。</p><div id="setupFeedback"></div><div class="page-actions start"><button class="primary" type="submit">保存并继续</button></div></form>';
}
function renderLogin() {
  return '<div class="setup-stepper"><span>1 Sunshine 管理账号</span><span><b>2 皎月连登录</b></span><span>3 设备连接</span></div>' + heading('登录皎月连', 'Sunshine 账号配置完成后，使用皎月连账号获取主机列表。') + message('这是界面方案演示，点击下方按钮使用示例设备；此页不收集实际账号密码。') + '<div class="setup-form">' + row('管理账号', '<span class="value-text good">已完成设置（示例）</span>') + '<div class="page-actions start">' + button('使用示例账号继续', 'demo-login', 'primary') + '</div></div>';
}
function renderAssist(mode = 'self') {
  return heading('开始协助', '在右侧页面显示本机连接信息，或通过识别码和密码连接其他人的电脑。') + '<div class="inline-tabs"><button class="' + (mode === 'self' ? 'active' : '') + '" data-assist-mode="self">让对方连接我</button><button class="' + (mode === 'other' ? 'active' : '') + '" data-assist-mode="other">连接对方电脑</button></div>' +
    (mode === 'self' ? section('本机连接信息', row('本机接受控制', toggle('acceptControl')) + '<div class="help-code">1234 5678</div><p class="page-description">示例识别码，实际由皎月连返回。</p><div class="page-actions start">' + button('复制连接信息', 'mock') + '</div>') : '<div class="setup-form"><label class="form-label" for="peerCode">对方识别码</label><input class="text-input" id="peerCode" value="' + (state.device.source === 'code' ? escapeHtml(state.device.peerCode) : '') + '" placeholder="此处填写示例识别码" autocomplete="off"><label class="form-label" for="peerPassword">连接密码</label><input class="text-input" id="peerPassword" type="password" placeholder="仅演示，请勿填写真实密码" autocomplete="off"><p class="form-note">首次成功后保存可复用授权，历史记录再次连接不再输入密码。草图仅保存模拟授权标记。</p><div class="page-actions start">' + button('连接（演示）', 'assist-connect', 'primary') + '</div></div>') + '<div id="pageFeedback"></div>';
}
function renderDiagnostics(reason = '') {
  return heading('连接诊断', '分别检查账号、皎月连网络、Sunshine、配对和串流，不把普通网络失败误判为时长不足。') + (reason ? message(reason, 'warning') : '') +
    section('连接步骤', row('皎月连账号', '<span class="value-text good">已登录（示例）</span>') + row('可用时长', '<span class="value-text">未知时长不等于 0</span>') + row('设备在线状态', '<span class="value-text">真实软件读取主机目录</span>') + row('Sunshine 与配对', '<span class="value-text">检查可达性与首次授权</span>') + row('Moonlight 串流', '<span class="value-text">启动及真实指标</span>')) + '<div class="page-actions">' + button('重新检查', 'mock', 'primary') + '</div><div id="pageFeedback"></div>';
}
function renderQuickAdd() {
  return heading('添加快速启动', '从 Sunshine 已发布应用中选择，入口仍在设备详情的快速启动区域。') + section('应用', row('应用名称', select('quickApp', ['Desktop', 'Steam Big Picture'], 'Desktop')) + row('显示名称', '<input class="text-input" id="quickName" placeholder="沿用应用名称">')) + saveActions();
}
function renderHistory() {
  const d = state.device, entries = history.filter(h => h.deviceId === d.id).slice().reverse();
  return heading('连接记录 · ' + escapeHtml(d.name), '成功连接的识别码主机显示于“我的设备”，收藏单独设置。此处时间与结果为演示数据。') +
    section('设备信息', row('识别码', '<span class="value-text">' + escapeHtml(d.peerCode || '—') + '</span>') + row('最近连接', '<span class="value-text">' + escapeHtml(d.lastConnectedAt || '—') + '</span>') + row('收藏状态', '<span class="value-text">' + (d.favorite ? '已收藏' : '未收藏') + '</span>') + row('历史重连', '<span class="value-text ' + (hasSavedCredential(d) ? 'good' : '') + '">' + (hasSavedCredential(d) ? '直接连接，无需重复输入密码' : '需更新连接凭据') + '</span>') + row('在线状态', '<span class="value-text">' + (d.status === 'online' ? '已连接（示例）' : '未知 · 历史不代表在线') + '</span>')) +
    section('连接历史', entries.length ? '<table class="device-table history-table"><thead><tr><th>时间</th><th>方式</th><th>结果</th></tr></thead><tbody>' + entries.map(h => '<tr><td>' + escapeHtml(h.connectedAt) + '</td><td>' + escapeHtml(h.method) + '</td><td>' + (h.result === 'success' ? '成功（示例）' : '失败（示例）') + '</td></tr>').join('') + '</tbody></table>' : '<div class="empty-history">暂无记录</div>') + '<div class="page-actions">' + '<button class="secondary" data-view="home">查看设备</button>' + button('再次连接', 'connect', 'primary') + '</div><div id="pageFeedback"></div>';
}
function route(view, options = {}) {
  if (state.requiresSetup && view !== 'setup') { toast('请先完成 Sunshine 管理账号设置。'); view = 'setup'; }
  state.view = view;
  const renderers = { home: renderHome, moonlight: renderMoonlight, jiaoyue: renderJiaoyue, sunshine: renderSunshine, settings: renderSettings, devices: () => renderDevices(), favorites: () => renderDevices(true), account: renderAccount, setup: renderSetup, login: renderLogin, assist: () => renderAssist(options.mode), diagnostics: () => renderDiagnostics(options.reason), quickAdd: renderQuickAdd, history: renderHistory };
  if (!renderers[view]) state.view = 'home';
  content.innerHTML = (renderers[state.view] || renderHome)(); content.scrollTop = 0; renderNavigation();
}

const editorBackdrop = document.getElementById('deviceEditorBackdrop');
let editorDraft = null, editorReturnFocus = null, editorImageRequest = 0;
function updateEditorWallpaper() {
  document.getElementById('editorWallpaperPreview').innerHTML = wallpaperMarkup(editorDraft, 'editor-cover');
  document.getElementById('editorWallpaperName').textContent = editorDraft.wallpaper ? editorDraft.wallpaperName || '自定义壁纸' : '默认壁纸';
}
function openDeviceEditor() {
  if (state.session.active || state.requiresSetup) return;
  editorReturnFocus = document.activeElement;
  editorDraft = { ...state.device }; editorImageRequest++;
  document.getElementById('editorName').value = state.device.name;
  document.getElementById('editorFavorite').checked = Boolean(state.device.favorite);
  document.getElementById('editorFeedback').innerHTML = '';
  document.querySelector('#deviceEditorForm button[type="submit"]').disabled = false;
  updateEditorWallpaper(); editorBackdrop.hidden = false; document.getElementById('appShell').inert = true;
  document.getElementById('editorName').focus(); document.getElementById('editorName').select();
}
function closeDeviceEditor() {
  editorBackdrop.hidden = true; document.getElementById('appShell').inert = false;
  editorDraft = null; editorImageRequest++; document.getElementById('editorWallpaperInput').value = '';
  if (editorReturnFocus?.isConnected) editorReturnFocus.focus();
  else document.getElementById('deviceMenuButton')?.focus();
}
function saveDeviceEditor() {
  if (!editorDraft) return;
  const name = document.getElementById('editorName').value.trim();
  if (!name || Array.from(name).length > 64) { document.getElementById('editorFeedback').innerHTML = message('请输入 1 至 64 个字符的设备名称。', 'warning'); return; }
  state.device.name = name; state.device.favorite = document.getElementById('editorFavorite').checked;
  state.device.wallpaper = editorDraft.wallpaper || ''; state.device.wallpaperName = editorDraft.wallpaperName || '';
  const persisted = persistDemoData(); closeDeviceEditor(); route('home'); document.getElementById('deviceMenuButton').focus();
  showFeedback(persisted ? '设备名称、收藏与壁纸已保存到本地预览数据。' : '修改已在本次预览生效，但浏览器暂无法保存预览数据。', persisted ? 'success' : 'warning');
}
async function readEditorWallpaper(file) {
  if (!editorDraft || !file) return;
  const draft = editorDraft, request = ++editorImageRequest;
  const saveButton = document.querySelector('#deviceEditorForm button[type="submit"]');
  if (!['image/png', 'image/jpeg', 'image/webp'].includes(file.type) || file.size > 20 * 1024 * 1024) { saveButton.disabled = false; document.getElementById('editorFeedback').innerHTML = message('请选择 20 MB 以内的 PNG、JPG 或 WebP 图片。', 'warning'); return; }
  saveButton.disabled = true;
  try {
    const url = await new Promise((resolve, reject) => { const reader = new FileReader(); reader.onload = () => resolve(reader.result); reader.onerror = reject; reader.readAsDataURL(file); });
    const image = await new Promise((resolve, reject) => { const img = new Image(); img.onload = () => resolve(img); img.onerror = reject; img.src = url; });
    if (draft !== editorDraft || request !== editorImageRequest) return;
    const scale = Math.min(1, 1440 / image.width, 900 / image.height);
    const canvas = document.createElement('canvas'); canvas.width = Math.max(1, Math.round(image.width * scale)); canvas.height = Math.max(1, Math.round(image.height * scale));
    const ctx = canvas.getContext('2d'); ctx.fillStyle = '#fff'; ctx.fillRect(0, 0, canvas.width, canvas.height); ctx.drawImage(image, 0, 0, canvas.width, canvas.height);
    draft.wallpaper = canvas.toDataURL('image/jpeg', 0.86); draft.wallpaperName = file.name;
    updateEditorWallpaper(); document.getElementById('editorFeedback').innerHTML = message('壁纸预览已更新，点击保存后应用到此设备。', '');
  } catch { if (draft === editorDraft && request === editorImageRequest) document.getElementById('editorFeedback').innerHTML = message('无法读取该图片，请选择其他文件。', 'warning'); }
  finally { if (draft === editorDraft && request === editorImageRequest) saveButton.disabled = false; }
}
function demoConnectionTime() {
  const time = new Date(Date.parse('2026-11-21T10:00:00+08:00') + (history.length + 1) * 60000);
  return new Intl.DateTimeFormat('sv-SE', { timeZone: 'Asia/Shanghai', dateStyle: 'short', timeStyle: 'short' }).format(time);
}
function completeDemoCodeConnection(device) {
  device.credentialAvailable = true; device.credentialState = 'valid'; device.credentialRef = 'demo-credential-' + device.id;
  device.status = 'online'; device.lastConnectedAt = demoConnectionTime(); state.device = device;
  history.push({ deviceId: device.id, connectedAt: device.lastConnectedAt, method: '识别码', result: 'success' });
  persistDemoData(); route('home'); startSession({ authenticatedCode: true });
}
function connectByCode() {
  const input = document.getElementById('peerCode');
  const code = input.value.trim().replace(/\s+/g, '');
  if (!code || code.length > 64) { showFeedback('请输入有效的示例识别码。', 'warning'); return; }
  if (state.quota === 'zero') { document.getElementById('peerPassword').value = ''; route('account'); showFeedback('可用时长不足，前往官网签到后刷新重试。', 'warning'); return; }
  let device = devices.find(d => d.source === 'code' && d.peerCode === code);
  const passwordField = document.getElementById('peerPassword');
  if (!hasSavedCredential(device || {}) && !passwordField.value) { showFeedback('请输入首次连接或更新后的密码；成功后历史记录可直接重连。', 'warning'); return; }
  passwordField.value = '';
  if (!device) {
    let number = devices.length + 1; while (devices.some(d => d.id === 'code-' + number)) number++;
    device = { id: 'code-' + number, name: '识别码 ' + code.slice(0, 60), status: 'online', favorite: false, source: 'code', peerCode: code, mouseMode: 'office' };
    devices.splice(1, 0, device);
  }
  completeDemoCodeConnection(device);
}
function setMouseMode(mode) {
  const selected = mode === 'game' ? 'game' : 'office';
  state.device.mouseMode = selected; state.session.mouseMode = selected; state.preferences.absoluteMouse = selected === 'office';
  document.querySelectorAll('#mouseModes button').forEach(button => button.classList.toggle('active', button.dataset.mouseMode === selected));
  persistDemoData();
}

const remoteStage = document.getElementById('remoteStage');
const floatBall = document.getElementById('floatBall');
const floatPanel = document.getElementById('floatPanel');
let idleTimer, ballX = 0, ballY = 0, dockedEdge = null, drag = null, suppressBallClick = false;
const BALL_SIZE = 54;
function workArea() {
  if (state.session.mode === 'windowed') return document.querySelector('.remote-desktop').getBoundingClientRect();
  return { left: 0, top: 43, right: innerWidth, bottom: innerHeight, width: innerWidth, height: innerHeight - 43 };
}
function clamp(value, min, max) { return Math.min(Math.max(value, min), Math.max(min, max)); }
function placeBall() { floatBall.style.left = ballX + 'px'; floatBall.style.top = ballY + 'px'; if (!floatPanel.hidden) placePanel(); }
function placePanel() {
  const rect = floatPanel.getBoundingClientRect(), area = workArea();
  const preferred = ballX > (area.left + area.right) / 2 ? ballX - rect.width - 15 : ballX + BALL_SIZE + 15;
  floatPanel.style.left = clamp(preferred, area.left + 9, area.right - rect.width - 9) + 'px';
  floatPanel.style.top = clamp(ballY + BALL_SIZE / 2 - rect.height / 2, area.top + 9, area.bottom - rect.height - 9) + 'px';
}
function dockBall() {
  if (!state.session.active || !floatPanel.hidden || drag) return;
  const a = workArea();
  const distances = [['left', Math.abs(ballX - a.left)], ['right', Math.abs(a.right - ballX - BALL_SIZE)], ['top', Math.abs(ballY - a.top)], ['bottom', Math.abs(a.bottom - ballY - BALL_SIZE)]];
  distances.sort((first, second) => first[1] - second[1]); dockedEdge = distances[0][0];
  if (dockedEdge === 'left') ballX = a.left - BALL_SIZE / 2;
  if (dockedEdge === 'right') ballX = a.right - BALL_SIZE / 2;
  if (dockedEdge === 'top') ballY = a.top - BALL_SIZE / 2;
  if (dockedEdge === 'bottom') ballY = a.bottom - BALL_SIZE / 2;
  floatBall.classList.add('docked'); placeBall();
}
function undockBall() {
  clearTimeout(idleTimer); const a = workArea();
  ballX = clamp(ballX, a.left + 8, a.right - BALL_SIZE - 8); ballY = clamp(ballY, a.top + 8, a.bottom - BALL_SIZE - 8);
  dockedEdge = null; floatBall.classList.remove('docked'); placeBall();
}
function scheduleDock() { clearTimeout(idleTimer); if (state.session.active && floatPanel.hidden && !drag) idleTimer = setTimeout(dockBall, 3000); }
function openFloatPanel() { undockBall(); floatPanel.hidden = false; floatBall.setAttribute('aria-expanded', 'true'); placePanel(); }
function closeFloatPanel() { floatPanel.hidden = true; floatBall.setAttribute('aria-expanded', 'false'); scheduleDock(); }
function updateSummary() { document.getElementById('sessionSummary').textContent = state.session.resolution.replace('x', ' × ') + ' · ' + state.session.bitrate + ' Mbps · ' + state.session.fps + ' FPS'; }
function startSession(options = {}) {
  if (state.requiresSetup) { route('setup'); return; }
  if (state.quota === 'zero') { route('account'); showFeedback('皎月连可用时长不足，请前往官网签到，返回后刷新重试。', 'warning'); return; }
  if (state.device.source === 'code' && !options.authenticatedCode) {
    if (hasSavedCredential(state.device)) { completeDemoCodeConnection(state.device); return; }
    const expired = state.device.credentialState === 'expired';
    route('assist', { mode: 'other' }); showFeedback(expired ? '保存的授权已失效，请更新连接密码；成功后继续直接重连。' : '本机保存的连接凭据不可用，请认证一次；成功后继续直接重连。', 'warning'); return;
  }
  if (state.device.status !== 'online') { route('diagnostics', { reason: state.device.status === 'offline' ? '示例设备当前离线，请检查被控电脑和皎月连服务。' : '示例设备网络在线，但 Sunshine 尚未就绪。' }); return; }
  state.session.active = true; state.session.mode = 'fullscreen'; state.session.monitoring = false;
  setMouseMode(state.device.mouseMode || 'office');
  document.getElementById('appShell').hidden = true; remoteStage.hidden = false; remoteStage.dataset.mode = 'fullscreen';
  document.getElementById('remoteDevice').textContent = state.device.name;
  document.getElementById('remoteModeLabel').textContent = '全屏 · 示例远控画面';
  document.getElementById('sessionResolution').value = state.session.resolution;
  document.getElementById('sessionBitrate').value = state.session.bitrate;
  document.getElementById('sessionFps').value = state.session.fps;
  document.getElementById('performancePanel').hidden = true;
  document.getElementById('monitorToggle').classList.remove('on'); document.getElementById('monitorToggle').setAttribute('aria-checked', 'false');
  document.querySelectorAll('#windowModes button').forEach(b => b.classList.toggle('active', b.dataset.mode === 'fullscreen'));
  floatPanel.hidden = true; dockedEdge = null; floatBall.classList.remove('docked');
  ballX = innerWidth - BALL_SIZE - 20; ballY = innerHeight * 0.55 - BALL_SIZE / 2;
  updateSummary(); placeBall(); scheduleDock();
}
function endSession() {
  state.session.active = false; clearTimeout(idleTimer); floatPanel.hidden = true; remoteStage.hidden = true;
  if (state.device.source === 'code') state.device.status = 'unknown';
  document.getElementById('appShell').hidden = false; route('home');
  showFeedback('示例会话已断开。正式软件仅结束本次连接，默认保留远端应用。');
}
function applySessionSetting(kind) {
  let label;
  if (kind === 'resolution') {
    let value = document.getElementById('sessionResolution').value;
    if (value === 'custom') {
      const width = Number(document.getElementById('customWidth').value), height = Number(document.getElementById('customHeight').value);
      if (!Number.isInteger(width) || !Number.isInteger(height) || width < 320 || height < 200) { document.getElementById('sessionHint').textContent = '请输入有效的整数宽高。'; return; }
      value = width + 'x' + height;
    }
    state.session.resolution = value; label = '分辨率';
  } else if (kind === 'bitrate') {
    const value = Number(document.getElementById('sessionBitrate').value);
    if (!Number.isFinite(value) || value < 1 || value > 500) { document.getElementById('sessionHint').textContent = '请输入 1 至 500 Mbps 范围内的码率。'; return; }
    state.session.bitrate = value; label = '码率';
  } else if (kind === 'fps') {
    const value = Number(document.getElementById('sessionFps').value);
    if (!Number.isInteger(value) || value < 1 || value > 360) { document.getElementById('sessionHint').textContent = '请输入有效的整数帧率。'; return; }
    state.session.fps = value; label = '帧率';
  }
  updateSummary(); document.getElementById('sessionHint').textContent = '示例：' + label + '已单独保存，其他两项不变。实际内核需要时将短暂重连。'; placePanel();
}
function sessionAction(action, element) {
  if (action === 'close-panel') closeFloatPanel();
  else if (action === 'disconnect') endSession();
  else if (action.startsWith('apply-')) applySessionSetting(action.slice(6));
  else if (action === 'mouse-mode') { setMouseMode(element.dataset.mouseMode); document.getElementById('sessionHint').textContent = '示例：已切换为' + (state.session.mouseMode === 'office' ? '办公鼠标（绝对坐标）' : '游戏鼠标（相对坐标）') + '，画质、窗口和音频保持原值。'; }
  else if (action === 'mode') {
    state.session.mode = element.dataset.mode; remoteStage.dataset.mode = state.session.mode;
    document.getElementById('remoteModeLabel').textContent = { fullscreen: '全屏', windowed: '窗口', borderless: '无边框' }[state.session.mode] + ' · 示例远控画面';
    document.querySelectorAll('#windowModes button').forEach(b => b.classList.toggle('active', b === element));
    undockBall(); setTimeout(placePanel, 190);
  } else if (action === 'mute') {
    state.session.muted = !state.session.muted; document.getElementById('muteButton').textContent = state.session.muted ? '取消静音' : '静音';
  } else if (action === 'monitor') {
    state.session.monitoring = !state.session.monitoring; document.getElementById('performancePanel').hidden = !state.session.monitoring;
    element.classList.toggle('on', state.session.monitoring); element.setAttribute('aria-checked', String(state.session.monitoring));
  }
}
floatBall.addEventListener('click', () => { if (suppressBallClick) { suppressBallClick = false; return; } if (floatPanel.hidden) openFloatPanel(); else closeFloatPanel(); });
floatBall.addEventListener('pointerenter', () => { if (state.session.active && !drag) undockBall(); });
floatBall.addEventListener('pointerleave', scheduleDock);
floatBall.addEventListener('pointerdown', event => {
  if (event.button !== 0) return; undockBall();
  drag = { startX: event.clientX, startY: event.clientY, oldX: ballX, oldY: ballY, moved: false };
  floatBall.setPointerCapture(event.pointerId);
});
floatBall.addEventListener('pointermove', event => {
  if (!drag) return; const dx = event.clientX - drag.startX, dy = event.clientY - drag.startY;
  if (Math.abs(dx) + Math.abs(dy) > 5) drag.moved = true;
  if (drag.moved) { const a = workArea(); floatBall.classList.add('dragging'); ballX = clamp(drag.oldX + dx, a.left, a.right - BALL_SIZE); ballY = clamp(drag.oldY + dy, a.top, a.bottom - BALL_SIZE); placeBall(); }
});
function finishDrag() { if (!drag) return; suppressBallClick = drag.moved; drag = null; floatBall.classList.remove('dragging'); scheduleDock(); }
floatBall.addEventListener('pointerup', finishDrag); floatBall.addEventListener('pointercancel', finishDrag);
remoteStage.addEventListener('click', event => { if (!event.target.closest('#floatBall, #floatPanel, #performancePanel') && !floatPanel.hidden) closeFloatPanel(); });
addEventListener('resize', () => { if (state.session.active) { undockBall(); scheduleDock(); } });

document.addEventListener('click', event => {
  const target = event.target;
  const editorAction = target.closest('[data-editor]');
  if (editorAction) {
    if (editorAction.dataset.editor === 'cancel') closeDeviceEditor();
    if (editorAction.dataset.editor === 'choose-wallpaper') document.getElementById('editorWallpaperInput').click();
    if (editorAction.dataset.editor === 'default-wallpaper' && editorDraft) { editorImageRequest++; editorDraft.wallpaper = ''; editorDraft.wallpaperName = ''; document.querySelector('#deviceEditorForm button[type="submit"]').disabled = false; updateEditorWallpaper(); }
    return;
  }
  const sessionElement = target.closest('[data-session]'); if (sessionElement) { sessionAction(sessionElement.dataset.session, sessionElement); return; }
  const device = target.closest('[data-device]'); if (device) { state.device = devices.find(d => d.id === device.dataset.device) || devices[0]; route('home'); return; }
  const view = target.closest('[data-view]'); if (view) { route(view.dataset.view); return; }
  const advancedElement = target.closest('[data-advanced]');
  if (advancedElement) {
    const targets = { jiaoyue: '随包皎月连原主界面', moonlight: '随包 Moonlight 原设置界面', sunshine: 'Sunshine 配置界面 /config，默认简体中文' };
    state.lastAdvanced = advancedElement.dataset.advanced;
    showFeedback('跳转演示：' + targets[state.lastAdvanced] + '。此草图不启动本机原软件。', ''); return;
  }
  const toggleElement = target.closest('[data-toggle]');
  if (toggleElement) { const key = toggleElement.dataset.toggle; state.preferences[key] = !state.preferences[key]; toggleElement.classList.toggle('on', state.preferences[key]); toggleElement.setAttribute('aria-checked', String(state.preferences[key])); return; }
  const setupMode = target.closest('[data-setup-mode]');
  if (setupMode) { document.querySelectorAll('[data-setup-mode]').forEach(b => b.classList.toggle('active', b === setupMode)); const existing = setupMode.dataset.setupMode === 'existing'; document.getElementById('confirmPasswordRow').hidden = existing; document.getElementById('setupConfirm').required = !existing; document.querySelector('#setupForm .primary').textContent = existing ? '验证并继续' : '保存并继续'; return; }
  const assistMode = target.closest('[data-assist-mode]'); if (assistMode) { route('assist', { mode: assistMode.dataset.assistMode }); return; }
  const action = target.closest('[data-action]'); if (!action) return;
  switch (action.dataset.action) {
    case 'connect': startSession(); break;
    case 'input-settings': route('moonlight'); document.getElementById('inputSection').scrollIntoView({ block: 'center' }); break;
    case 'save': if (state.view === 'moonlight') setMouseMode(document.getElementById('moonMouseMode').value); showFeedback('示例设置已更新。此草图未修改 Windows 或原软件配置。'); break;
    case 'edit-device': openDeviceEditor(); break;
    case 'sun-service': state.sunshineRunning = !state.sunshineRunning; route('sunshine'); showFeedback('示例服务状态已切换，未启动本机 Sunshine。'); break;
    case 'pin': {
      const input = document.getElementById('sunPin');
      if (!/^\d{4}$/.test(input.value)) showFeedback('请输入四位数字 PIN。', 'warning');
      else { input.value = ''; showFeedback('PIN 确认入口演示：实际由本机管理凭据提交至 Sunshine。'); }
      break;
    }
    case 'refresh-quota': showFeedback(state.quota === 'unknown' ? '当前草图未读取账号余额；正式软件刷新真实接口，可通过官网查询。' : '这是示例时长，正式软件会刷新真实数据。', ''); break;
    case 'logout': route('login'); break;
    case 'demo-login': state.requiresSetup = false; state.setupDone = true; route('home'); break;
    case 'assist-connect': connectByCode(); break;
    case 'mock': showFeedback('此操作为界面演示，不执行真实账号或系统操作。', ''); break;
    case 'window': toast('这是浏览器中的方案草图，窗口按钮用于展示布局。'); break;
  }
});
document.addEventListener('submit', event => {
  if (event.target.id === 'deviceEditorForm') { event.preventDefault(); saveDeviceEditor(); return; }
  if (event.target.id !== 'setupForm') return; event.preventDefault();
  const user = document.getElementById('setupUser'), password = document.getElementById('setupPassword'), confirm = document.getElementById('setupConfirm');
  if (!user.value.trim() || !password.value) { document.getElementById('setupFeedback').innerHTML = message('请填写用户名与密码。', 'warning'); return; }
  if (!document.getElementById('confirmPasswordRow').hidden && password.value !== confirm.value) { document.getElementById('setupFeedback').innerHTML = message('两次密码不一致。', 'warning'); return; }
  state.username = user.value.trim(); password.value = ''; confirm.value = ''; state.requiresSetup = false; state.setupDone = true;
  route('login'); showFeedback('首次配置步骤演示完成；未保存或发送任何密码。');
});
document.addEventListener('input', event => {
  if (event.target.id === 'deviceSearch') { const text = event.target.value.trim().toLowerCase(); document.querySelectorAll('[data-search]').forEach(row => row.hidden = !row.dataset.search.includes(text)); }
  if (event.target.id === 'sessionVolume') { state.session.volume = Number(event.target.value); document.getElementById('volumeValue').textContent = state.session.volume + '%'; }
});
document.getElementById('sessionResolution').addEventListener('change', event => { document.getElementById('customResolution').hidden = event.target.value !== 'custom'; if (!floatPanel.hidden) placePanel(); });
document.getElementById('editorWallpaperInput').addEventListener('change', event => { readEditorWallpaper(event.target.files[0]); });
editorBackdrop.addEventListener('click', event => { if (event.target === editorBackdrop) closeDeviceEditor(); });
document.addEventListener('keydown', event => {
  if (!editorBackdrop.hidden) {
    if (event.key === 'Escape') { event.preventDefault(); closeDeviceEditor(); return; }
    if (event.key === 'Tab') {
      const items = Array.from(editorBackdrop.querySelectorAll('button,input,[tabindex="0"]')).filter(el => !el.disabled && el.getClientRects().length);
      const first = items[0], last = items[items.length - 1];
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
    }
    return;
  }
  if ((event.key === 'Enter' || event.key === ' ') && event.target.classList.contains('cover')) { event.preventDefault(); startSession(); }
  if (event.key === 'Escape' && state.session.active && !floatPanel.hidden) closeFloatPanel();
});

window.LLPlanPreview = {
  route,
  startSession,
  openFloatPanel,
  dockBall,
  openDeviceEditor,
  invalidateCredential(deviceId) { const device = devices.find(d => d.id === deviceId); if (!device) return; device.credentialAvailable = false; device.credentialState = 'expired'; device.credentialRef = ''; persistDemoData(); if (state.view === 'history') route('history'); },
  resetDemo() { if (state.session.active) endSession(); closeDeviceEditor(); devices.splice(0, devices.length, ...JSON.parse(JSON.stringify(defaultDevices))); history.splice(0, history.length, ...JSON.parse(JSON.stringify(defaultHistory))); state.device = devices[0]; persistDemoData(); route('home'); },
  setQuotaStatus(status, minutes = 750) { state.quota = status; state.quotaMinutes = minutes; if (state.view === 'account') route('account'); },
  getState() { return { view: state.view, setupDone: state.setupDone, requiresSetup: state.requiresSetup, deviceId: state.device.id, session: { ...state.session }, dockedEdge, ball: { x: ballX, y: ballY }, lastAdvanced: state.lastAdvanced, editorOpen: !editorBackdrop.hidden, devices: devices.map(d => ({ id: d.id, name: d.name, status: d.status, source: d.source || 'account', peerCode: d.peerCode || '', favorite: Boolean(d.favorite), mouseMode: d.mouseMode || 'office', wallpaper: Boolean(d.wallpaper), lastConnectedAt: d.lastConnectedAt || '', credentialAvailable: hasSavedCredential(d), credentialState: d.credentialState || 'missing' })), history: history.map(h => ({ ...h })) }; }
};
const requestedView = new URLSearchParams(location.search).get('view') || 'home';
const requestedDevice = new URLSearchParams(location.search).get('device');
if (requestedDevice) state.device = devices.find(d => d.id === requestedDevice) || devices[0];
else if (requestedView === 'history') state.device = devices.find(d => d.source === 'code') || devices[0];
state.requiresSetup = requestedView === 'setup'; state.setupDone = !state.requiresSetup;
route(requestedView === 'session' ? 'home' : requestedView);
if (requestedView === 'session') { startSession(); if (new URLSearchParams(location.search).get('panel') === '1') openFloatPanel(); }
