"""THIRD_PARTY_NOTICES.md 를 생성한다.  사용: python tools/gen_notices.py
- Rust 의존성: cargo metadata(Windows 대상, 개발 전용 제외)에서 이름/버전/라이선스/저장소를 읽는다.
- 아이콘 출처와 런타임 구성요소는 아래 상수에 적는다.
"""
import collections
import json
import os
import subprocess

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, '..'))
MANIFEST = os.path.join(ROOT, 'src-tauri', 'Cargo.toml')
OUT = os.path.join(ROOT, 'THIRD_PARTY_NOTICES.md')

raw = subprocess.run(
    ['cargo', 'metadata', '--format-version', '1', '--filter-platform', 'x86_64-pc-windows-msvc',
     '--manifest-path', MANIFEST],
    capture_output=True, text=True, encoding='utf-8', check=True).stdout
meta = json.loads(raw)
by_id = {p['id']: p for p in meta['packages']}
nodes = {n['id']: n for n in meta['resolve']['nodes']}
root = meta['resolve']['root']

seen, stack = set(), [root]
while stack:
    cur = stack.pop()
    if cur in seen:
        continue
    seen.add(cur)
    for dep in nodes[cur]['deps']:
        if all(k['kind'] == 'dev' for k in dep['dep_kinds']):
            continue
        stack.append(dep['pkg'])

rows = []
for pid in seen:
    if pid == root:
        continue
    p = by_id[pid]
    rows.append((p['name'], p['version'], p.get('license') or '(라이선스 파일 참조)', p.get('repository') or p.get('homepage') or ''))
rows.sort(key=lambda r: (r[0].lower(), r[1]))

counts = collections.Counter(r[2] for r in rows)
mpl = [f'{r[0]} {r[1]}' for r in rows if 'MPL' in r[2]]

lines = []
w = lines.append
w('# Third-Party Notices')
w('')
w('Dock 자체는 [MIT License](LICENSE)로 배포됩니다. 아래는 Dock 빌드에 포함되는 외부 구성요소와 그 라이선스 표기입니다.')
w('')
w('> 이 문서는 `python tools/gen_notices.py`로 생성한 **목록**입니다. 각 라이선스의 전문과 저작권 표기는 해당 구성요소의 저장소(또는 crates.io 소스 패키지)에 있습니다.')
w('> 법률 자문이 아니며, 배포 범위를 넓힐 경우 [cargo-about](https://github.com/EmbarkStudios/cargo-about) 같은 도구로 전문이 포함된 고지문을 만드는 것을 권합니다.')
w('')
w('## 1. 런타임 구성요소')
w('')
w('| 구성요소 | 설명 | 라이선스/약관 |')
w('|---|---|---|')
w('| [Tauri](https://tauri.app) 2 및 플러그인 | 앱 프레임워크 | MIT OR Apache-2.0 |')
w('| Microsoft Edge WebView2 Runtime | 웹 서비스 화면 렌더링. **Dock에 포함되지 않으며** Windows에 설치된 런타임을 사용 | Microsoft 소프트웨어 사용 조건 |')
w('')
w('## 2. 서비스 아이콘')
w('')
w('`src/icons/services/`와 `src/service-icons.js`의 아이콘은 `tools/build_icons.py`로 내려받았습니다.')
w('')
w('| 출처 | 용도 | 라이선스 |')
w('|---|---|---|')
w('| [Iconify `logos` (SVG Logos, gilbarbara/logos)](https://github.com/gilbarbara/logos) | 풀컬러 서비스 로고 | CC0-1.0 |')
w('| [Simple Icons](https://simpleicons.org) v16.35.0 | 단색 글리프 + 브랜드 색 타일 | CC0-1.0 |')
w('| Outlook, To Do 서비스 파비콘 | 해당 서비스 식별용 | 각 서비스 소유자의 상표/저작물 |')
w('')
w('각 서비스의 이름과 로고는 해당 소유자의 상표입니다. Dock은 서비스를 식별하는 용도로만 사용하며, 해당 소유자와 제휴하거나 보증받은 앱이 아닙니다.')
w('')
w('## 3. Rust 의존성')
w('')
w(f'Windows 빌드에 포함되는 크레이트 {len(rows)}개의 라이선스 분포:')
w('')
w('| 라이선스 표기 | 개수 |')
w('|---|---:|')
for lic, n in counts.most_common():
    w(f'| {lic} | {n} |')
w('')
if mpl:
    w('**MPL-2.0 크레이트**: ' + ', '.join(mpl) + '. 수정하지 않고 의존성으로만 사용하며, 소스는 각 저장소에서 받을 수 있습니다.')
    w('')
w('GPL, LGPL, AGPL 계열 의존성은 포함되어 있지 않습니다(위 표기 기준).')
w('')
w('<details><summary>전체 목록 (이름, 버전, 라이선스, 저장소)</summary>')
w('')
w('| 이름 | 버전 | 라이선스 | 저장소 |')
w('|---|---|---|---|')
for name, ver, lic, repo in rows:
    w(f'| {name} | {ver} | {lic} | {repo} |')
w('')
w('</details>')
w('')
w('## 4. 개발 도구 (배포물에 포함되지 않음)')
w('')
w('- `@tauri-apps/cli` — MIT OR Apache-2.0 (설치 파일 빌드용)')
w('')

open(OUT, 'w', encoding='utf-8', newline='\n').write('\n'.join(lines))
print('작성:', OUT, '/ 크레이트', len(rows), '개')
