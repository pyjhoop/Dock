mod ai;
mod model;
mod views;

use model::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, Webview, WindowEvent};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_notification::NotificationExt;

const RECIPES: &str = include_str!("../recipes.json");

pub fn preset_recipes() -> Vec<Recipe> {
    #[derive(serde::Deserialize)]
    struct File {
        services: Vec<Recipe>,
    }
    serde_json::from_str::<File>(RECIPES).map(|f| f.services).unwrap_or_default()
}

/// 이 항목에 적용할 레시피. 제공자(로그인/팝아웃/작성 URL 등) 설정에 이 항목의 서비스 하나만 담는다.
pub fn recipe_for(acc: &Account) -> Recipe {
    if let Some(c) = &acc.custom {
        return c.clone();
    }
    let mut r = preset_recipes()
        .into_iter()
        .find(|r| r.key == acc.service)
        .unwrap_or_else(|| Recipe { key: acc.service.clone(), name: acc.service.clone(), ..Default::default() });
    r.subs = acc.subs.clone();
    r
}

/// 이 항목의 세션 쿠키를 디스크에 남길지. 설정이 없으면 직접 입력 서비스만 켠다.
pub fn keep_login(acc: &Account) -> bool {
    acc.keep_login.unwrap_or_else(|| recipe_for(acc).key == "custom")
}

pub fn profile_of(acc: &Account) -> String {
    if acc.profile.is_empty() { acc.id.clone() } else { acc.profile.clone() }
}

/// 구버전(계정 하나에 서비스 여러 개)을 서비스 단위 항목으로 풀고, 세션은 그대로 공유시킨다.
fn migrate(store: &mut StoreFile) -> bool {
    let mut changed = false;
    let mut out = Vec::new();
    for mut a in store.accounts.drain(..) {
        if !a.profile.is_empty() {
            out.push(a);
            continue;
        }
        a.profile = a.id.clone();
        changed = true;
        if a.subs.len() <= 1 {
            out.push(a);
            continue;
        }
        let mut keep: Vec<Sub> = a.subs.iter().filter(|s| s.pinned).cloned().collect();
        if keep.is_empty() {
            keep.push(a.subs[0].clone());
        }
        for (i, sub) in keep.into_iter().enumerate() {
            let mut e = a.clone();
            if i > 0 {
                e.id = uuid::Uuid::new_v4().simple().to_string();
                e.name = format!("{} · {}", a.name, sub.name);
                e.picture = None;
            }
            e.subs = vec![sub];
            out.push(e);
        }
    }
    store.accounts = out;
    changed
}

fn hash_code(salt: &str, code: &str) -> String {
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(code.as_bytes());
    h.finish_hex()
}

trait FinishHex {
    fn finish_hex(self) -> String;
}
impl FinishHex for Sha256 {
    fn finish_hex(self) -> String {
        self.finalize().iter().map(|b| format!("{:02x}", b)).collect()
    }
}

fn initials_of(name: &str) -> String {
    let parts: Vec<&str> = name.split_whitespace().collect();
    let s: String = if parts.len() >= 2 {
        parts.iter().take(2).filter_map(|p| p.chars().next()).collect()
    } else {
        name.chars().take(2).collect()
    };
    s.to_uppercase()
}

// ───────────────────────── state snapshot ─────────────────────────

fn badge_payload(app: &AppHandle) -> Value {
    let st = app.state::<AppState>();
    let views = st.views.lock().unwrap();
    let store = st.store.lock().unwrap();
    let mut accs = serde_json::Map::new();
    let mut vs = serde_json::Map::new();
    let mut total: u32 = 0;
    for a in &store.accounts {
        let mut sum = 0;
        for (label, v) in views.iter() {
            if v.account == a.id {
                vs.insert(label.clone(), json!(v.count));
                if v.sub >= 0 {
                    sum += v.count;
                }
            }
        }
        accs.insert(a.id.clone(), json!(sum));
        if a.badges {
            total += sum;
        }
    }
    json!({ "accounts": accs, "views": vs, "total": total })
}

fn snapshot(app: &AppHandle) -> Value {
    let st = app.state::<AppState>();
    let store = st.store.lock().unwrap().clone();
    let mut settings = store.settings.clone();
    let has_passcode = settings.passcode_hash.is_some();
    settings.passcode_hash = None;
    settings.passcode_salt = None;
    json!({
        "settings": settings,
        "has_passcode": has_passcode,
        "accounts": store.accounts,
        "recipes": preset_recipes().iter().map(|r| json!({
            "key": r.key, "name": r.name, "color": r.color,
            "services": r.subs.iter().map(|x| json!({"name": x.name, "is_mail": x.is_mail})).collect::<Vec<_>>()
        })).collect::<Vec<_>>(),
        "badges": badge_payload(app),
        "locked": st.locked.load(Ordering::SeqCst),
        "downloads": *st.downloads.lock().unwrap(),
        "current": *st.current.lock().unwrap(),
        "version": app.package_info().version.to_string(),
    })
}

