use crate::model::*;
use crate::{emit_badges, recipe_for};
use regex::Regex;
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::image::Image;
use tauri::webview::{DownloadEvent, NewWindowResponse, PageLoadEvent, WebviewBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Url, WebviewUrl, WebviewWindowBuilder};

const INJECT: &str = include_str!("../inject.js");
const CHROME_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";

/// "Google로 계속하기" 같은 소셜 로그인 팝업 주소. 결과를 window.opener로 돌려주므로 앱 안의 새 창으로 열어야 한다.
const AUTH_POPUP_HOSTS: &[&str] = &[
    r"^https://accounts\.google\.com/",
    r"^https://appleid\.apple\.com/",
    r"^https://login\.microsoftonline\.com/",
    r"^https://login\.live\.com/",
    r"^https://github\.com/login/",
    r"^https://www\.facebook\.com/(v[\d.]+/)?dialog/",
    r"^https://kauth\.kakao\.com/",
    r"^https://nid\.naver\.com/oauth",
    // Notion이 로그인 전에 팝업 차단 여부를 검사하는 창(열려서 응답해야 로그인 단계로 넘어간다)
    r"^https://([a-z0-9-]+\.)?notion\.(so|com)/verifyNoPopupBlocker",
];

fn is_auth_popup(recipe: &Recipe, url: &str) -> bool {
    AUTH_POPUP_HOSTS.iter().any(|p| matches_any(&[p.to_string()], url)) || matches_any(&recipe.auth_popup, url)
}

/// 임베디드 웹뷰 UA로는 구글 로그인이 막히므로 Chrome UA를 쓸 서비스
fn wants_chrome_ua(recipe: &Recipe) -> bool {
    !recipe.chrome_ua_hosts.is_empty() || recipe.key == "notion" || recipe.key == "custom"
}

pub fn view_label(acc: &str, sub: i32) -> String {
    if sub < 0 {
        format!("v-{}-login", acc)
    } else {
        format!("v-{}-{}", acc, sub)
    }
}

pub fn account_of_label(label: &str) -> Option<String> {
    let rest = label.strip_prefix("v-").or_else(|| label.strip_prefix("pop-")).or_else(|| label.strip_prefix("auth-"))?;
    Some(rest.split('-').next()?.to_string())
}

fn matches_any(list: &[String], url: &str) -> bool {
    list.iter().any(|p| Regex::new(&format!("(?i){}", p)).map(|r| r.is_match(url)).unwrap_or(false))
}

fn find_account(app: &AppHandle, id: &str) -> Option<Account> {
    app.state::<AppState>().store.lock().unwrap().accounts.iter().find(|a| a.id == id).cloned()
}

fn profile_dir(app: &AppHandle, id: &str) -> PathBuf {
    let profile = find_account(app, id).map(|a| crate::profile_of(&a)).unwrap_or_else(|| id.to_string());
    let p = app.state::<AppState>().data_root.join("profiles").join(profile);
    let _ = std::fs::create_dir_all(&p);
    p
}

fn init_script(recipe: &Recipe) -> String {
    let mut s = String::from(INJECT);
    if let Some(css) = &recipe.css {
        let css = serde_json::to_string(css).unwrap_or_default();
        s.push_str(&format!(
            "\n(function(){{var css={};var add=function(){{var s=document.createElement('style');s.textContent=css;(document.head||document.documentElement).appendChild(s)}};if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',add);else add();}})();",
            css
        ));
    }
    s
}

fn spawn_route(app: &AppHandle, acc: &str, sub: i32, url: String) {
    let app = app.clone();
    let acc = acc.to_string();
    std::thread::spawn(move || {
        let _ = show(&app, &acc, sub, Some(url));
        let _ = app.emit("route", json!({ "account": acc, "sub": sub }));
    });
}

/// 같은 세션을 공유하는 다른 항목 중 이 URL을 처리하는 항목의 id
fn sibling_for(app: &AppHandle, self_id: &str, profile: &str, url: &str) -> Option<String> {
    let st = app.state::<AppState>();
    let store = st.store.lock().unwrap();
    store
        .accounts
        .iter()
        .find(|a| a.id != self_id && crate::profile_of(a) == profile && a.subs.first().map(|s| matches_any(&s.handled, url)).unwrap_or(false))
        .map(|a| a.id.clone())
}

