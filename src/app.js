window.addEventListener('error', (e) => {
  const t = document.getElementById('toast');
  if (t) { t.textContent = `JS 오류: ${e.message} (${(e.filename || '').split('/').pop()}:${e.lineno})`; t.hidden = false; }
});
window.addEventListener('unhandledrejection', (e) => {
  const t = document.getElementById('toast');
  if (t) { t.textContent = `오류: ${e.reason?.message || e.reason}`; t.hidden = false; }
});
const XI = '<svg viewBox="0 0 24 24"><path d="M6 6l12 12M18 6L6 18"/></svg>';
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $ = (s, r = document) => r.querySelector(s);
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

let S = null;                 // 백엔드 스냅샷
let cur = { acc: null, sub: 0 };   // acc: 선택한 서비스 항목 id, sub: 0=서비스 화면 / -1=로그인 화면
let modalOpen = 0;
let dlOpen = false;

const ACCENTS = ['#3182F6', '#7C3AED', '#DB2777', '#DC2626', '#EA580C', '#CA8A04', '#16A34A', '#0D9488', '#0891B2', '#475569'];
const providerColor = (key) => (S && S.recipes.find((r) => r.key === key)?.color) || '#475569';
const providerName = (key) => (S && S.recipes.find((r) => r.key === key)?.name) || '직접 입력';

async function call(cmd, args = {}) {
  try { return await invoke(cmd, args); }
  catch (e) { toast(typeof e === 'string' ? e : (e?.message || '오류가 발생했습니다')); throw e; }
}
let toastTimer;
function toast(msg) {
  const t = $('#toast'); t.textContent = msg; t.hidden = false;
  clearTimeout(toastTimer); toastTimer = setTimeout(() => (t.hidden = true), 3200);
}

// ───────── 오버레이 / 레이아웃 ─────────
function syncOverlay() { invoke('set_overlay', { on: modalOpen > 0 || S?.locked || false }); }
function reportRect() {
  const r = $('#content').getBoundingClientRect();
  invoke('set_rect', { x: r.left, y: r.top, w: r.width, h: r.height });
}
new ResizeObserver(reportRect).observe($('#content'));
window.addEventListener('resize', reportRect);

function applySettings() {
  const s = S.settings;
  const dark = s.theme === 'dark' || (s.theme === 'system' && matchMedia('(prefers-color-scheme: dark)').matches);
  document.documentElement.dataset.theme = dark ? 'dark' : 'light';
  document.documentElement.style.setProperty('--accent', s.accent);
  $('#app').className = (s.layout === 'top' ? 'layout-top' : 'layout-side') + (s.colorful ? ' colorful' : '');
  const acc = S.accounts.find((a) => a.id === cur.acc);
  document.documentElement.style.setProperty('--acc', acc?.accent || s.accent);
  requestAnimationFrame(reportRect);
}
matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => S && applySettings());

// ───────── 렌더링 ─────────
function accColor(a) { return a.accent || providerColor(a.service); }
function svcName(a) { return a.subs[0]?.name || a.name; }
// 서비스 아이콘: service-icons.js(자동 생성)에 있으면 실제 로고, 없으면 글자
const iconSlug = (s) => String(s).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
function serviceIcon(provider, name) { return (window.SERVICE_ICONS || {})[`${provider}-${iconSlug(name)}`] || null; }
/** 아이콘 타일 안쪽 HTML(이미지). 로드 실패 시 글자로 대체 */
function iconInner(ic, letter) {
  return `<img src="${esc(ic.src)}" alt="" draggable="false" data-l="${esc(letter)}" onerror="this.replaceWith(document.createTextNode(this.dataset.l))">`;
}
/** 클래스/배경을 포함한 타일 속성 */
function iconTile(ic, cls) {
  return ic.kind === 'glyph' ? { cls: `${cls} ico glyph`, style: `background:${ic.bg}` } : { cls: `${cls} ico color`, style: '' };
}
function thumbHtml(a) {
  if (a.thumb === 'picture' && a.picture) return `<div class="thumb" style="background-image:url('${esc(a.picture)}')"></div>`;
  if (a.thumb === 'logo') {
    const ic = serviceIcon(a.service, svcName(a));
    if (ic) { const t = iconTile(ic, 'thumb'); return `<div class="${t.cls}" style="${t.style}">${iconInner(ic, svcName(a).slice(0, 1).toUpperCase())}</div>`; }
  }
  const txt = a.thumb === 'logo' ? svcName(a).slice(0, 1).toUpperCase() : (a.initials || a.name.slice(0, 2));
  return `<div class="thumb" style="background:${accColor(a)}">${esc(txt)}</div>`;
}
function badgeOf(id) { return (S.badges.accounts[id] || 0); }