pub fn emit_badges(app: &AppHandle) {
    let payload = badge_payload(app);
    let total = payload["total"].as_u64().unwrap_or(0) as u32;
    let enabled = app.state::<AppState>().store.lock().unwrap().settings.badges;
    let shown = if enabled { total } else { 0 };
    let _ = app.emit("badges", &payload);
    if let Some(w) = app.get_window("main") {
        let _ = w.set_title(&if shown > 0 { format!("({}) Dock", shown) } else { "Dock".into() });
        let icon = if shown > 0 { Some(views::badge_icon(shown)) } else { None };
        let _ = w.set_overlay_icon(icon);
    }
    if let Some(t) = app.tray_by_id("tray") {
        let _ = t.set_tooltip(Some(if shown > 0 { format!("Dock — {} unread", shown) } else { "Dock".into() }));
    }
}

pub(crate) fn log_line(app: &AppHandle, msg: &str) {
    use std::io::Write;
    let dir = app.state::<AppState>().data_root.clone();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("dock.log")) {
        let _ = writeln!(f, "[{}] {}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0), msg);
    }
}

/// 토스트 발신자(AppUserModelID). 앱 identifier(데이터 폴더 이름)와 분리해 두어,
/// 이름을 바꿔도 데이터가 유지되고 Windows의 표시 이름 캐시에 영향받지 않게 한다.
#[cfg(windows)]
const TOAST_AUMID: &str = "dev.laoh.dock";

/// Windows 토스트의 발신자를 HKCU에 등록한다.
/// 등록되지 않은 ID로 보낸 토스트는 Windows가 조용히 버리므로, 설치/개발 실행 여부와 상관없이 필요하다.
#[cfg(windows)]
fn register_aumid(app: &AppHandle) {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let dir = app.state::<AppState>().data_root.clone();
    let _ = std::fs::create_dir_all(&dir);
    let icon = dir.join("icon.png");
    let _ = std::fs::write(&icon, include_bytes!("../icons/128x128.png"));
    let root = RegKey::predef(HKEY_CURRENT_USER);
    // 이전 버전이 등록해 둔 발신자 정보 정리
    let _ = root.delete_subkey_all(format!("Software\\Classes\\AppUserModelId\\{}", app.config().identifier));
    match root.create_subkey(format!("Software\\Classes\\AppUserModelId\\{}", TOAST_AUMID)) {
        Ok((k, _)) => {
            let _ = k.set_value("DisplayName", &"Dock");
            let _ = k.set_value("IconUri", &icon.to_string_lossy().to_string());
            let _ = k.set_value("ShowInSettings", &1u32);
        }
        Err(e) => log_line(app, &format!("AUMID 등록 실패: {}", e)),
    }
}