fn open_system(app: &AppHandle, url: &str) {
    use tauri_plugin_opener::OpenerExt;
    if url.starts_with("http://") || url.starts_with("https://") {
        let _ = app.opener().open_url(url, None::<&str>);
    }
}

fn unique_path(dir: &PathBuf, name: &str) -> PathBuf {
    let mut p = dir.join(name);
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{}", e)),
        None => (name.to_string(), String::new()),
    };
    let mut n = 1;
    while p.exists() {
        p = dir.join(format!("{} ({}){}", stem, n, ext));
        n += 1;
    }
    p
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// 웹뷰 한 개를 만든다. 반드시 메인 스레드가 아닌 곳에서 호출해야 한다(add_child가 메인 스레드를 기다림).
fn ensure_view(app: &AppHandle, acc_id: &str, sub: i32, url_override: Option<String>) -> Result<tauri::Webview, String> {
    let label = view_label(acc_id, sub);
    if let Some(v) = app.get_webview(&label) {
        if let Some(u) = url_override {
            if let Ok(u) = u.parse::<Url>() {
                let _ = v.navigate(u);
            }
        }
        return Ok(v);
    }
    let acc = find_account(app, acc_id).ok_or("계정 없음")?;
    let recipe = recipe_for(&acc);
    let keep = crate::keep_login(&acc);
    let (start_url, sub_idx) = if sub < 0 {
        (recipe.login_url.clone(), -1)
    } else {
        (acc.subs.get(sub as usize).ok_or("서비스 없음")?.url.clone(), sub)
    };
    let url_str = url_override.unwrap_or(start_url);
    let url: Url = url_str.parse().map_err(|_| format!("잘못된 URL: {}", url_str))?;

    let window = app.get_window("main").ok_or("main window missing")?;
    let mut b = WebviewBuilder::new(&label, WebviewUrl::External(url))
        .data_directory(profile_dir(app, acc_id))
        .initialization_script(init_script(&recipe));
    if wants_chrome_ua(&recipe) {
        b = b.user_agent(CHROME_UA);
    }

    // 제목 → 배지
    {
        let app = app.clone();
        let acc_id = acc_id.to_string();
        let subs = acc.subs.clone();
        let label = label.clone();
        b = b.on_document_title_changed(move |_wv, title| {
            if sub_idx < 0 {
                return;
            }
            let s = &subs[sub_idx as usize];
            let pat = s.badge_regex.clone().unwrap_or_else(|| r"\((\d+)\)".to_string());
            let count = Regex::new(&pat)
                .ok()
                .and_then(|r| r.captures(&title).and_then(|c| c.get(1)).and_then(|m| m.as_str().parse::<u32>().ok()))
                .unwrap_or(0);
            {
                let st = app.state::<AppState>();
                let mut v = st.views.lock().unwrap();
                v.insert(label.clone(), ViewInfo { account: acc_id.clone(), sub: sub_idx, count });
            }
            emit_badges(&app);
        });
    }

    // 같은 로그인 세션을 쓰는 다른 서비스의 URL이면 그 항목으로 이동시킨다
    let mine = acc.subs.first().map(|x| x.handled.clone()).unwrap_or_default();
    let profile = crate::profile_of(&acc);
    {
        let app = app.clone();
        let acc_id = acc_id.to_string();
        let mine = mine.clone();
        let profile = profile.clone();
        b = b.on_navigation(move |u| {
            let s = u.as_str();
            if sub_idx < 0 || matches_any(&mine, s) {
                return true;
            }
            if let Some(other) = sibling_for(&app, &acc_id, &profile, s) {
                spawn_route(&app, &other, 0, s.to_string());
                return false;
            }
            true
        });
    }

    // window.open / target=_blank
    {
        let app = app.clone();
        let acc_id = acc_id.to_string();
        let recipe = recipe.clone();
        // 직접 입력한 서비스는 새 창 링크를 PC 브라우저로 보내지 않고 현재 화면에서 연다
        // (프리셋 서비스는 받은편지함 같은 화면이 덮이지 않도록 기존대로 외부 브라우저)
        let inline_blank = recipe.key == "custom";
        b = b.on_new_window(move |u, f| {
            let s = u.as_str().to_string();
            // 링크 주소의 쿼리/조각에는 토큰이 들어 있을 수 있으니 로그에는 경로까지만 남긴다
            let shown = s.split(|c| c == '?' || c == '#').next().unwrap_or("").chars().take(200).collect::<String>();
            crate::log_line(&app, &format!("새 창 요청: {} (직접입력={})", shown, inline_blank));
            // 소셜 로그인 팝업: 부모와 같은 세션/opener를 유지한 앱 내 창으로 연다
            if is_auth_popup(&recipe, &s) {
                let label = format!("auth-{}-{}", acc_id, uuid::Uuid::new_v4().simple());
                let mut pb = WebviewWindowBuilder::new(&app, label, WebviewUrl::External("about:blank".parse().unwrap()))
                    .title("Dock — 로그인")
                    .inner_size(500.0, 680.0)
                    .window_features(f)
                    .initialization_script(init_script(&recipe))
                    .on_page_load({
                        let app = app.clone();
                        move |_w, p| {
                            let u = p.url();
                            let ev = if p.event() == PageLoadEvent::Finished { "끝" } else { "시작" };
                            crate::log_line(&app, &format!("인증 팝업 로드 {}: {}{}", ev, u.host_str().unwrap_or(""), u.path()));
                        }
                    });
                if wants_chrome_ua(&recipe) {
                    pb = pb.user_agent(CHROME_UA);
                }
                match pb.build() {
                    Ok(window) => {
                        crate::log_line(&app, "인증 팝업 열림");
                        return NewWindowResponse::Create { window };
                    }
                    Err(e) => {
                        crate::log_line(&app, &format!("인증 팝업 생성 실패: {}", e));
                        open_system(&app, &s);
                        return NewWindowResponse::Deny;
                    }
                }
            }
            if matches_any(&recipe.popout, &s) {
                let app = app.clone();
                let acc_id = acc_id.clone();
                std::thread::spawn(move || {
                    let _ = open_popup(&app, &acc_id, &s);
                });
            } else if let Some(other) = sibling_for(&app, &acc_id, &profile, &s) {
                spawn_route(&app, &other, 0, s);
            } else if matches_any(&mine, &s) || (inline_blank && (s.starts_with("http://") || s.starts_with("https://"))) {
                spawn_route(&app, &acc_id, 0, s);
            } else {
                open_system(&app, &s);
            }
            NewWindowResponse::Deny
        });
    }

    // 다운로드
    {
        let app = app.clone();
        b = b.on_download(move |_wv, ev| handle_download(&app, ev));
    }

    // 로그인 성공 감지
    if sub < 0 {
        let app = app.clone();
        let acc_id = acc_id.to_string();
        let ok = recipe.login_success.clone();
        // 직접 입력 서비스는 성공 패턴이 없으므로, 같은 호스트의 로그인 페이지가 아닌 곳으로 이동하면 성공으로 본다
        let custom_hosts = if recipe.key == "custom" { recipe.subs.first().map(|s| s.handled.clone()).unwrap_or_default() } else { Vec::new() };
        b = b.on_page_load(move |wv, p| {
            if p.event() == PageLoadEvent::Finished {
                if keep {
                    persist_session_cookies(&wv);
                }
                let u = p.url().as_str().to_string();
                let lu = u.to_lowercase();
                let custom_ok = !custom_hosts.is_empty()
                    && matches_any(&custom_hosts, &u)
                    && !["login", "signin", "sign-in", "logon"].iter().any(|k| lu.contains(k));
                if !lu.contains("setosid") && (matches_any(&ok, &u) || custom_ok) {
                    let _ = app.emit("login-success", json!({ "account": acc_id }));
                }
            }
        });
    }

    // 로그인 화면이 아닌 일반 화면도 페이지가 로드될 때마다 세션 쿠키를 저장 가능한 쿠키로 바꾼다
    if sub >= 0 && keep {
        b = b.on_page_load(move |wv, p| {
            if p.event() == PageLoadEvent::Finished {
                persist_session_cookies(&wv);
            }
        });
    }

    let (x, y, w, h) = *app.state::<AppState>().rect.lock().unwrap();
    let wv = window
        .add_child(b, LogicalPosition::new(x, y), LogicalSize::new(w, h))
        .map_err(|e| e.to_string())?;
    app.state::<AppState>()
        .views
        .lock()
        .unwrap()
        .entry(label)
        .or_insert(ViewInfo { account: acc_id.to_string(), sub: sub_idx, count: 0 });
    // 생성 중에 set_rect가 먼저 실행됐을 수 있으므로 최신 좌표를 다시 적용한다
    let (x, y, w, h) = *app.state::<AppState>().rect.lock().unwrap();
    let _ = wv.set_position(LogicalPosition::new(x, y));
    let _ = wv.set_size(LogicalSize::new(w, h));
    // 새 웹뷰는 보이는 상태로 만들어지므로 가시성 규칙을 즉시 적용한다
    apply_visibility(app);
    Ok(wv)
}