function render() {
  applySettings();
  const box = $('#accounts'); box.innerHTML = '';
  S.accounts.forEach((a) => {
    const b = document.createElement('button');
    b.className = 'acc' + (a.id === cur.acc ? ' active' : '');
    b.title = `${a.name}\n${providerName(a.service)} · ${svcName(a)}` + (a.handle ? `\n${a.handle}` : '');
    const n = a.badges && S.settings.badges ? badgeOf(a.id) : 0;
    b.innerHTML = thumbHtml(a) + (n > 0 ? `<span class="badge">${n > 99 ? '99+' : n}</span>` : '');
    b.onclick = () => selectAccount(a.id);
    b.oncontextmenu = (e) => { e.preventDefault(); accountSettings(a.id); };
    box.appendChild(b);
  });
  renderTitle();
  const has = S.accounts.length > 0;
  $('#empty').hidden = has && cur.acc !== null;
  $('#empty').textContent = has ? '' : '서비스를 추가해 시작하세요';
  $('#dl-dot').hidden = !S.downloads.some((d) => d.state === 'progress');
  $('#lock').hidden = !S.locked;
  $('#btn-lock').style.display = S.has_passcode ? '' : 'none';
  const acc = S.accounts.find((a) => a.id === cur.acc);
  $('#loginbar').hidden = !(acc && cur.sub < 0);
  if (acc && cur.sub < 0) $('#login-text').textContent = `${acc.name}: 아래에서 로그인하세요. 완료되면 자동으로 이동해요.`;
}

function renderTitle() {
  const box = $('#tabs'); box.innerHTML = '';
  const acc = S.accounts.find((a) => a.id === cur.acc);
  if (!acc) return;
  const n = acc.badges && S.settings.badges ? badgeOf(acc.id) : 0;
  const sub = [providerName(acc.service) === svcName(acc) ? '' : providerName(acc.service), acc.handle].filter(Boolean).join(' · ');
  const ic = acc.thumb === 'logo' ? serviceIcon(acc.service, svcName(acc)) : null;
  const ico = ic ? (() => { const t = iconTile(ic, 'svc-ico'); return `<div class="${t.cls}" style="${t.style}">${iconInner(ic, '')}</div>`; })() : '';
  box.innerHTML = `<div class="svc-title">${ico}<b>${esc(acc.name)}</b>${n > 0 ? `<span class="badge">${n > 99 ? '99+' : n}</span>` : ''}${sub ? `<span class="svc-sub">${esc(sub)}</span>` : ''}</div>`;
}

// ───────── 선택 ─────────
async function selectAccount(id) {
  const acc = S.accounts.find((a) => a.id === id);
  if (!acc) return;
  cur.acc = id;
  cur.sub = acc.logged_in ? 0 : -1;
  render();
  await call('show_view', { accountId: id, sub: cur.sub, url: null });
}

// ───────── 모달 ─────────
let modalCleanup = null;
function modal(html, onMount) {
  const m = $('#modal'); const wasOpen = !m.hidden;
  m.innerHTML = html; m.hidden = false;
  if (!wasOpen) { modalOpen++; syncOverlay(); }
  m.onmousedown = (e) => { if (e.target === m) closeModal(); };
  m.querySelectorAll('[data-close]').forEach((b) => (b.onclick = closeModal));
  onMount && onMount(m);
}
function closeModal() {
  const m = $('#modal'); if (m.hidden) return;
  m.hidden = true; m.innerHTML = ''; modalOpen = Math.max(0, modalOpen - 1); dlOpen = false; syncOverlay();
}
const sw = (id, on) => `<label class="switch"><input type="checkbox" id="${id}" ${on ? 'checked' : ''}><span></span></label>`;

