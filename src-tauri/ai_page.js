(function (op, a) {
  var K = '__dockAiRefs', st = window[K];
  if (!st) { st = { m: new Map(), n: 0 }; try { Object.defineProperty(window, K, { value: st }); } catch (e) {} }
  var Q = 'a[href],button,input:not([type=hidden]),select,textarea,summary,[role=button],[role=link],[role=tab],[role=menuitem],[role=menuitemcheckbox],[role=menuitemradio],[role=checkbox],[role=radio],[role=switch],[role=option],[role=combobox],[role=textbox],[role=searchbox],[role=row],[role=treeitem],[contenteditable=""],[contenteditable=true]';
  var RISK = /(send|submit|delete|remove|discard|pay\b|purchase|buy|checkout|place order|order|confirm|publish|post\b|transfer|sign ?out|log ?out|unsubscribe|archive|trash|spam|보내기|전송|발송|삭제|제거|지우기|결제|구매|주문|확인|게시|등록|송금|제출|로그아웃|구독 ?취소|보관|휴지통|스팸)/i;

  function clip(s, n) { s = String(s || '').replace(/\s+/g, ' ').trim(); return s.length > n ? s.slice(0, n - 1) + '…' : s; }
  function vis(el) {
    var r = el.getBoundingClientRect();
    if (r.width < 2 || r.height < 2) return null;
    var cs = getComputedStyle(el);
    if (cs.visibility === 'hidden' || cs.display === 'none') return null;
    if (el.closest('[aria-hidden=true],[inert]')) return null;
    return r;
  }
  function lab(el) {
    var t = el.getAttribute('aria-label'), tag = el.tagName;
    if (!t && el.labels && el.labels.length) t = el.labels[0].innerText;
    if (!t) { var id = el.getAttribute('aria-labelledby'); if (id) { var n = document.getElementById(id.split(' ')[0]); if (n) t = n.innerText; } }
    if (!t && tag === 'INPUT' && /^(submit|button|reset)$/.test(el.type)) t = el.value;
    if (!t && tag !== 'INPUT' && tag !== 'SELECT' && tag !== 'TEXTAREA') t = el.innerText || el.textContent;
    if (!t) t = el.getAttribute('placeholder') || el.title || el.getAttribute('alt') || el.getAttribute('name') || '';
    if (!t && el.querySelector) { var im = el.querySelector('img[alt]'); if (im) t = im.getAttribute('alt'); }
    return clip(t, 50);
  }
  function role(el) {
    var r = el.getAttribute('role'), tag = el.tagName.toLowerCase();
    if (tag === 'a') return 'link';
    if (tag === 'button' || tag === 'summary') return 'btn';
    if (tag === 'select') return 'select';
    if (tag === 'textarea') return 'text';
    if (tag === 'input') {
      var t = el.type;
      if (t === 'checkbox' || t === 'radio' || t === 'file') return t;
      if (/^(submit|button|reset|image)$/.test(t)) return 'btn';
      return t === 'password' ? 'pw' : 'input';
    }
    if (r) return r === 'button' ? 'btn' : r;
    return el.isContentEditable ? 'edit' : tag;
  }
  function get(ref) { var w = st.m.get(String(ref)); var el = w && w.deref(); return el && el.isConnected ? el : null; }
  function stale(ref) { return { ok: false, e: 'ref ' + ref + ' is stale; call snapshot again' }; }
  function head() { return location.href.slice(0, 140) + ' | ' + clip(document.title, 60); }

  function snap(a) {
    var sc = a.scope ? get(a.scope) : null, note = '';
    if (!sc) {
      var dl = [].slice.call(document.querySelectorAll('dialog[open],[role=alertdialog],[aria-modal=true]')).filter(vis);
      if (dl.length) { sc = dl[dl.length - 1]; note = ' [dialog]'; }
    }
    st.m.clear(); st.n = 0;
    var root = sc || document.body || document.documentElement, vh = innerHeight;
    var q = a.q ? String(a.q).toLowerCase() : '', max = Math.min(a.max || 60, 200), out = [], total = 0;
    function add(el, r, ro, extra) {
      var below = r.top >= vh, above = r.bottom <= 0;
      if (!a.all && (above || r.top > vh * 2)) return;
      var l = lab(el);
      if (q && (l + ' ' + ro).toLowerCase().indexOf(q) < 0) return;
      total++;
      if (out.length >= max) return;
      var id = 'e' + (++st.n); st.m.set(id, new WeakRef(el));
      var s = id + ' ' + ro + (l ? ' "' + l + '"' : ''), tag = el.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA') {
        if (el.type !== 'password' && el.type !== 'checkbox' && el.type !== 'radio' && el.value) s += ' =' + clip(el.value, 40);
        if (el.checked) s += ' [x]';
      } else if (tag === 'SELECT') {
        var o = el.options[el.selectedIndex]; if (o) s += ' =' + clip(o.text, 30);
      }
      if (el.getAttribute('aria-checked') === 'true' || el.getAttribute('aria-selected') === 'true' || el.getAttribute('aria-expanded') === 'true') s += ' [on]';
      if (el.disabled || el.getAttribute('aria-disabled') === 'true') s += ' [off]';
      if (below) s += ' ↓'; else if (above) s += ' ↑';
      out.push(s);
    }
    var nodes = root.querySelectorAll(Q);
    for (var i = 0; i < nodes.length; i++) { var r = vis(nodes[i]); if (r) add(nodes[i], r, role(nodes[i])); }
    if (out.length < 3 && !q) {
      var cand = root.querySelectorAll('div,span,li,img,td'), seen = 0;
      for (var j = 0; j < cand.length && seen < 3000; j++, seen++) {
        var c = cand[j], cs = getComputedStyle(c);
        if (cs.cursor !== 'pointer' || (c.parentElement && getComputedStyle(c.parentElement).cursor === 'pointer')) continue;
        var rr = vis(c); if (rr) add(c, rr, 'click');
      }
    }
    var hd = [].slice.call(root.querySelectorAll('h1,h2,h3,[role=heading]')).filter(function (h) { var r = vis(h); return r && r.bottom > 0 && r.top < vh; }).slice(0, 5).map(function (h) { return '# ' + clip(h.innerText, 60); });
    var ph = document.documentElement.scrollHeight;
    var s = head() + (ph > vh * 1.2 ? ' | y' + Math.round(scrollY) + '/' + ph : '') + note;
    if (hd.length) s += '\n' + hd.join('\n');
    if (out.length) s += '\n' + out.join('\n'); else s += '\n(no interactive elements)';
    if (total > out.length) s += '\n…+' + (total - out.length) + ' more (narrow with q/scope, or all)';
    return s;
  }

  function read(a) {
    var el = a.ref ? get(a.ref) : document.body;
    if (!el) return stale(a.ref).e;
    var t = (el.innerText || '').replace(/[ \t]+/g, ' ').replace(/ ?\n ?/g, '\n').replace(/\n{3,}/g, '\n\n').trim();
    var max = a.max || 6000, sel = '';
    try { sel = String(getSelection() || '').trim(); } catch (e) {}
    var s = head() + '\n---\n' + t.slice(0, max);
    if (t.length > max) s += '\n…(+' + (t.length - max) + ' chars; use ref to read a part)';
    if (sel) s += '\n[selected] ' + clip(sel, 500);
    return s;
  }

  function prep(a) {
    var el = get(a.ref);
    if (!el) return stale(a.ref);
    try { el.scrollIntoView({ block: 'center', inline: 'center' }); } catch (e) {}
    var r = el.getBoundingClientRect();
    if (r.width < 1 || r.height < 1) return { ok: false, e: 'ref ' + a.ref + ' is not visible' };
    var x = Math.round(Math.min(Math.max(r.left + r.width / 2, 1), innerWidth - 1));
    var y = Math.round(Math.min(Math.max(r.top + r.height / 2, 1), innerHeight - 1));
    var l = lab(el), ro = role(el), tag = el.tagName.toLowerCase();
    if (a.verb === 'click' || a.verb === 'dblclick' || a.verb === 'hover') {
      var top = document.elementFromPoint(x, y);
      if (!top || !(top === el || el.contains(top) || top.contains(el))) return { ok: false, e: ref(a.ref) + ' is covered by ' + role(top || document.body) + ' "' + (top ? lab(top) : '') + '"' };
    }
    var form = el.closest && el.closest('form');
    var submit = (tag === 'button' && form && (el.getAttribute('type') || 'submit') === 'submit') || (tag === 'input' && el.type === 'submit');
    var risk = (submit || RISK.test(l + ' ' + (el.getAttribute('data-tooltip') || ''))) ? (submit ? 'submit' : 'label') : null;
    return { ok: true, x: x, y: y, l: l, ro: ro, risk: risk, pw: el.type === 'password', host: location.hostname };
  }
  function ref(r) { return 'ref ' + r; }

  function focus(a) {
    var el = get(a.ref);
    if (!el) return stale(a.ref);
    var tag = el.tagName;
    if (el.type === 'password') return { ok: false, e: 'password fields are blocked' };
    if (!(tag === 'INPUT' || tag === 'TEXTAREA' || el.isContentEditable)) return { ok: false, e: ref(a.ref) + ' is not editable' };
    el.focus();
    if (tag === 'INPUT' || tag === 'TEXTAREA') {
      try { if (a.clear) el.select(); else { var n = el.value.length; el.setSelectionRange(n, n); } } catch (e) {}
    } else {
      var rg = document.createRange(); rg.selectNodeContents(el); if (!a.clear) rg.collapse(false);
      var s = getSelection(); s.removeAllRanges(); s.addRange(rg);
    }
    return { ok: true };
  }

  function sel(a) {
    var el = get(a.ref);
    if (!el) return stale(a.ref);
    if (el.tagName !== 'SELECT') return { ok: false, e: ref(a.ref) + ' is not a select' };
    var t = String(a.text).toLowerCase(), idx = -1, i, o;
    for (i = 0; i < el.options.length; i++) { o = el.options[i]; if (o.text.trim().toLowerCase() === t || o.value.toLowerCase() === t) { idx = i; break; } }
    if (idx < 0) for (i = 0; i < el.options.length; i++) if (el.options[i].text.toLowerCase().indexOf(t) >= 0) { idx = i; break; }
    if (idx < 0) return { ok: false, e: 'no such option; options: ' + [].slice.call(el.options, 0, 12).map(function (o) { return clip(o.text, 30); }).join(' | ') };
    el.selectedIndex = idx;
    el.dispatchEvent(new Event('input', { bubbles: true })); el.dispatchEvent(new Event('change', { bubbles: true }));
    return { ok: true };
  }

  function scroll(a) {
    var el = a.ref ? get(a.ref) : null;
    if (a.ref && !el) return stale(a.ref);
    var t = a.to, h = el ? el.clientHeight : innerHeight, dy = 0;
    if (t === 'down') dy = h * 0.8; else if (t === 'up') dy = -h * 0.8; else if (t === 'top' || t === 'bottom') dy = null; else dy = parseFloat(t) || 0;
    if (el) { if (dy === null) el.scrollTop = t === 'top' ? 0 : el.scrollHeight; else el.scrollBy(0, dy); }
    else { if (dy === null) scrollTo(0, t === 'top' ? 0 : document.documentElement.scrollHeight); else scrollBy(0, dy); }
    return { ok: true };
  }

  switch (op) {
    case 'snapshot': return snap(a || {});
    case 'read': return read(a || {});
    case 'prep': return prep(a);
    case 'focus': return focus(a);
    case 'select': return sel(a);
    case 'scroll': return scroll(a);
    case 'info': return head();
  }
  return null;
})