fn handle_download(app: &AppHandle, ev: DownloadEvent<'_>) -> bool {
    let st = app.state::<AppState>();
    match ev {
        DownloadEvent::Requested { url, destination } => {
            let dir = st
                .store
                .lock()
                .unwrap()
                .settings
                .download_dir
                .clone()
                .map(PathBuf::from)
                .or_else(|| app.path().download_dir().ok())
                .unwrap_or_else(|| destination.parent().map(|p| p.to_path_buf()).unwrap_or_default());
            let _ = std::fs::create_dir_all(&dir);
            let name = destination.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "download".into());
            let path = unique_path(&dir, &name);
            *destination = path.clone();
            st.downloads.lock().unwrap().insert(
                0,
                Download {
                    id: uuid::Uuid::new_v4().simple().to_string(),
                    name: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                    path: path.to_string_lossy().to_string(),
                    url: url.to_string(),
                    state: "progress".into(),
                    ts: now(),
                },
            );
            let _ = app.emit("downloads", &*st.downloads.lock().unwrap());
        }
        DownloadEvent::Finished { url, path, success } => {
            {
                let mut d = st.downloads.lock().unwrap();
                if let Some(item) = d.iter_mut().find(|i| i.state == "progress" && (i.url == url.to_string() || path.as_ref().map(|p| p.to_string_lossy() == i.path).unwrap_or(false))) {
                    item.state = if success { "done".into() } else { "failed".into() };
                    if success {
                        crate::notify(app, "다운로드 완료", &item.name, None);
                    }
                }
            }
            let _ = app.emit("downloads", &*st.downloads.lock().unwrap());
        }
        _ => {}
    }
    true
}