// ───────── 서비스 추가 ─────────
function addAccountDialog() {
  let pick = null;   // { provider, service }
  const groups = [...S.recipes.map((r) => ({ key: r.key, name: r.name, color: r.color, services: r.services.map((x) => x.name) })),
    { key: 'custom', name: '직접 입력', color: '#475569', services: ['웹 주소로 추가'] }];
  const sessionsFor = (provider) => {
    const m = new Map();
    S.accounts.filter((a) => a.service === provider).forEach((a) => {
      const k = a.profile || a.id; m.set(k, [...(m.get(k) || []), a.name]);
    });
    return [...m.entries()];
  };
  const cardsHtml = (q) => groups.map((g) => {
    const items = g.services.filter((n) => !q || (n + ' ' + g.name).toLowerCase().includes(q));
    if (!items.length) return '';
    return `<div class="grp"><div class="grp-name">${esc(g.name)}</div><div class="grid">${items.map((n) =>
      `<button class="card" data-p="${g.key}" data-s="${esc(n)}">${(() => {
        const ic = g.key === 'custom' ? null : serviceIcon(g.key, n); const L = esc(n.slice(0, 1).toUpperCase());
        if (!ic) return `<div class="logo" style="background:${g.color}">${L}</div>`;
        const t = iconTile(ic, 'logo'); return `<div class="${t.cls}" style="${t.style}">${iconInner(ic, L)}</div>`;
      })()}<b>${esc(n)}</b></button>`).join('')}</div></div>`;
  }).join('') || '<div class="muted" style="padding:24px 0">검색 결과가 없어요</div>';

  modal(`<div class="dialog"><header>서비스 추가<button class="icon" data-close>${XI}</button></header>
    <div class="body">
      <input type="text" id="add-q" placeholder="서비스 검색 (예: Gmail, Slack)" style="margin-bottom:6px">
      <div id="add-cards">${cardsHtml('')}</div>
      <div id="add-form" hidden style="margin-top:18px">
        <div class="field"><label>이름</label><input type="text" id="add-name"></div>
        <div class="field" id="add-url-f" hidden><label>웹 주소</label><input type="url" id="add-url" placeholder="https://"></div>
        <div class="field" id="add-sess-f"><label>로그인</label><select id="add-sess"></select>
          <div class="muted" style="font-size:12px;margin-top:6px">같은 로그인을 고르면 다시 로그인하지 않아도 돼요. 계정이 다르면 새로 로그인하세요.</div></div>
      </div></div>
    <footer><button data-close>취소</button><button class="primary" id="add-ok" disabled>추가</button></footer></div>`, (root) => {
    const bindCards = () => root.querySelectorAll('#add-cards .card').forEach((el) => (el.onclick = () => {
      pick = { provider: el.dataset.p, service: el.dataset.s };
      root.querySelectorAll('#add-cards .card').forEach((c) => c.classList.toggle('sel', c === el));
      $('#add-form').hidden = false;
      $('#add-url-f').hidden = pick.provider !== 'custom';
      const dup = S.accounts.filter((x) => x.name === pick.service || x.name.startsWith(pick.service + ' ')).length;
      $('#add-name').value = pick.provider === 'custom' ? '' : (dup ? `${pick.service} ${dup + 1}` : pick.service);
      const ss = pick.provider === 'custom' ? [] : sessionsFor(pick.provider);
      $('#add-sess-f').hidden = pick.provider === 'custom';
      const label = (names) => names.slice(0, 2).join(', ') + (names.length > 2 ? ` 외 ${names.length - 2}개` : '');
      $('#add-sess').innerHTML = '<option value="">새로 로그인</option>' + ss.map(([k, names]) =>
        `<option value="${esc(k)}">${esc(label(names))}의 로그인 공유</option>`).join('');
      $('#add-ok').disabled = false; $('#add-name').focus();
      $('#add-form').scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    }));
    bindCards();
    $('#add-q').oninput = (e) => { $('#add-cards').innerHTML = cardsHtml(e.target.value.trim().toLowerCase()); bindCards(); };
    $('#add-q').focus();
    $('#add-ok').onclick = async () => {
      const name = $('#add-name').value.trim() || pick.service;
      const r = await call('add_account', {
        service: pick.provider, serviceName: pick.provider === 'custom' ? null : pick.service, name,
        customUrl: pick.provider === 'custom' ? $('#add-url').value.trim() : null,
        profile: pick.provider === 'custom' ? null : ($('#add-sess').value || null),
      });
      S = r.state; closeModal(); await selectAccount(r.id);
    };
  });
}

