(function () {
  if (window.__dock) return;
  window.__dock = true;
  var inv = function (cmd, args) {
    try { return window.__TAURI_INTERNALS__.invoke(cmd, args); } catch (e) { return Promise.reject(e); }
  };
  var isTop = window.top === window;
  var host = location.hostname;

  // <a target="_blank"> 클릭은 WebView2가 새 창 요청(on_new_window)으로 앱에 전달하지 않고 삼킨다.
  // (window.open과 이름 있는 target은 정상 전달된다.) 클릭 직전에 이름 있는 target으로 바꿔 같은 경로로 보낸다.
  (function () {
    var NAME = '_dockblank';
    var fix = function (el) {
      var t = (el.getAttribute('target') || '').toLowerCase();
      if (t === '_blank' || t === '_new') el.setAttribute('target', NAME);
    };
    document.addEventListener('click', function (e) {
      var path = e.composedPath ? e.composedPath() : [];
      for (var i = 0; i < path.length; i++) {
        var n = path[i];
        if (n && n.getAttribute && (n.tagName === 'A' || n.tagName === 'AREA') && n.hasAttribute('target')) { fix(n); break; }
      }
    }, true);
    document.addEventListener('submit', function (e) {
      if (e.target && e.target.getAttribute && e.target.hasAttribute('target')) fix(e.target);
    }, true);
  })();

  // 웹 Notification -> 네이티브 토스트 (Gmail은 아래 Atom 폴링이 담당하므로 제외)
  if (isTop && host !== 'mail.google.com') {
    var N = function (title, opts) {
      opts = opts || {};
      inv('report_event', { event: { kind: 'notification', title: String(title), body: String(opts.body || '') } }).catch(function () {});
      this.title = title; this.body = opts.body || '';
    };
    N.permission = 'granted';
    N.requestPermission = function (cb) { if (cb) cb('granted'); return Promise.resolve('granted'); };
    N.prototype.close = function () {};
    N.prototype.addEventListener = function () {};
    N.prototype.removeEventListener = function () {};
    try { Object.defineProperty(window, 'Notification', { value: N, configurable: true, writable: true }); } catch (e) {}

    // 서비스 워커 방식(registration.showNotification)을 쓰는 사이트(예: Google Calendar)도 같은 경로로 보낸다
    try {
      if (window.ServiceWorkerRegistration && ServiceWorkerRegistration.prototype.showNotification) {
        ServiceWorkerRegistration.prototype.showNotification = function (title, opts) {
          opts = opts || {};
          inv('report_event', { event: { kind: 'notification', title: String(title), body: String(opts.body || '') } }).catch(function () {});
          return Promise.resolve();
        };
      }
    } catch (e) {}
  }

  // 로그인 화면에서 Passkey 요청을 거부해 비밀번호 로그인으로 유도
  if (host === 'accounts.google.com' && navigator.credentials) {
    var cr = navigator.credentials;
    var rej = function () { var e = new DOMException('Passkey disabled', 'NotAllowedError'); return Promise.reject(e); };
    var oc = cr.create ? cr.create.bind(cr) : null;
    var og = cr.get ? cr.get.bind(cr) : null;
    try {
      if (oc) cr.create = function (o) { return o && o.publicKey ? rej() : oc(o); };
      if (og) cr.get = function (o) { return o && o.publicKey ? rej() : og(o); };
    } catch (e) {}
  }

  // Gmail Atom 피드 폴링 (페이지 컨텍스트라 쿠키가 그대로 적용됨)
  if (isTop && host === 'mail.google.com') {
    // 카테고리 피드를 먼저 읽고, 마지막에 받은편지함 전체 피드를 읽는다.
    // 앞에서 이미 본 메일은 건너뛰므로 남는 것이 Primary(또는 카테고리를 끈 경우 전체)다.
    var cats = {
      social: '^smartlabel_social',
      promotions: '^smartlabel_promo',
      updates: '^smartlabel_notification',
      forums: '^smartlabel_group',
      primary: ''
    };
    var seen = new Set();
    var first = true;
    var poll = async function () {
      var m = location.pathname.match(/\/mail\/u\/(\d+)/);
      var u = m ? m[1] : '0';
      var items = [];
      var names = Object.keys(cats);
      for (var i = 0; i < names.length; i++) {
        var cat = names[i];
        try {
          var r = await fetch('/mail/u/' + u + '/feed/atom' + (cats[cat] ? '/' + cats[cat] : ''), { credentials: 'include' });
          if (!r.ok) continue;
          var x = new DOMParser().parseFromString(await r.text(), 'text/xml');
          var es = x.querySelectorAll('entry');
          for (var j = 0; j < es.length; j++) {
            var e = es[j];
            var idEl = e.querySelector('id');
            var id = idEl && idEl.textContent;
            if (!id || seen.has(id)) continue;
            seen.add(id);
            if (first) continue;
            var nm = e.querySelector('author > name');
            var em = e.querySelector('author > email');
            var ti = e.querySelector('title');
            var su = e.querySelector('summary');
            items.push({
              category: cat,
              from: (nm && nm.textContent) || (em && em.textContent) || '',
              title: (ti && ti.textContent) || '',
              summary: (su && su.textContent) || ''
            });
          }
        } catch (err) { /* 다음 주기에 재시도 */ }
      }
      first = false;
      if (items.length) inv('report_event', { event: { kind: 'mail', items: items } }).catch(function () {});
    };
    setTimeout(function () { poll(); setInterval(poll, 60000); }, 15000);
  }
})();