/// 토스트를 보낸다. 본문을 누르면 on_click(None), 버튼을 누르면 on_click(Some(action))이 호출된다.
/// (Windows가 아니면 버튼/클릭 동작 없음)
pub fn notify_with(
    app: &AppHandle,
    title: &str,
    body: &str,
    buttons: &[(&str, &str)],
    on_click: Option<Box<dyn Fn(Option<String>) + Send + 'static>>,
) {
    #[cfg(windows)]
    {
        let mut toast = tauri_winrt_notification::Toast::new(TOAST_AUMID).title(title).text1(body);
        for (label, action) in buttons {
            toast = toast.add_button(label, action);
        }
        let toast = toast.on_activated(move |action| {
            if let Some(f) = &on_click {
                f(action.filter(|a| !a.is_empty()));
            }
            Ok(())
        });
        match toast.show() {
            Ok(()) => log_line(app, &format!("토스트 표시: {}", title)),
            Err(e) => {
                log_line(app, &format!("토스트 실패({}): {} — 플러그인으로 재시도", e, title));
                let _ = app.notification().builder().title(title).body(body).show();
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (buttons, on_click);
        let _ = app.notification().builder().title(title).body(body).show();
    }
}

/// 서비스 알림. `target`(서비스 항목 id)이 있으면 눌렀을 때 창을 열고 그 서비스로 이동한다.
pub fn notify(app: &AppHandle, title: &str, body: &str, target: Option<&str>) {
    let handle = app.clone();
    let target_id = target.map(String::from);
    notify_with(
        app,
        title,
        body,
        &[],
        Some(Box::new(move |_| {
            show_main(&handle);
            if let Some(acc) = target_id.clone() {
                let h = handle.clone();
                // 웹뷰 생성/전환은 메인 스레드가 아닌 곳에서 해야 한다
                std::thread::spawn(move || {
                    if views::show(&h, &acc, 0, None).is_ok() {
                        let _ = h.emit("route", json!({ "account": acc, "sub": 0 }));
                    }
                });
            }
        })),
    );
}

// ───────────────────────── 업데이트 ─────────────────────────

const UPDATE_API: &str = "https://api.github.com/repos/pyjhoop/Dock/releases/latest";
/// 설치 파일을 내려받을 수 있는 호스트(GitHub 릴리즈). DOCK_UPDATE_URL로 테스트할 때만 제한을 푼다.
const DOWNLOAD_HOSTS: [&str; 3] = ["github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com"];
const MAX_INSTALLER_BYTES: u64 = 300 * 1024 * 1024;

static INSTALLING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(serde::Serialize, Clone)]
pub struct UpdateInfo {
    current: String,
    latest: String,
    available: bool,
    url: String,
    notes: String,
    /// 설치 파일과 SHA-256이 모두 확인되어 앱 안에서 바로 설치할 수 있는지
    can_install: bool,
    asset_name: String,
    size: u64,
}

#[derive(Clone)]
struct Asset {
    name: String,
    url: String,
    sha256: Option<String>,
    size: u64,
}

fn parse_ver(s: &str) -> Vec<u64> {
    s.trim_start_matches(|c| c == 'v' || c == 'V')
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

fn is_newer(latest: &str, current: &str) -> bool {
    let (a, b) = (parse_ver(latest), parse_ver(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (*a.get(i).unwrap_or(&0), *b.get(i).unwrap_or(&0));
        if x != y {
            return x > y;
        }
    }
    false
}

fn update_url_override() -> Option<String> {
    std::env::var("DOCK_UPDATE_URL").ok().filter(|u| !u.is_empty())
}

/// GitHub 최신 릴리즈를 조회한다 (블로킹: 메인 스레드에서 호출하지 말 것).
fn fetch_release(app: &AppHandle) -> Result<(UpdateInfo, Option<Asset>), String> {
    let url = update_url_override().unwrap_or_else(|| UPDATE_API.to_string());
    let resp = ureq::get(&url)
        .set("User-Agent", "Dock-updater")
        .set("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(15))
        .call();
    let v: Value = match resp {
        Ok(r) => r.into_json().map_err(|e| format!("응답을 읽지 못했어요: {}", e))?,
        Err(ureq::Error::Status(404, _)) => return Err("릴리즈를 찾을 수 없어요. 저장소가 비공개이거나 아직 릴리즈가 없어요".into()),
        Err(ureq::Error::Status(403, _)) | Err(ureq::Error::Status(429, _)) => return Err("요청이 너무 많아요. 잠시 후 다시 시도해 주세요".into()),
        Err(ureq::Error::Status(code, _)) => return Err(format!("서버 오류({})", code)),
        Err(e) => return Err(format!("네트워크에 연결하지 못했어요: {}", e)),
    };
    let tag = v["tag_name"].as_str().ok_or("릴리즈 정보에 버전이 없어요")?.to_string();
    let current = app.package_info().version.to_string();
    // 설치 파일(x64 setup.exe)과 GitHub가 계산한 SHA-256
    let asset = v["assets"].as_array().and_then(|list| {
        list.iter()
            .find(|a| {
                let n = a["name"].as_str().unwrap_or("").to_lowercase();
                n.ends_with("-setup.exe") || (n.ends_with(".exe") && n.contains("setup"))
            })
            .map(|a| Asset {
                name: a["name"].as_str().unwrap_or("").to_string(),
                url: a["browser_download_url"].as_str().unwrap_or("").to_string(),
                sha256: a["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(|h| h.to_lowercase()),
                size: a["size"].as_u64().unwrap_or(0),
            })
    });
    let can_install = asset.as_ref().map(|a| !a.url.is_empty() && a.sha256.is_some()).unwrap_or(false);
    let info = UpdateInfo {
        available: is_newer(&tag, &current),
        latest: tag.trim_start_matches('v').to_string(),
        current,
        url: v["html_url"].as_str().unwrap_or("https://github.com/pyjhoop/Dock/releases").to_string(),
        notes: v["body"].as_str().unwrap_or("").chars().take(600).collect(),
        can_install,
        asset_name: asset.as_ref().map(|a| a.name.clone()).unwrap_or_default(),
        size: asset.as_ref().map(|a| a.size).unwrap_or(0),
    };
    Ok((info, asset))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// 설치 파일을 내려받아 SHA-256과 형식을 검증하고, 설치 프로그램을 실행한 뒤 앱을 종료한다. (블로킹)
fn install_update(app: &AppHandle) -> Result<(), String> {
    use std::io::{Read, Write};
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err("이미 업데이트를 진행하고 있어요".into());
    }
    let result = (|| -> Result<(), String> {
        // UI가 보여 준 값이 아니라 지금 다시 조회한 값만 믿는다
        let (info, asset) = fetch_release(app)?;
        if !info.available {
            return Err("이미 최신 버전이에요".into());
        }
        let asset = asset.ok_or("릴리즈에 설치 파일이 없어요")?;
        let expected = asset.sha256.clone().ok_or("무결성 정보(SHA-256)가 없어 자동 설치하지 않아요. 릴리즈 페이지에서 받아 주세요")?;
        let parsed = url::Url::parse(&asset.url).map_err(|_| "설치 파일 주소가 올바르지 않아요".to_string())?;
        if update_url_override().is_none() {
            let host_ok = parsed.scheme() == "https" && parsed.host_str().map(|h| DOWNLOAD_HOSTS.contains(&h)).unwrap_or(false);
            if !host_ok {
                return Err("신뢰할 수 없는 다운로드 주소라 설치하지 않아요".into());
            }
        }
        if asset.size > MAX_INSTALLER_BYTES {
            return Err("설치 파일이 비정상적으로 커서 설치하지 않아요".into());
        }

        let dir = std::env::temp_dir().join("Dock-update");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let safe_name = std::path::Path::new(&asset.name).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Dock-setup.exe".into());
        let path = dir.join(safe_name);

        // 전체 시간 제한은 두지 않고(큰 파일), 연결/읽기 정체만 끊는다
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(10))
            .timeout_read(std::time::Duration::from_secs(30))
            .build();
        let resp = agent
            .get(&asset.url)
            .set("User-Agent", "Dock-updater")
            .call()
            .map_err(|e| format!("다운로드하지 못했어요: {}", e))?;
        let total = resp.header("Content-Length").and_then(|v| v.parse::<u64>().ok()).unwrap_or(asset.size);
        if total > MAX_INSTALLER_BYTES {
            return Err("설치 파일이 비정상적으로 커서 설치하지 않아요".into());
        }
        let mut reader = resp.into_reader().take(MAX_INSTALLER_BYTES + 1);
        let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        let (mut done, mut last_pct) = (0u64, 101u64);
        loop {
            let n = reader.read(&mut buf).map_err(|e| format!("다운로드 중 끊겼어요: {}", e))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            done += n as u64;
            if done > MAX_INSTALLER_BYTES {
                return Err("설치 파일이 비정상적으로 커서 설치하지 않아요".into());
            }
            let pct = if total > 0 { done * 100 / total } else { 0 };
            if pct != last_pct {
                last_pct = pct;
                let _ = app.emit("update-progress", json!({ "done": done, "total": total, "percent": pct }));
            }
        }
        drop(file);

        // 무결성: GitHub가 릴리즈 등록 시 계산한 SHA-256과 일치해야 한다
        let actual = hex(&hasher.finalize());
        if actual != expected {
            let _ = std::fs::remove_file(&path);
            log_line(app, &format!("업데이트 중단: SHA-256 불일치 (기대 {} / 실제 {})", expected, actual));
            return Err("내려받은 파일이 손상되었거나 변조되어 설치하지 않았어요 (SHA-256 불일치)".into());
        }
        // 실행 파일 형식(MZ)도 확인
        let mut head = [0u8; 2];
        std::fs::File::open(&path).and_then(|mut f| f.read_exact(&mut head)).map_err(|e| e.to_string())?;
        if &head != b"MZ" {
            let _ = std::fs::remove_file(&path);
            return Err("설치 파일 형식이 올바르지 않아요".into());
        }

        log_line(app, &format!("업데이트 설치 시작: {} (v{} → v{})", asset.name, info.current, info.latest));
        let _ = app.emit("update-installing", json!({ "version": info.latest }));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // /P 진행 창만 표시, /UPDATE 업데이트 모드(제거 확인 없이 덮어쓰기), /R 설치 후 앱 재실행
            std::process::Command::new(&path)
                .args(["/P", "/UPDATE", "/R"])
                .creation_flags(0x0000_0008 | 0x0000_0200) // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
                .spawn()
                .map_err(|e| format!("설치 프로그램을 실행하지 못했어요: {}", e))?;
            // 설치 프로그램이 실행 파일을 교체할 수 있도록 앱을 종료한다
            std::thread::sleep(std::time::Duration::from_millis(700));
            app.exit(0);
            Ok(())
        }
        #[cfg(not(windows))]
        {
            Err("이 운영체제에서는 자동 설치를 지원하지 않아요".into())
        }
    })();
    if result.is_err() {
        INSTALLING.store(false, Ordering::SeqCst);
    }
    result
}

/// 확인 후 새 버전이 있으면 토스트를 띄운다. 자동 확인은 같은 버전을 한 번만 알린다.
fn check_and_notify(app: &AppHandle, manual: bool) -> Result<UpdateInfo, String> {
    let (info, _) = fetch_release(app)?;
    if info.available {
        let st = app.state::<AppState>();
        let already = st.store.lock().unwrap().settings.update_notified.as_deref() == Some(info.latest.as_str());
        if manual || !already {
            let h = app.clone();
            let buttons: Vec<(&str, &str)> = if info.can_install { vec![("지금 업데이트", "update")] } else { vec![] };
            notify_with(
                app,
                "Dock 업데이트",
                &format!("새 버전 v{}이 나왔어요.", info.latest),
                &buttons,
                Some(Box::new(move |action| {
                    if action.as_deref() == Some("update") {
                        // 버튼: 바로 설치
                        let h2 = h.clone();
                        std::thread::spawn(move || {
                            if let Err(e) = install_update(&h2) {
                                log_line(&h2, &format!("업데이트 실패: {}", e));
                                notify_with(&h2, "Dock 업데이트 실패", &e, &[], None);
                            }
                        });
                    } else {
                        // 본문: 창을 열고 설정 > 정보로 안내
                        show_main(&h);
                        let _ = h.emit("open-settings", json!({ "section": "about" }));
                    }
                })),
            );
            st.store.lock().unwrap().settings.update_notified = Some(info.latest.clone());
            st.save();
        }
    }
    log_line(app, &format!("업데이트 확인: 현재 {} / 최신 {} / 있음={} / 앱 안 설치 가능={}", info.current, info.latest, info.available, info.can_install));
    Ok(info)
}

#[tauri::command]
async fn check_update(app: AppHandle) -> Result<UpdateInfo, String> {
    tauri::async_runtime::spawn_blocking(move || check_and_notify(&app, true))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn install_update_cmd(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || install_update(&app))
        .await
        .map_err(|e| e.to_string())?
}

// ───────────────────────── commands ─────────────────────────

#[tauri::command]
async fn get_state(app: AppHandle) -> Value {
    snapshot(&app)
}

#[tauri::command]
async fn add_account(
    app: AppHandle,
    state: State<'_, AppState>,
    service: String,
    service_name: Option<String>,
    name: String,
    custom_url: Option<String>,
    profile: Option<String>,
) -> Result<Value, String> {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let (custom, sub) = if service == "custom" {
        let url = custom_url.unwrap_or_default();
        let parsed = url::Url::parse(&url).map_err(|_| "URL이 올바르지 않습니다".to_string())?;
        let host = parsed.host_str().unwrap_or("").replace('.', "\\.");
        let sub = Sub { name: name.clone(), url: url.clone(), pinned: true, is_mail: false, handled: vec![format!("https?://{}", host)], badge_regex: None };
        let r = Recipe { key: "custom".into(), name: name.clone(), color: "#475569".into(), login_url: url, subs: vec![sub.clone()], ..Default::default() };
        (Some(r), sub)
    } else {
        let r = preset_recipes().into_iter().find(|r| r.key == service).ok_or("알 수 없는 서비스")?;
        let wanted = service_name.unwrap_or_default();
        let sub = r.subs.iter().find(|s| s.name == wanted).cloned().ok_or("알 수 없는 서비스")?;
        (None, sub)
    };
    // 기존 세션을 고르면 로그인을 공유하므로 바로 열 수 있다
    let shared = profile.filter(|p| state.store.lock().unwrap().accounts.iter().any(|a| profile_of(a) == *p));
    let acc = Account {
        id: id.clone(),
        initials: initials_of(&name),
        name,
        handle: String::new(),
        service,
        custom,
        thumb: "logo".into(),
        picture: None,
        accent: None,
        subs: vec![sub],
        notifications: true,
        badges: true,
        logged_in: shared.is_some(),
        profile: shared.unwrap_or_else(|| id.clone()),
        keep_login: None,
    };
    let logged_in = acc.logged_in;
    state.store.lock().unwrap().accounts.push(acc);
    state.save();
    if logged_in {
        views::preload_account(&app, &id);
    }
    Ok(json!({ "id": id, "state": snapshot(&app) }))
}

#[tauri::command]
async fn update_account(app: AppHandle, state: State<'_, AppState>, id: String, patch: Value) -> Result<Value, String> {
    {
        let mut s = state.store.lock().unwrap();
        let a = s.accounts.iter_mut().find(|a| a.id == id).ok_or("계정 없음")?;
        if let Some(v) = patch["name"].as_str() {
            a.name = v.into();
        }
        if let Some(v) = patch["handle"].as_str() {
            a.handle = v.into();
        }
        if let Some(v) = patch["initials"].as_str() {
            a.initials = v.chars().take(3).collect();
        }
        if let Some(v) = patch["thumb"].as_str() {
            a.thumb = v.into();
        }
        if patch.get("picture").is_some() {
            a.picture = patch["picture"].as_str().map(String::from);
        }
        if patch.get("accent").is_some() {
            a.accent = patch["accent"].as_str().map(String::from);
        }
        if let Some(v) = patch["notifications"].as_bool() {
            a.notifications = v;
        }
        if let Some(v) = patch["badges"].as_bool() {
            a.badges = v;
        }
        if let Some(v) = patch["logged_in"].as_bool() {
            a.logged_in = v;
        }
        if let Some(v) = patch["keep_login"].as_bool() {
            a.keep_login = Some(v);
        }
        if let Some(arr) = patch["pinned"].as_array() {
            for (i, p) in arr.iter().enumerate() {
                if let (Some(sub), Some(b)) = (a.subs.get_mut(i), p.as_bool()) {
                    sub.pinned = b;
                }
            }
        }
    }
    state.save();
    emit_badges(&app);
    Ok(snapshot(&app))
}

#[tauri::command]
async fn remove_account(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<Value, String> {
    views::close_account_views(&app, &id);
    let profile = state.store.lock().unwrap().accounts.iter().find(|a| a.id == id).map(profile_of);
    state.store.lock().unwrap().accounts.retain(|a| a.id != id);
    state.save();
    if let Some(profile) = profile {
        let still_used = state.store.lock().unwrap().accounts.iter().any(|a| profile_of(a) == profile);
        if !still_used {
            let dir = state.data_root.join("profiles").join(&profile);
            std::thread::spawn(move || {
                // WebView2가 파일을 놓을 때까지 잠시 기다린 뒤 세션 폴더 삭제
                std::thread::sleep(std::time::Duration::from_secs(2));
                let _ = std::fs::remove_dir_all(dir);
            });
        }
    }
    emit_badges(&app);
    Ok(snapshot(&app))
}

#[tauri::command]
async fn reorder_accounts(app: AppHandle, state: State<'_, AppState>, ids: Vec<String>) -> Result<Value, String> {
    {
        let mut s = state.store.lock().unwrap();
        s.accounts.sort_by_key(|a| ids.iter().position(|i| *i == a.id).unwrap_or(usize::MAX));
    }
    state.save();
    Ok(snapshot(&app))
}

#[tauri::command]
async fn set_settings(app: AppHandle, state: State<'_, AppState>, patch: Value) -> Result<Value, String> {
    {
        let mut s = state.store.lock().unwrap();
        let mut cur = serde_json::to_value(&s.settings).map_err(|e| e.to_string())?;
        if let (Some(c), Some(p)) = (cur.as_object_mut(), patch.as_object()) {
            for (k, v) in p {
                if k != "passcode_hash" && k != "passcode_salt" {
                    c.insert(k.clone(), v.clone());
                }
            }
        }
        s.settings = serde_json::from_value(cur).map_err(|e| e.to_string())?;
    }
    state.save();
    apply_system_settings(&app);
    emit_badges(&app);
    Ok(snapshot(&app))
}

#[tauri::command]
async fn show_view(app: AppHandle, account_id: String, sub: i32, url: Option<String>) -> Result<(), String> {
    views::show(&app, &account_id, sub, url)
}

#[tauri::command]
async fn set_rect(app: AppHandle, x: f64, y: f64, w: f64, h: f64) {
    views::set_rect(&app, (x, y, w.max(1.0), h.max(1.0)));
}

#[tauri::command]
async fn set_overlay(app: AppHandle, on: bool) {
    app.state::<AppState>().overlay.store(on, Ordering::SeqCst);
    views::apply_visibility(&app);
}

/// 폴더 선택 창. tauri-plugin-dialog는 외부 웹페이지의 alert/confirm까지 가로채 막아 버리므로 쓰지 않는다.
#[tauri::command]
async fn pick_folder() -> Option<String> {
    rfd::AsyncFileDialog::new().pick_folder().await.map(|h| h.path().to_string_lossy().to_string())
}

#[tauri::command]
async fn reload_view(app: AppHandle) {
    views::nav(&app, "reload");
}

#[tauri::command]
async fn view_nav(app: AppHandle, action: String) {
    views::nav(&app, &action);
}

#[tauri::command]
async fn finish_login(app: AppHandle, state: State<'_, AppState>, account_id: String) -> Result<Value, String> {
    views::close_login(&app, &account_id);
    {
        let mut s = state.store.lock().unwrap();
        if let Some(a) = s.accounts.iter_mut().find(|a| a.id == account_id) {
            a.logged_in = true;
        }
    }
    state.save();
    views::preload_account(&app, &account_id);
    Ok(snapshot(&app))
}

#[tauri::command]
async fn set_passcode(app: AppHandle, state: State<'_, AppState>, code: String, old: Option<String>) -> Result<Value, String> {
    if code.len() < 4 {
        return Err("패스코드는 4자 이상이어야 합니다".into());
    }
    {
        let mut s = state.store.lock().unwrap();
        if let (Some(h), Some(salt)) = (&s.settings.passcode_hash, &s.settings.passcode_salt) {
            if hash_code(salt, &old.unwrap_or_default()) != *h {
                return Err("현재 패스코드가 일치하지 않습니다".into());
            }
        }
        let salt: String = (0..16).map(|_| format!("{:02x}", rand::random::<u8>())).collect();
        s.settings.passcode_hash = Some(hash_code(&salt, &code));
        s.settings.passcode_salt = Some(salt);
    }
    state.save();
    Ok(snapshot(&app))
}

#[tauri::command]
async fn clear_passcode(app: AppHandle, state: State<'_, AppState>, code: String) -> Result<Value, String> {
    {
        let mut s = state.store.lock().unwrap();
        if let (Some(h), Some(salt)) = (&s.settings.passcode_hash, &s.settings.passcode_salt) {
            if hash_code(salt, &code) != *h {
                return Err("패스코드가 일치하지 않습니다".into());
            }
        }
        s.settings.passcode_hash = None;
        s.settings.passcode_salt = None;
    }
    state.locked.store(false, Ordering::SeqCst);
    state.save();
    views::apply_visibility(&app);
    Ok(snapshot(&app))
}

#[tauri::command]
async fn unlock(app: AppHandle, state: State<'_, AppState>, code: String) -> Result<bool, String> {
    let ok = {
        let s = state.store.lock().unwrap();
        match (&s.settings.passcode_hash, &s.settings.passcode_salt) {
            (Some(h), Some(salt)) => hash_code(salt, &code) == *h,
            _ => true,
        }
    };
    if ok {
        state.locked.store(false, Ordering::SeqCst);
        views::apply_visibility(&app);
        let _ = app.emit("locked", false);
    }
    Ok(ok)
}

pub fn lock(app: &AppHandle) {
    let st = app.state::<AppState>();
    if st.store.lock().unwrap().settings.passcode_hash.is_some() {
        st.locked.store(true, Ordering::SeqCst);
        views::apply_visibility(app);
        let _ = app.emit("locked", true);
    }
}

#[tauri::command]
async fn lock_app(app: AppHandle) {
    lock(&app);
}

#[tauri::command]
async fn list_downloads(state: State<'_, AppState>) -> Result<Vec<Download>, String> {
    Ok(state.downloads.lock().unwrap().clone())
}

#[tauri::command]
async fn download_action(app: AppHandle, state: State<'_, AppState>, id: String, action: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let item = state.downloads.lock().unwrap().iter().find(|d| d.id == id).cloned().ok_or("항목 없음")?;
    match action.as_str() {
        "open" => app.opener().open_path(&item.path, None::<&str>).map_err(|e| e.to_string())?,
        "folder" => app.opener().reveal_item_in_dir(&item.path).map_err(|e| e.to_string())?,
        "delete" => {
            let _ = std::fs::remove_file(&item.path);
            state.downloads.lock().unwrap().retain(|d| d.id != id);
        }
        "remove" => state.downloads.lock().unwrap().retain(|d| d.id != id),
        _ => {}
    }
    let _ = app.emit("downloads", &*state.downloads.lock().unwrap());
    Ok(())
}

#[tauri::command]
async fn clear_downloads(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.downloads.lock().unwrap().retain(|d| d.state == "progress");
    let _ = app.emit("downloads", &*state.downloads.lock().unwrap());
    Ok(())
}

#[tauri::command]
async fn open_external(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("허용되지 않은 URL".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
async fn test_notification(app: AppHandle) {
    notify(&app, "Dock", "테스트 알림입니다. 이 알림이 보이면 알림 설정이 정상입니다. 눌러 보세요.", None);
}

#[tauri::command]
async fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// 원격(웹) 페이지에서 오는 이벤트. 이 커맨드만 원격 capability에 허용되어 있다.
#[tauri::command]
async fn report_event(app: AppHandle, webview: Webview, event: Value) -> Result<(), String> {
    let label = webview.label().to_string();
    let acc_id = views::account_of_label(&label).ok_or("unknown webview")?;
    let (settings, acc) = {
        let st = app.state::<AppState>();
        let s = st.store.lock().unwrap();
        (s.settings.clone(), s.accounts.iter().find(|a| a.id == acc_id).cloned())
    };
    let acc = acc.ok_or("unknown account")?;
    if !settings.notifications || !acc.notifications {
        log_line(&app, &format!("이벤트 무시(알림 꺼짐): {}", acc.name));
        return Ok(());
    }
    log_line(&app, &format!("이벤트 수신: {} / {}", acc.name, event["kind"].as_str().unwrap_or("?")));
    match event["kind"].as_str() {
        Some("notification") => {
            let t = event["title"].as_str().unwrap_or("").chars().take(120).collect::<String>();
            let b = event["body"].as_str().unwrap_or("").chars().take(300).collect::<String>();
            notify(&app, &format!("{} · {}", acc.name, t), &b, Some(&acc.id));
        }
        Some("mail") => {
            let items: Vec<&Value> = event["items"]
                .as_array()
                .map(|a| a.iter().filter(|i| settings.categories.iter().any(|c| Some(c.as_str()) == i["category"].as_str())).collect())
                .unwrap_or_default();
            if items.is_empty() {
                log_line(&app, "메일 이벤트: 선택된 카테고리에 해당하는 새 메일 없음");
                return Ok(());
            }
            if settings.summary && items.len() > 3 {
                notify(&app, &acc.name, &format!("새 메일 {}통", items.len()), Some(&acc.id));
            } else {
                for i in items.iter().take(5) {
                    let from = i["from"].as_str().unwrap_or("");
                    let title = i["title"].as_str().unwrap_or("");
                    let sum = i["summary"].as_str().unwrap_or("");
                    notify(&app, &format!("{} · {}", acc.name, from), &format!("{}\n{}", title, sum), Some(&acc.id));
                }
            }
        }
        _ => {}
    }
    Ok(())
}

// ───────────────────────── AI (Claude CLI, 읽기 전용) ─────────────────────────

#[tauri::command]
async fn ai_status(app: AppHandle) -> Value {
    ai::status(&app)
}

#[tauri::command]
async fn ai_ask(app: AppHandle, prompt: String) -> Result<(), String> {
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return Err("질문을 입력해 주세요".into());
    }
    ai::ask(&app, prompt)
}

#[tauri::command]
async fn ai_cancel() {
    ai::cancel();
}

#[tauri::command]
async fn ai_reset() {
    ai::reset();
}

#[tauri::command]
async fn ai_confirm(id: u64, allow: bool) {
    ai::answer(id, allow);
}

// ───────────────────────── system integration ─────────────────────────

/// DOCK_DATA_DIR이 지정된 테스트/개발 실행인지. 이때는 OS 전역 설정(자동 시작, 전역 단축키)을 건드리지 않는다.
fn is_isolated_run() -> bool {
    std::env::var("DOCK_DATA_DIR").map(|v| !v.is_empty()).unwrap_or(false)
}

fn apply_system_settings(app: &AppHandle) {
    if is_isolated_run() {
        return;
    }
    let (startup, shortcut) = {
        let st = app.state::<AppState>();
        let s = st.store.lock().unwrap();
        (s.settings.startup.clone(), s.settings.shortcut.clone())
    };
    let al = app.autolaunch();
    if startup == "disabled" {
        let _ = al.disable();
    } else {
        let _ = al.enable();
    }
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    if !shortcut.trim().is_empty() {
        let _ = gs.on_shortcut(shortcut.as_str(), |app, _sc, ev| {
            if ev.state == ShortcutState::Pressed {
                toggle_main(app);
            }
        });
    }
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn toggle_main(app: &AppHandle) {
    if let Some(w) = app.get_window("main") {
        if w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false) {
            hide_main(app);
        } else {
            show_main(app);
        }
    }
}

fn hide_main(app: &AppHandle) {
    if let Some(w) = app.get_window("main") {
        let _ = w.hide();
    }
    let lock_on_hide = app.state::<AppState>().store.lock().unwrap().settings.lock_on_hide;
    if lock_on_hide {
        lock(app);
    }
}

fn handle_args(app: &AppHandle, args: &[String]) {
    for a in args {
        if a.to_lowercase().starts_with("mailto:") {
            show_main(app);
            let app2 = app.clone();
            let a2 = a.clone();
            std::thread::spawn(move || {
                let _ = views::compose(&app2, None, Some(a2));
            });
        }
    }
}

pub fn run() {
    let mut builder = tauri::Builder::default();
    // 격리 실행(DOCK_DATA_DIR)은 실제 사용 중인 Dock과 별개로 뜨도록 단일 인스턴스 제한을 두지 않는다
    if !is_isolated_run() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            show_main(app);
            handle_args(app, &args);
        }));
    }
    let builder = builder
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--startup"])));

    builder
        .invoke_handler(tauri::generate_handler![
            get_state, add_account, update_account, remove_account, reorder_accounts, set_settings,
            show_view, set_rect, set_overlay, reload_view, view_nav, set_passcode,
            clear_passcode, unlock, lock_app, list_downloads, download_action, clear_downloads,
            report_event, quit_app, finish_login, open_external, pick_folder, test_notification, check_update, install_update_cmd,
            ai_status, ai_ask, ai_cancel, ai_reset, ai_confirm
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            // 테스트/개발용: DOCK_DATA_DIR을 지정하면 실제 사용자 데이터(%APPDATA%)를 건드리지 않는다
            let data_root = match std::env::var("DOCK_DATA_DIR").ok().filter(|v| !v.is_empty()) {
                Some(dir) => std::path::PathBuf::from(dir),
                None => app.path().app_data_dir()?,
            };
            let path = data_root.join("store.json");
            let mut store: StoreFile = std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default();
            let migrated = migrate(&mut store);
            let locked = store.settings.passcode_hash.is_some();
            let startup = store.settings.startup.clone();
            app.manage(AppState {
                store: Mutex::new(store),
                path,
                data_root,
                views: Mutex::new(Default::default()),
                current: Mutex::new(None),
                rect: Mutex::new((72.0, 40.0, 1000.0, 700.0)),
                overlay: Default::default(),
                locked: std::sync::atomic::AtomicBool::new(locked),
                downloads: Mutex::new(vec![]),
            });
            if migrated {
                // 이전된 항목 id가 실행마다 바뀌지 않도록 바로 저장
                handle.state::<AppState>().save();
            }

            #[cfg(windows)]
            register_aumid(&handle);

            ai::start_server(&handle);

            // 트레이
            let open = MenuItem::with_id(app, "open", "Dock 열기", true, None::<&str>)?;
            let compose = MenuItem::with_id(app, "compose", "새 메일", true, None::<&str>)?;
            let lock_i = MenuItem::with_id(app, "lock", "잠금", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &compose, &lock_i, &quit])?;
            let mut tb = TrayIconBuilder::with_id("tray")
                .tooltip("Dock")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| match ev.id.as_ref() {
                    "open" => show_main(app),
                    "compose" => {
                        show_main(app);
                        let a = app.clone();
                        std::thread::spawn(move || {
                            let _ = views::compose(&a, None, None);
                        });
                    }
                    "lock" => lock(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tb = tb.icon(icon.clone());
            }
            tb.build(app)?;

            // 창 이벤트 (닫기→트레이, 최소화→트레이)
            if let Some(w) = app.get_window("main") {
                let h = handle.clone();
                w.on_window_event(move |ev| match ev {
                    WindowEvent::CloseRequested { api, .. } => {
                        let to_tray = h.state::<AppState>().store.lock().unwrap().settings.close_to_tray;
                        if to_tray {
                            api.prevent_close();
                            hide_main(&h);
                        }
                    }
                    WindowEvent::Resized(_) => {
                        if let Some(w) = h.get_window("main") {
                            if w.is_minimized().unwrap_or(false) && h.state::<AppState>().store.lock().unwrap().settings.minimize_to_tray {
                                hide_main(&h);
                            }
                        }
                    }
                    _ => {}
                });
            }

            apply_system_settings(&handle);

            let args: Vec<String> = std::env::args().collect();
            let started_by_os = args.iter().any(|a| a == "--startup");
            if !(started_by_os && startup == "tray") {
                show_main(&handle);
            } else if locked {
                // 트레이로 시작해도 잠금 상태 유지
            }
            // 메일 서비스를 백그라운드에 미리 로드해 배지/알림이 동작하게 한다
            let h2 = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(1200));
                views::preload_all(&h2);
                handle_args(&h2, &args);
            });
            // 로그인 유지: 세션 쿠키를 주기적으로 저장 가능한 쿠키로 바꾼다(페이지 로드 때도 실행됨)
            let h4 = handle.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(30));
                views::persist_all(&h4);
            });
            // 새 버전 확인: 시작 30초 뒤, 이후 6시간마다 (설정에서 끌 수 있음)
            let h3 = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(30));
                loop {
                    if h3.state::<AppState>().store.lock().unwrap().settings.auto_update_check {
                        if let Err(e) = check_and_notify(&h3, false) {
                            log_line(&h3, &format!("업데이트 확인 실패: {}", e));
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_secs(6 * 3600));
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Dock");
}