// ───────── 계정 설정 ─────────
function accountSettings(id) {
  const a = S.accounts.find((x) => x.id === id); if (!a) return;
  const idx = S.accounts.findIndex((x) => x.id === id);
  let picture = a.picture;
  modal(`<div class="dialog"><header>서비스 설정 — ${esc(a.name)}<button class="icon" data-close>${XI}</button></header>
    <div class="body">
      <div class="field"><label>이름</label><input type="text" id="as-name" value="${esc(a.name)}"></div>
      <div class="field"><label>이메일/메모</label><input type="text" id="as-handle" value="${esc(a.handle)}"></div>
      <div class="field"><label>썸네일</label>
        <div class="inline"><select id="as-thumb" style="width:150px"><option value="initials">이니셜</option><option value="logo">서비스 로고</option><option value="picture">사진</option></select>
        <input type="text" id="as-init" value="${esc(a.initials)}" maxlength="3" style="width:80px" placeholder="이니셜">
        <input type="file" id="as-file" accept="image/*" style="display:none"><button id="as-pick">사진 선택</button></div></div>
      <div class="field"><label>강조 색상</label><div class="swatches" id="as-colors">${['', ...ACCENTS].map((c) =>
        `<button class="swatch ${(a.accent || '') === c ? 'sel' : ''}" data-c="${c}" style="background:${c || 'var(--border)'}" title="${c || '기본'}"></button>`).join('')}</div></div>
      <div class="row"><div>알림</div>${sw('as-notif', a.notifications)}</div>
      <div class="row"><div>배지(읽지 않은 수)</div>${sw('as-badges', a.badges)}</div>
      <div class="field" style="margin-top:12px"><label>로그인 세션</label>
        <div class="muted" style="font-size:13px">${esc(providerName(a.service))} · ${esc(svcName(a))}${(() => {
          const mates = S.accounts.filter((x) => x.id !== a.id && (x.profile || x.id) === (a.profile || a.id)).map((x) => x.name);
          return mates.length ? `<br>같은 로그인을 쓰는 서비스: ${esc(mates.join(', '))}` : '<br>이 서비스만 쓰는 로그인이에요';
        })()}</div></div>
    </div>
    <footer><button class="danger" id="as-del" style="margin-right:auto">삭제</button>
      <button id="as-up" ${idx === 0 ? 'disabled' : ''}>↑</button><button id="as-down" ${idx === S.accounts.length - 1 ? 'disabled' : ''}>↓</button>
      <button id="as-relogin">다시 로그인</button><button class="primary" id="as-save">저장</button></footer></div>`, (root) => {
    $('#as-thumb').value = a.thumb;
    let accent = a.accent || '';
    root.querySelectorAll('#as-colors .swatch').forEach((b) => (b.onclick = () => {
      accent = b.dataset.c; root.querySelectorAll('.swatch').forEach((x) => x.classList.toggle('sel', x === b));
    }));
    $('#as-pick').onclick = () => $('#as-file').click();
    $('#as-file').onchange = (e) => {
      const f = e.target.files[0]; if (!f) return;
      const img = new Image(); const url = URL.createObjectURL(f);
      img.onload = () => {
        const c = document.createElement('canvas'); c.width = c.height = 128;
        const s = Math.min(img.width, img.height);
        c.getContext('2d').drawImage(img, (img.width - s) / 2, (img.height - s) / 2, s, s, 0, 0, 128, 128);
        picture = c.toDataURL('image/png'); $('#as-thumb').value = 'picture'; URL.revokeObjectURL(url);
      };
      img.src = url;
    };
    $('#as-save').onclick = async () => {
      S = await call('update_account', { id, patch: {
        name: $('#as-name').value.trim() || a.name, handle: $('#as-handle').value.trim(),
        initials: $('#as-init').value.trim(), thumb: $('#as-thumb').value, picture, accent: accent || null,
        notifications: $('#as-notif').checked, badges: $('#as-badges').checked,
      } });
      closeModal(); render();
    };
    const move = async (d) => {
      const ids = S.accounts.map((x) => x.id); const i = ids.indexOf(id);
      [ids[i], ids[i + d]] = [ids[i + d], ids[i]];
      S = await call('reorder_accounts', { ids }); closeModal(); render();
    };
    $('#as-up').onclick = () => move(-1); $('#as-down').onclick = () => move(1);
    $('#as-relogin').onclick = async () => {
      S = await call('update_account', { id, patch: { logged_in: false } }); closeModal(); selectAccount(id);
    };
    $('#as-del').onclick = () => {
      if (!confirm(`'${a.name}' 서비스를 삭제할까요? 다른 서비스가 쓰지 않는 로그인 정보는 함께 지워져요.`)) return;
      call('remove_account', { id }).then(async (st) => {
        S = st; closeModal();
        if (cur.acc === id) { cur.acc = null; S.accounts[0] ? await selectAccount(S.accounts[0].id) : render(); } else render();
      });
    };
  });
}

