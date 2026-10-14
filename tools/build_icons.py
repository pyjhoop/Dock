"""서비스 아이콘을 내려받아 src/icons/services 에 저장하고 src/service-icons.js 를 만든다.
우선순위: logos(풀컬러, CC0) > Simple Icons(단색 글리프 + 브랜드색 타일, CC0) > 128px 파비콘 > (없음: UI가 글자로 대체)
"""
import json
import os
import re
import shutil
import urllib.request

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'src'))
OUT = os.path.join(ROOT, 'icons', 'services')
SI_VER = '16.35.0'
SI = f'https://cdn.jsdelivr.net/npm/simple-icons@{SI_VER}'

shutil.rmtree(OUT, ignore_errors=True)
os.makedirs(OUT, exist_ok=True)


def get(url):
    req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
    return urllib.request.urlopen(req, timeout=30).read()


si_data = json.loads(get(SI + '/data/simple-icons.json'))
si_by_slug = {}
for it in si_data:
    slug = it.get('slug') or ''.join(c for c in it['title'].lower() if c.isalnum())
    si_by_slug[slug] = it

# key -> (소스, 식별자)
SPEC = {
    'google-gmail': ('logos', 'google-gmail'),
    'google-calendar': ('logos', 'google-calendar'),
    'google-keep': ('logos', 'google-keep'),
    'google-drive': ('logos', 'google-drive'),
    'google-meet': ('logos', 'google-meet'),
    'google-photos': ('logos', 'google-photos'),
    'google-maps': ('logos', 'google-maps'),
    'google-gemini': ('logos', 'google-gemini-icon'),
    'google-tasks': ('si', 'googletasks'),
    'google-chat': ('si', 'googlechat'),
    'google-news': ('si', 'googlenews'),
    'google-translate': ('si', 'googletranslate'),
    'google-classroom': ('si', 'googleclassroom'),
    'microsoft-mail': ('s2', 'outlook.live.com'),
    'microsoft-calendar': ('s2', 'outlook.live.com'),
    'microsoft-people': ('s2', 'outlook.live.com'),
    'microsoft-to-do': ('s2', 'to-do.live.com'),
    'microsoft-teams': ('logos', 'microsoft-teams'),
    'microsoft-onedrive': ('logos', 'microsoft-onedrive'),
    'naver-mail': ('si', 'naver'),
    'naver-calendar': ('si', 'naver'),
    'naver-cloud': ('si', 'naver'),
    'daum-mail': ('si', 'kakao'),
    'yahoo-mail': ('logos', 'yahoo'),
    'slack-slack': ('logos', 'slack-icon'),
    'notion-notion': ('logos', 'notion-icon'),
    'discord-discord': ('logos', 'discord-icon'),
    'telegram-telegram': ('logos', 'telegram'),
}


def luminance(hex6):
    r, g, b = (int(hex6[i:i + 2], 16) / 255 for i in (0, 2, 4))
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


result = {}
cache = {}
for key, (src, ident) in SPEC.items():
    try:
        if (src, ident) in cache:
            entry = dict(cache[(src, ident)])
            result[key] = entry
            continue
        if src == 'logos':
            svg = get(f'https://api.iconify.design/logos/{ident}.svg').decode('utf-8')
            if '<svg' not in svg:
                raise ValueError('svg 아님')
            fname = f'{key}.svg'
            open(os.path.join(OUT, fname), 'w', encoding='utf-8').write(svg)
            entry = {'src': f'icons/services/{fname}', 'kind': 'color'}
        elif src == 'si':
            it = si_by_slug[ident]
            svg = get(f'{SI}/icons/{ident}.svg').decode('utf-8')
            hexc = it['hex']
            glyph = '#111111' if luminance(hexc) > 0.62 else '#FFFFFF'
            svg = svg.replace('<svg ', f'<svg fill="{glyph}" ', 1)
            fname = f'{key}.svg'
            open(os.path.join(OUT, fname), 'w', encoding='utf-8').write(svg)
            entry = {'src': f'icons/services/{fname}', 'kind': 'glyph', 'bg': '#' + hexc}
        elif src == 's2':
            data = get(f'https://www.google.com/s2/favicons?domain={ident}&sz=128')
            fname = f'{key}.png'
            open(os.path.join(OUT, fname), 'wb').write(data)
            entry = {'src': f'icons/services/{fname}', 'kind': 'color'}
        cache[(src, ident)] = entry
        result[key] = entry
        print('OK  ', key, src, ident)
    except Exception as e:
        print('FAIL', key, src, ident, e)

js = '// 자동 생성: scratchpad/build_icons.py (소스: logos CC0, Simple Icons CC0, 서비스 파비콘)\nwindow.SERVICE_ICONS = ' + json.dumps(result, ensure_ascii=False, indent=1) + ';\n'
open(os.path.join(ROOT, 'service-icons.js'), 'w', encoding='utf-8').write(js)
print('총', len(result), '개 / 요청', len(SPEC), '개')