fn open_popup(app: &AppHandle, acc_id: &str, url: &str) -> Result<(), String> {
    let acc = find_account(app, acc_id).ok_or("계정 없음")?;
    let recipe = recipe_for(&acc);
    let u: Url = url.parse().map_err(|_| "bad url".to_string())?;
    let label = format!("pop-{}-{}", acc_id, uuid::Uuid::new_v4().simple());
    let mut b = WebviewWindowBuilder::new(app, label, WebviewUrl::External(u))
        .title(format!("Dock — {}", acc.name))
        .inner_size(960.0, 720.0)
        .data_directory(profile_dir(app, acc_id))
        .initialization_script(init_script(&recipe));
    if wants_chrome_ua(&recipe) {
        b = b.user_agent(CHROME_UA);
    }
    b.build().map(|_| ()).map_err(|e| e.to_string())
}

pub fn compose(app: &AppHandle, account_id: Option<String>, mailto: Option<String>) -> Result<(), String> {
    let (accounts, pref) = {
        let st = app.state::<AppState>();
        let s = st.store.lock().unwrap();
        (s.accounts.clone(), s.settings.mailto_account.clone())
    };
    let pick = |id: &Option<String>| id.as_ref().and_then(|i| accounts.iter().find(|a| a.id == *i).cloned());
    let acc = pick(&account_id)
        .or_else(|| pick(&pref))
        .or_else(|| accounts.iter().find(|a| a.subs.first().map(|s| s.is_mail).unwrap_or(false) && recipe_for(a).compose.is_some()).cloned())
        .or_else(|| accounts.first().cloned())
        .ok_or("계정이 없습니다")?;
    let recipe = recipe_for(&acc);
    let mut fields = std::collections::HashMap::<&str, String>::new();
    if let Some(m) = mailto {
        if let Ok(u) = Url::parse(&m) {
            fields.insert("TO", percent_decode(u.path()));
            for (k, v) in u.query_pairs() {
                match k.to_lowercase().as_str() {
                    "to" => {
                        let prev = fields.get("TO").cloned().unwrap_or_default();
                        fields.insert("TO", if prev.is_empty() { v.to_string() } else { format!("{},{}", prev, v) });
                    }
                    "cc" => {
                        fields.insert("CC", v.to_string());
                    }
                    "bcc" => {
                        fields.insert("BCC", v.to_string());
                    }
                    "subject" => {
                        fields.insert("SUBJECT", v.to_string());
                    }
                    "body" => {
                        fields.insert("BODY", v.to_string());
                    }
                    _ => {}
                }
            }
        }
    }
    let enc = |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>();
    let target = match &recipe.compose {
        Some(t) => {
            let mut out = t.clone();
            for k in ["TO", "CC", "BCC", "SUBJECT", "BODY"] {
                out = out.replace(&format!("{{{}}}", k.to_lowercase()), &enc(fields.get(k).map(|s| s.as_str()).unwrap_or("")));
            }
            out
        }
        None => acc.subs.iter().find(|s| s.is_mail).or(acc.subs.first()).map(|s| s.url.clone()).ok_or("메일 서비스 없음")?,
    };
    open_popup(app, &acc.id, &target)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() + 0 && i + 2 <= b.len() - 1 {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

pub fn show(app: &AppHandle, acc_id: &str, sub: i32, url: Option<String>) -> Result<(), String> {
    let label = view_label(acc_id, sub);
    ensure_view(app, acc_id, sub, url)?;
    *app.state::<AppState>().current.lock().unwrap() = Some(label);
    apply_visibility(app);
    Ok(())
}

pub fn apply_visibility(app: &AppHandle) {
    let st = app.state::<AppState>();
    let cur = st.current.lock().unwrap().clone();
    let hidden_all = st.overlay.load(Ordering::SeqCst) || st.locked.load(Ordering::SeqCst);
    let labels: Vec<String> = app.webviews().keys().filter(|l| l.starts_with("v-")).cloned().collect();
    for l in labels {
        if let Some(v) = app.get_webview(&l) {
            if !hidden_all && Some(&l) == cur.as_ref() {
                let _ = v.show();
            } else {
                let _ = v.hide();
            }
        }
    }
}

pub fn set_rect(app: &AppHandle, r: (f64, f64, f64, f64)) {
    *app.state::<AppState>().rect.lock().unwrap() = r;
    for (l, v) in app.webviews() {
        if l.starts_with("v-") {
            let _ = v.set_position(LogicalPosition::new(r.0, r.1));
            let _ = v.set_size(LogicalSize::new(r.2, r.3));
        }
    }
}

pub fn nav(app: &AppHandle, action: &str) {
    let cur = app.state::<AppState>().current.lock().unwrap().clone();
    let Some(v) = cur.and_then(|l| app.get_webview(&l)) else { return };
    match action {
        "reload" => {
            let _ = v.reload();
        }
        "back" => {
            let _ = v.eval("history.back()");
        }
        "forward" => {
            let _ = v.eval("history.forward()");
        }
        "home" => {
            if let Some(info) = app.state::<AppState>().views.lock().unwrap().get(v.label()).cloned() {
                if let Some(a) = find_account(app, &info.account) {
                    if let Some(s) = a.subs.get(info.sub.max(0) as usize) {
                        if let Ok(u) = s.url.parse::<Url>() {
                            let _ = v.navigate(u);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

pub fn close_account_views(app: &AppHandle, acc_id: &str) {
    let prefix = format!("v-{}-", acc_id);
    for (l, v) in app.webviews() {
        if l.starts_with(&prefix) || l.starts_with(&format!("pop-{}-", acc_id)) {
            let _ = v.close();
        }
    }
    let st = app.state::<AppState>();
    st.views.lock().unwrap().retain(|_, v| v.account != acc_id);
    let mut cur = st.current.lock().unwrap();
    if cur.as_ref().map(|c| c.starts_with(&prefix)).unwrap_or(false) {
        *cur = None;
    }
}

pub fn close_login(app: &AppHandle, acc_id: &str) {
    let l = view_label(acc_id, -1);
    if let Some(v) = app.get_webview(&l) {
        let _ = v.close();
    }
    let st = app.state::<AppState>();
    st.views.lock().unwrap().remove(&l);
    let mut cur = st.current.lock().unwrap();
    if cur.as_deref() == Some(l.as_str()) {
        *cur = None;
    }
}

/// 로그인된 항목을 백그라운드에서 미리 로드(배지/알림용)
pub fn preload_account(app: &AppHandle, acc_id: &str) {
    let _ = ensure_view(app, acc_id, 0, None);
}

pub fn preload_all(app: &AppHandle) {
    let ids: Vec<String> = app
        .state::<AppState>()
        .store
        .lock()
        .unwrap()
        .accounts
        .iter()
        .filter(|a| a.logged_in)
        .map(|a| a.id.clone())
        .collect();
    for id in ids {
        preload_account(app, &id);
    }
    apply_visibility(app);
}

// ───────────────────────── 로그인 유지(세션 쿠키 저장) ─────────────────────────

/// 이 웹뷰 프로필의 세션 쿠키(만료 시간 없음)에 만료 시간을 붙여 디스크에 남게 한다.
/// WebView2는 세션 쿠키를 앱을 끄면 지우므로, 그런 방식으로 로그인을 유지하는 사이트는 재시작할 때마다 로그아웃된다.
/// 서버 쪽 세션 만료까지 늘려 주지는 않는다.
#[cfg(windows)]
pub fn persist_session_cookies(view: &tauri::Webview) {
    use webview2_com::GetCookiesCompletedHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_2;
    use windows::core::{Interface, BOOL, HSTRING};

    const KEEP_DAYS: f64 = 30.0;
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
        + KEEP_DAYS * 86400.0;
    let _ = view.with_webview(move |p| {
        let _ = (|| -> windows::core::Result<()> {
            let core = unsafe { p.controller().CoreWebView2()? };
            let mgr = unsafe { core.cast::<ICoreWebView2_2>()?.CookieManager()? };
            let mgr2 = mgr.clone();
            let handler = GetCookiesCompletedHandler::create(Box::new(move |res, list| {
                if res.is_err() {
                    return Ok(());
                }
                let Some(list) = list else { return Ok(()) };
                let mut n = 0u32;
                unsafe { list.Count(&mut n)? };
                for i in 0..n {
                    let c = unsafe { list.GetValueAtIndex(i)? };
                    let mut is_session = BOOL(0);
                    unsafe { c.IsSession(&mut is_session)? };
                    if is_session.as_bool() {
                        unsafe {
                            c.SetExpires(expires)?;
                            let _ = mgr2.AddOrUpdateCookie(&c);
                        }
                    }
                }
                Ok(())
            }));
            // 빈 주소 = 이 프로필의 모든 쿠키
            unsafe { mgr.GetCookies(&HSTRING::new(), &handler) }
        })();
    });
}

#[cfg(not(windows))]
pub fn persist_session_cookies(_view: &tauri::Webview) {}

/// 로그인 유지가 켜진 서비스의 열려 있는 화면 전부에 적용한다(주기 실행용).
pub fn persist_all(app: &AppHandle) {
    for (label, view) in app.webviews() {
        if !label.starts_with("v-") {
            continue;
        }
        let keep = account_of_label(&label).and_then(|id| find_account(app, &id)).map(|a| crate::keep_login(&a)).unwrap_or(false);
        if keep {
            persist_session_cookies(&view);
        }
    }
}

// ───────────────────────── 작업표시줄 배지 아이콘 ─────────────────────────

const FONT: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b001, 0b001, 0b001],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

pub fn badge_icon(count: u32) -> Image<'static> {
    const S: usize = 32;
    let mut px = vec![0u8; S * S * 4];
    let c = (S as f32 - 1.0) / 2.0;
    for y in 0..S {
        for x in 0..S {
            let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
            if d <= 15.5 {
                let i = (y * S + x) * 4;
                px[i..i + 4].copy_from_slice(&[220, 38, 38, 255]);
            }
        }
    }
    let n = count.min(99);
    let text = n.to_string();
    let scale = if text.len() == 1 { 4 } else { 3 };
    let w = text.len() * 3 * scale + (text.len() - 1) * scale;
    let ox = (S - w) / 2;
    let oy = (S - 5 * scale) / 2;
    for (ci, ch) in text.chars().enumerate() {
        let g = FONT[ch.to_digit(10).unwrap() as usize];
        for (ry, row) in g.iter().enumerate() {
            for rx in 0..3 {
                if row & (1 << (2 - rx)) != 0 {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let x = ox + ci * 4 * scale + rx * scale + dx;
                            let y = oy + ry * scale + dy;
                            let i = (y * S + x) * 4;
                            px[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
                        }
                    }
                }
            }
        }
    }
    Image::new_owned(px, S as u32, S as u32)
}