// ───────── 설정 ─────────
const SECTIONS = [['general', '일반'], ['notif', '알림'], ['look', '개인화'], ['security', '보안'], ['dl', '다운로드'], ['about', '정보']];
function settingsDialog(section = 'general') {
  modal(`<div class="dialog" style="width:min(860px,94vw)"><header>설정<button class="icon" data-close>${XI}</button></header>
    <div class="split"><nav>${SECTIONS.map(([k, n]) => `<button data-sec="${k}" class="${k === section ? 'active' : ''}">${n}</button>`).join('')}</nav><div class="pane" id="pane"></div></div></div>`, (root) => {
    root.querySelectorAll('[data-sec]').forEach((b) => (b.onclick = () => settingsDialog(b.dataset.sec)));
    paintPane(section);
  });
}
async function patch(p) { S = await call('set_settings', { patch: p }); applySettings(); render(); }
function bindSwitch(id, key) { $('#' + id).onchange = (e) => patch({ [key]: e.target.checked }); }
function bindSelect(id, key) { $('#' + id).onchange = (e) => patch({ [key]: e.target.value }); }

function paintPane(sec) {
  const s = S.settings; const p = $('#pane');
  if (sec === 'general') {
    p.innerHTML = `
      <div class="row"><div>Windows 시작 시 실행<div class="sub">로그인할 때 자동으로 시작</div></div><select class="ctl" id="g-startup"><option value="disabled">사용 안 함</option><option value="tray">트레이로 시작</option><option value="visible">창 표시</option></select></div>
      <div class="row"><div>닫기 버튼 → 트레이로 이동</div>${sw('g-close', s.close_to_tray)}</div>
      <div class="row"><div>최소화 → 트레이로 이동</div>${sw('g-min', s.minimize_to_tray)}</div>
      <div class="row"><div>전역 단축키 (창 열기/숨기기)<div class="sub">예: Ctrl+Alt+M, 비우면 사용 안 함</div></div><input class="ctl" type="text" id="g-short" value="${esc(s.shortcut)}"></div>
      <div class="row"><div>mailto: 링크를 열 메일 서비스<div class="sub">기본값: 새 메일을 지원하는 첫 번째 메일 서비스. 기본 메일 앱 등록은 README 참고</div></div>
        <select class="ctl" id="g-mailto"><option value="">자동</option>${S.accounts.filter((a) => a.subs[0]?.is_mail).map((a) => `<option value="${a.id}" ${s.mailto_account === a.id ? 'selected' : ''}>${esc(a.name)}</option>`).join('')}</select></div>`;
    $('#g-startup').value = s.startup; bindSelect('g-startup', 'startup');
    bindSwitch('g-close', 'close_to_tray'); bindSwitch('g-min', 'minimize_to_tray');
    $('#g-short').onchange = (e) => patch({ shortcut: e.target.value.trim() });
    $('#g-mailto').onchange = (e) => patch({ mailto_account: e.target.value || null });
  } else if (sec === 'notif') {
    const cats = [['primary', '기본'], ['social', '소셜'], ['promotions', '프로모션'], ['updates', '업데이트'], ['forums', '포럼']];
    p.innerHTML = `
      <div class="row"><div>알림 사용</div>${sw('n-on', s.notifications)}</div>
      <div class="row"><div>알림 테스트<div class="sub">토스트가 보이지 않으면 Windows 설정 > 시스템 > 알림에서 Dock과 방해 금지 모드를 확인하세요</div></div><button id="n-test">테스트 알림 보내기</button></div>
      <div class="row"><div>배지(작업표시줄·서비스 목록) 사용</div>${sw('n-badge', s.badges)}</div>
      <div class="row"><div>여러 통이면 요약 알림 1개로<div class="sub">한 번에 4통 이상 도착하면 요약</div></div>${sw('n-sum', s.summary)}</div>
      <div style="margin-top:12px"><b>Gmail 알림 카테고리</b><div class="sub muted">받은편지함 탭(카테고리)을 켜 둔 경우에만 구분됩니다</div></div>
      ${cats.map(([k, n]) => `<div class="row"><div>${n}</div>${sw('c-' + k, s.categories.includes(k))}</div>`).join('')}`;
    $('#n-test').onclick = () => call('test_notification');
    bindSwitch('n-on', 'notifications'); bindSwitch('n-badge', 'badges'); bindSwitch('n-sum', 'summary');
    cats.forEach(([k]) => ($('#c-' + k).onchange = (e) => {
      const set = new Set(S.settings.categories); e.target.checked ? set.add(k) : set.delete(k); patch({ categories: [...set] });
    }));
  } else if (sec === 'look') {
    p.innerHTML = `
      <div class="row"><div>테마</div><select class="ctl" id="l-theme"><option value="system">시스템</option><option value="light">라이트</option><option value="dark">다크</option></select></div>
      <div class="row"><div>서비스 목록 위치</div><select class="ctl" id="l-layout"><option value="side">왼쪽</option><option value="top">위쪽</option></select></div>
      <div class="row"><div>서비스 색상을 UI에 반영 (컬러풀 메뉴)</div>${sw('l-color', s.colorful)}</div>
      <div class="row"><div>강조 색상</div><div class="swatches">${ACCENTS.map((c) => `<button class="swatch ${s.accent.toUpperCase() === c ? 'sel' : ''}" data-c="${c}" style="background:${c}"></button>`).join('')}</div></div>`;
    $('#l-theme').value = s.theme; $('#l-layout').value = s.layout;
    bindSelect('l-theme', 'theme'); bindSelect('l-layout', 'layout'); bindSwitch('l-color', 'colorful');
    p.querySelectorAll('.swatch').forEach((b) => (b.onclick = async () => { await patch({ accent: b.dataset.c }); paintPane('look'); }));
  } else if (sec === 'security') {
    p.innerHTML = `
      <div class="row"><div>패스코드 잠금<div class="sub">${S.has_passcode ? '설정됨 — 실행 시와 트레이로 숨길 때 잠깁니다' : '설정되지 않음'}</div></div>
        <div class="inline">${S.has_passcode ? '<button id="p-change">변경</button><button class="danger" id="p-clear">해제</button>' : '<button class="primary" id="p-set">설정</button>'}</div></div>
      <div class="row"><div>트레이로 숨길 때 자동 잠금</div>${sw('p-hide', s.lock_on_hide)}</div>
      <div class="sub muted" style="margin-top:10px">패스코드는 솔트+SHA-256 해시로만 저장됩니다. 잊으면 앱 데이터의 store.json에서 passcode 항목을 지우면 초기화됩니다.</div>`;
    bindSwitch('p-hide', 'lock_on_hide');
    const ask = (label, withOld) => {
      const old = withOld ? prompt('현재 패스코드') : null; if (withOld && old === null) return null;
      const code = prompt(label); return code === null ? null : { old, code };
    };
    if (S.has_passcode) {
      $('#p-change').onclick = async () => { const r = ask('새 패스코드 (4자 이상)', true); if (r) { S = await call('set_passcode', { code: r.code, old: r.old }); toast('변경되었습니다'); paintPane('security'); } };
      $('#p-clear').onclick = async () => { const c = prompt('현재 패스코드'); if (c !== null) { S = await call('clear_passcode', { code: c }); render(); paintPane('security'); } };
    } else {
      $('#p-set').onclick = async () => { const r = ask('새 패스코드 (4자 이상)', false); if (r) { S = await call('set_passcode', { code: r.code, old: null }); render(); paintPane('security'); } };
    }
  } else if (sec === 'dl') {
    p.innerHTML = `<div class="row"><div>다운로드 폴더<div class="sub">${esc(s.download_dir || '(Windows 기본 다운로드 폴더)')}</div></div>
      <div class="inline"><button id="d-pick">폴더 선택</button><button id="d-reset">기본값</button></div></div>`;
    $('#d-pick').onclick = async () => {
      const dir = await call('pick_folder');
      if (dir) { await patch({ download_dir: dir }); paintPane('dl'); }
    };
    $('#d-reset').onclick = async () => { await patch({ download_dir: null }); paintPane('dl'); };
  } else {
    p.innerHTML = `<h3 style="margin-top:0">Dock ${esc(S.version)}</h3>
      <p class="muted">메일·캘린더·메신저 같은 웹 서비스를 서비스별 격리 로그인으로 모아 보는 개인용 독입니다.<br>광고·결제·원격 분석이 없고 모든 기능이 항상 활성화되어 있습니다.</p>
      <div class="row"><div>업데이트 확인<div class="sub" id="u-msg">새 버전이 있는지 GitHub 릴리즈에서 확인해요</div></div>
        <div class="inline"><button id="u-open" hidden>릴리즈 페이지</button><button class="primary" id="u-install" hidden>지금 업데이트</button><button class="primary" id="u-check">지금 확인</button></div></div>
      <div class="row"><div>자동으로 업데이트 확인<div class="sub">시작 30초 뒤와 6시간마다 확인하고, 새 버전이 있으면 알림으로 알려줘요</div></div>${sw('u-auto', s.auto_update_check)}</div>`;
    bindSwitch('u-auto', 'auto_update_check');
    let updateUrl = null;
    $('#u-open').onclick = () => updateUrl && call('open_external', { url: updateUrl });
    $('#u-install').onclick = async () => {
      const btn = $('#u-install'); const msg = $('#u-msg');
      btn.disabled = true; btn.textContent = '받는 중…'; $('#u-open').hidden = true;
      msg.textContent = '업데이트를 내려받고 있어요…';
      try {
        await invoke('install_update_cmd');          // 성공하면 앱이 종료되고 설치가 진행돼요
        msg.textContent = '설치를 시작해요. 앱이 잠시 종료됐다가 다시 열려요.';
      } catch (e) {
        msg.textContent = typeof e === 'string' ? e : '업데이트하지 못했어요';
        btn.disabled = false; btn.textContent = '다시 시도'; $('#u-open').hidden = false;
      }
    };
    $('#u-check').onclick = async () => {
      const btn = $('#u-check'); const msg = $('#u-msg');
      btn.disabled = true; btn.textContent = '확인 중…'; $('#u-open').hidden = true; $('#u-install').hidden = true;
      try {
        const r = await invoke('check_update');
        if (r.available) {
          updateUrl = r.url;
          if (r.can_install) {
            const mb = r.size ? ` · ${(r.size / 1048576).toFixed(1)}MB` : '';
            msg.textContent = `새 버전 v${r.latest}이 있어요 (현재 v${r.current}${mb})`; $('#u-install').hidden = false; btn.hidden = true;
          } else {
            msg.textContent = `새 버전 v${r.latest}이 있지만 앱 안에서 설치할 수 없어요 (설치 파일 또는 무결성 정보가 없어요). 릴리즈 페이지에서 받아 주세요`;
            $('#u-open').hidden = false;
          }
        } else {
          msg.textContent = `최신 버전이에요 (v${r.current})`;
        }
      } catch (e) {
        msg.textContent = typeof e === 'string' ? e : '확인하지 못했어요';
      }
      btn.disabled = false; btn.textContent = '지금 확인';
    };
  }
}

