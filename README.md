# Dock

메일·캘린더·메신저 같은 웹 서비스를 서비스 단위로 한곳에 모아 두는 **개인용** 데스크톱 독 (Tauri 2 + Rust).
서비스마다 로그인 세션을 분리하거나 공유할 수 있고, 배지·알림·트레이·잠금을 얹었다.
광고·결제·원격 분석 없음, 모든 기능 상시 활성.

## 실행 / 빌드
```
cd src-tauri
cargo run --features custom-protocol        # 개발 실행 (프런트엔드 내장)
cargo install tauri-cli --version "^2"      # 최초 1회
cargo tauri build                           # NSIS 설치 파일 (target/release/bundle/nsis)
```
요구사항: Rust(MSVC), WebView2 런타임(Windows 11 기본 포함).

개발·테스트 중 실제 사용자 데이터를 건드리지 않으려면 `DOCK_DATA_DIR=<임시 폴더>`를 지정해 실행한다.

## 구조
- `src/` — UI (바닐라 HTML/CSS/JS, 빌드 단계 없음)
- `src-tauri/recipes.json` — 서비스 레시피(Google, Microsoft, Naver, Daum, Yahoo, Slack, Notion, Discord, Telegram). 서비스를 추가하려면 항목만 넣으면 된다
- `src-tauri/inject.js` — 웹페이지에 주입: 웹 알림 → 토스트, Gmail Atom 피드 폴링, 구글 로그인 Passkey 차단
- `src-tauri/src/views.rs` — 서비스별 격리 웹뷰(프로필 폴더 분리), 링크 라우팅, 팝아웃, 다운로드, 배지
- `src-tauri/src/lib.rs` — 커맨드, 트레이, 자동시작, 전역 단축키, 패스코드, 알림, 업데이트
- 데이터: `%APPDATA%\dev.laoh.mymail\` (`store.json`, `profiles\<세션>\` = 쿠키/세션)

## 서비스 단위 구조
- 사이드바의 항목 하나 = 서비스 하나 (예: Gmail, Google Calendar, Slack). 같은 서비스를 여러 번 추가할 수 있다.
- 추가할 때 **새로 로그인** 또는 **기존 로그인 공유**를 고른다. 공유하면 같은 쿠키 폴더를 써서 다시 로그인하지 않아도 된다.
- 항목을 삭제해도 같은 로그인을 쓰는 다른 항목이 남아 있으면 로그인 정보는 지우지 않는다.

## 알림
- 웹 알림(`new Notification`, 서비스 워커 `showNotification`)을 네이티브 토스트로 보낸다. Gmail은 Atom 피드를 1분마다 읽어 새 메일을 알린다.
- 토스트를 누르면 창을 열고 해당 서비스로 이동한다.
- 토스트 발신자는 전용 ID(`dev.laoh.dock`)로 HKCU에 등록한다.

## 보안 모델
UI 창만 모든 커맨드를 호출할 수 있다. 외부 웹페이지(`v-*`, `pop-*`)는 `report_event` 하나만 허용한다(`capabilities/remote.json`).

## mailto 기본 앱
`powershell -ExecutionPolicy Bypass -File register-mailto.ps1 -Exe <dock.exe 경로>` 후 Windows 기본 앱에서 MAILTO를 Dock으로 지정.

## 업데이트
- 설정 > 정보에서 **지금 확인**을 누르거나, 시작 30초 뒤와 6시간마다 자동으로 GitHub 최신 릴리즈를 조회한다.
- 새 버전이 있으면 토스트에 **지금 업데이트** 버튼이 나온다. 누르면 앱 안에서 설치 파일을 내려받아 바로 설치하고 앱이 다시 열린다.
- 설치 순서: 최신 릴리즈 재조회 → HTTPS(`github.com` 계열)로 설치 파일 다운로드 → GitHub가 릴리즈에 기록한 **SHA-256과 일치하는지 검증** → 실행 파일 형식 확인 → `설치파일 /P /UPDATE /R` 실행 후 앱 종료. SHA-256이 없거나 다르면 설치하지 않는다.
- 설치 파일에는 코드 서명이 없다. 검증은 "받은 파일이 릴리즈에 올라간 파일과 같은지"만 보장한다.
- 저장소가 **비공개이면 확인할 수 없다**(로그인 없이 조회하므로 404). 공개 저장소이고 릴리즈에 설치 파일이 있어야 동작한다.
- 테스트용으로 `DOCK_UPDATE_URL` 환경변수로 조회 주소를 바꿀 수 있다(이때만 다운로드 호스트 제한이 풀린다).

## 서비스 아이콘
- `src/icons/services/`의 아이콘은 `python tools/build_icons.py`로 내려받아 만든다(결과는 `src/service-icons.js`에 기록).
- 출처 우선순위: [Iconify `logos`](https://github.com/gilbarbara/logos) 풀컬러 로고(CC0) → [Simple Icons](https://simpleicons.org) 단색 글리프(CC0, 브랜드 색 타일 위에 표시) → 서비스 파비콘(Outlook, To Do) → 없으면 글자 아이콘(Google Contacts).
- 각 로고의 상표권은 해당 서비스 소유자에게 있으며, 이 앱에서는 서비스를 식별하는 용도로만 쓴다.

## 알려진 한계
- 다운로드 일시정지/재개/재시도 없음 — wry가 다운로드 시작·완료 이벤트만 제공한다. 열기/폴더/삭제는 가능.
- Gmail 카테고리 알림은 Gmail의 받은편지함 카테고리 탭(기본/소셜/프로모션 등)을 켠 경우에만 종류별로 구분된다. 꺼져 있으면 모든 새 메일이 "기본"으로 취급된다. 카테고리용 피드 라벨 이름은 실제 계정으로 검증하지 못했다.
- 숨겨진 서비스 화면은 WebView2가 타이머를 늦출 수 있어 배지/폴링이 최대 1분 단위로 지연될 수 있다.
- Naver/Daum/Yahoo 등은 제목에 `(N)` 패턴이 없으면 배지가 뜨지 않는다. 필요하면 `recipes.json`의 `badge_regex`를 조정한다.
- 설치 파일 코드 서명, 스토어 배포는 하지 않는다.

## 라이선스
- Dock은 [MIT License](LICENSE)로 배포한다. Copyright (c) 2026 pyjhoop.
- 포함된 외부 구성요소(Rust 의존성, 서비스 아이콘 출처)의 라이선스는 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)에 정리했다. 의존성이 바뀌면 `python tools/gen_notices.py`로 다시 만든다.
- 서비스 이름과 로고는 각 소유자의 상표이며, Dock은 해당 소유자와 무관하다.
