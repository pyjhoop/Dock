use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Sub {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub is_mail: bool,
    #[serde(default)]
    pub handled: Vec<String>,
    #[serde(default)]
    pub badge_regex: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Recipe {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub login_url: String,
    #[serde(default)]
    pub login_success: Vec<String>,
    #[serde(default)]
    pub logout: Vec<String>,
    #[serde(default)]
    pub chrome_ua_hosts: Vec<String>,
    #[serde(default)]
    pub popout: Vec<String>,
    /// 로그인(OAuth) 팝업으로 보고 opener 관계를 유지한 채 앱 안에서 열 주소 패턴(기본 목록에 추가)
    #[serde(default)]
    pub auth_popup: Vec<String>,
    #[serde(default)]
    pub compose: Option<String>,
    #[serde(default)]
    pub css: Option<String>,
    pub subs: Vec<Sub>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Account {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub handle: String,
    pub service: String,
    #[serde(default)]
    pub custom: Option<Recipe>,
    #[serde(default)]
    pub initials: String,
    #[serde(default = "d_thumb")]
    pub thumb: String,
    #[serde(default)]
    pub picture: Option<String>,
    #[serde(default)]
    pub accent: Option<String>,
    pub subs: Vec<Sub>,
    #[serde(default = "d_true")]
    pub notifications: bool,
    #[serde(default = "d_true")]
    pub badges: bool,
    #[serde(default)]
    pub logged_in: bool,
    /// 로그인 세션(쿠키 폴더) 식별자. 같은 값을 가진 항목은 로그인을 공유한다. 비어 있으면 id와 같다.
    #[serde(default)]
    pub profile: String,
    /// 로그인 유지: 만료 시간이 없는 세션 쿠키를 디스크에 남겨 Dock을 껐다 켜도 로그인을 유지한다.
    /// None이면 직접 입력 서비스만 켜진다.
    #[serde(default)]
    pub keep_login: Option<bool>,
}

fn d_thumb() -> String {
    "initials".into()
}
fn d_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Settings {
    pub startup: String,
    pub close_to_tray: bool,
    pub minimize_to_tray: bool,
    pub notifications: bool,
    pub badges: bool,
    pub categories: Vec<String>,
    pub summary: bool,
    pub layout: String,
    pub theme: String,
    pub accent: String,
    pub colorful: bool,
    pub download_dir: Option<String>,
    pub shortcut: String,
    pub lock_on_hide: bool,
    pub mailto_account: Option<String>,
    /// 시작 후/6시간마다 새 버전을 확인하고 토스트로 알린다
    #[serde(default = "d_true")]
    pub auto_update_check: bool,
    /// 이미 토스트로 알린 버전(같은 버전을 반복 알림하지 않기 위함)
    #[serde(default)]
    pub update_notified: Option<String>,
    #[serde(default)]
    pub passcode_hash: Option<String>,
    #[serde(default)]
    pub passcode_salt: Option<String>,
    /// Claude 도우미(현재 화면 읽기)를 사용할지. 기본 꺼짐.
    #[serde(default)]
    pub ai_enabled: bool,
    /// Claude 모델 별칭(빈 값이면 claude 기본값)
    #[serde(default)]
    pub ai_model: String,
    /// 추론 강도(빈 값이면 claude 기본값)
    #[serde(default)]
    pub ai_effort: String,
    /// Claude가 화면을 조작(클릭·입력)하도록 허용할지. 기본 꺼짐.
    #[serde(default)]
    pub ai_control: bool,
    /// 조작 승인 방식: "always"(매번 묻기) 또는 빈 값(위험한 동작만 묻기)
    #[serde(default)]
    pub ai_confirm: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            startup: "disabled".into(),
            close_to_tray: true,
            minimize_to_tray: false,
            notifications: true,
            badges: true,
            categories: vec!["primary".into()],
            summary: true,
            layout: "side".into(),
            theme: "system".into(),
            accent: "#3182F6".into(),
            colorful: false,
            download_dir: None,
            shortcut: "Ctrl+Alt+M".into(),
            lock_on_hide: true,
            mailto_account: None,
            auto_update_check: true,
            update_notified: None,
            passcode_hash: None,
            passcode_salt: None,
            ai_enabled: false,
            ai_model: String::new(),
            ai_effort: String::new(),
            ai_control: false,
            ai_confirm: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct StoreFile {
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub accounts: Vec<Account>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Download {
    pub id: String,
    pub name: String,
    pub path: String,
    pub url: String,
    pub state: String,
    pub ts: u64,
}

#[derive(Clone, Debug)]
pub struct ViewInfo {
    pub account: String,
    pub sub: i32,
    pub count: u32,
}

pub struct AppState {
    pub store: Mutex<StoreFile>,
    pub path: PathBuf,
    pub data_root: PathBuf,
    pub views: Mutex<HashMap<String, ViewInfo>>,
    pub current: Mutex<Option<String>>,
    pub rect: Mutex<(f64, f64, f64, f64)>,
    pub overlay: AtomicBool,
    pub locked: AtomicBool,
    pub downloads: Mutex<Vec<Download>>,
}

impl AppState {
    pub fn save(&self) {
        let s = self.store.lock().unwrap();
        if let Ok(text) = serde_json::to_string_pretty(&*s) {
            let _ = std::fs::create_dir_all(self.path.parent().unwrap());
            let _ = std::fs::write(&self.path, text);
        }
    }
}