// ───────── 다운로드 패널 ─────────
function downloadsDialog() {
  const paint = () => {
    const list = S.downloads;
    $('#dl-body').innerHTML = list.length ? list.map((d) => `<div class="dl"><div class="name" title="${esc(d.path)}">${esc(d.name)}</div>
      <div class="st">${{ progress: '다운로드 중…', done: '완료', failed: '실패' }[d.state]}</div>
      ${d.state === 'done' ? `<button data-a="open" data-id="${d.id}">열기</button><button data-a="folder" data-id="${d.id}">폴더</button>` : ''}
      <button data-a="${d.state === 'done' ? 'delete' : 'remove'}" data-id="${d.id}" title="${d.state === 'done' ? '파일 삭제' : '목록에서 제거'}">${d.state === 'done' ? '삭제' : '제거'}</button></div>`).join('')
      : '<div class="muted">다운로드 내역이 없습니다.</div>';
    $('#dl-body').querySelectorAll('[data-a]').forEach((b) => (b.onclick = () => call('download_action', { id: b.dataset.id, action: b.dataset.a })));
  };
  modal(`<div class="dialog"><header>다운로드<button class="icon" data-close>${XI}</button></header><div class="body" id="dl-body"></div>
    <footer><button id="dl-clear">완료 항목 지우기</button><button data-close>닫기</button></footer></div>`, () => {
    dlOpen = true; paint(); $('#dl-clear').onclick = () => call('clear_downloads');
  });
  window.__paintDownloads = () => dlOpen && $('#dl-body') && paint();
}

// ───────── 이벤트 연결 ─────────
$('#btn-add').onclick = addAccountDialog;
$('#btn-settings').onclick = () => settingsDialog();
$('#btn-downloads').onclick = downloadsDialog;
$('#btn-lock').onclick = () => call('lock_app');
$('#nav-back').onclick = () => call('view_nav', { action: 'back' });
$('#nav-forward').onclick = () => call('view_nav', { action: 'forward' });
$('#nav-reload').onclick = () => call('view_nav', { action: 'reload' });
$('#nav-home').onclick = () => call('view_nav', { action: 'home' });
$('#btn-login-done').onclick = async () => { S = await call('finish_login', { accountId: cur.acc }); await selectAccount(cur.acc); };

$('#lock-form').onsubmit = async (e) => {
  e.preventDefault();
  const ok = await call('unlock', { code: $('#lock-input').value });
  if (ok) { $('#lock-input').value = ''; $('#lock-msg').textContent = ''; S.locked = false; render(); syncOverlay(); }
  else { $('#lock-msg').textContent = '패스코드가 일치하지 않습니다'; const f = $('#lock-form'); f.classList.remove('shake'); void f.offsetWidth; f.classList.add('shake'); $('#lock-input').select(); }
};

window.addEventListener('keydown', (e) => {
  if (S?.locked) return;
  if (e.ctrlKey && /^[1-9]$/.test(e.key)) { const a = S.accounts[+e.key - 1]; a && selectAccount(a.id); }
  else if (e.ctrlKey && e.key.toLowerCase() === 'r') { e.preventDefault(); call('view_nav', { action: 'reload' }); }
  else if (e.ctrlKey && e.key.toLowerCase() === 'l' && S.has_passcode) call('lock_app');
  else if (e.altKey && e.key === 'ArrowLeft') call('view_nav', { action: 'back' });
  else if (e.altKey && e.key === 'ArrowRight') call('view_nav', { action: 'forward' });
  else if (e.key === 'Escape') closeModal();
});

const setOnline = () => { $('#offline').hidden = navigator.onLine; if (navigator.onLine && cur.acc) call('view_nav', { action: 'reload' }).catch(() => {}); };
window.addEventListener('offline', () => ($('#offline').hidden = false));
window.addEventListener('online', setOnline);

listen('badges', (e) => { S.badges = e.payload; render(); });
listen('update-progress', (e) => { const m = $('#u-msg'); if (m && $('#u-install')) m.textContent = `내려받는 중… ${e.payload.percent}%`; });
listen('update-installing', () => { const m = $('#u-msg'); if (m) m.textContent = '설치를 시작해요. 앱이 잠시 종료됐다가 다시 열려요.'; });
listen('open-settings', (e) => {
  settingsDialog(e.payload.section || 'general');
  if (e.payload.section === 'about') setTimeout(() => { const b = $('#u-check'); b && b.click(); }, 250);
});
listen('downloads', (e) => { S.downloads = e.payload; render(); window.__paintDownloads && window.__paintDownloads(); });
listen('locked', (e) => { S.locked = e.payload; render(); syncOverlay(); if (e.payload) setTimeout(() => $('#lock-input').focus(), 50); });
listen('route', (e) => {
  const { account, sub } = e.payload;
  if (sub >= 0 && S.accounts.some((a) => a.id === account)) { cur.acc = account; cur.sub = 0; render(); }
});
listen('login-success', async (e) => {
  if (e.payload.account !== cur.acc || cur.sub >= 0) return;
  S = await call('finish_login', { accountId: cur.acc }); toast('로그인되었습니다'); await selectAccount(cur.acc);
});

(async function init() {
  S = await invoke('get_state');
  $('#offline').hidden = navigator.onLine;
  render(); syncOverlay();
  if (S.locked) setTimeout(() => $('#lock-input').focus(), 100);
  if (S.accounts[0]) await selectAccount(S.accounts[0].id);
  else addAccountDialog();
})();
