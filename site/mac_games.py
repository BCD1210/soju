#!/usr/bin/env python3
"""Build the per-game "does it run on a Mac" pages from site/mac-games.json.

docs/mac/<slug>.html and docs/mac/index.html (English), docs/ko/mac/... for entries
with a "ko" block, and the matching block of docs/sitemap.xml. Run: python3 site/mac_games.py
"""
import html
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"
SITE = "https://didimlabs.com/"
CSS = re.search(r"CSS_VERSION = '([^']+)'", (ROOT / "site/build.mjs").read_text()).group(1)
DATA = json.loads((ROOT / "site/mac-games.json").read_text())
CHECKED = DATA["checked"]
REPORT = "https://github.com/BCD1210/soju/issues/new?template=compatibility.yml"
DISCORD = "https://discord.gg/ttBDcF5ftz"

LAUNCHER = {
    "battlenet": ("Battle.net", "battlenet"), "steam": ("Steam", "steam"), "epic": ("Epic Games Launcher", "epic"),
    "ea": ("EA app", "ea"), "rockstar": ("Rockstar Games Launcher", "rockstar"), "ubisoft": ("Ubisoft Connect", "ubisoft"),
    "none": ("its own launcher", None),
}
STATUS = {
    "en": {"verified": ("Runs in Soju", "ok"), "untested": ("Not checked in Soju yet", "unknown"),
           "blocked": ("Does not run: anti-cheat", "no"), "blocked-online": ("Online blocked by anti-cheat", "no"),
           "native": ("Has a Mac version", "native")},
    "ko": {"verified": ("소주에서 됨", "ok"), "untested": ("소주에서 아직 미확인", "unknown"),
           "blocked": ("안 됨: 안티치트", "no"), "blocked-online": ("온라인은 안티치트로 막힘", "no"),
           "native": ("맥 버전 있음", "native")},
}
CSS_EXTRA = """<style>
.status{display:inline-block;padding:.25em .7em;border-radius:999px;font-weight:600;font-size:.95em}
.status.ok{background:#1f6f43;color:#fff}.status.unknown{background:#5a5a5a;color:#fff}
.status.no{background:#8a2b2b;color:#fff}.status.native{background:#2b4f8a;color:#fff}
table.games td,table.games th{vertical-align:top}
</style>"""


def esc(s):
    return html.escape(s, quote=True)


def page(lang, title, desc, canonical, alternates, body, faq, base):
    ld = {"@context": "https://schema.org", "@type": "TechArticle", "headline": title, "description": desc,
          "url": canonical, "mainEntityOfPage": canonical, "inLanguage": lang, "dateModified": CHECKED,
          "isPartOf": {"@type": "WebSite", "name": "Soju", "url": SITE}}
    blocks = [json.dumps(ld, ensure_ascii=False, indent=2)]
    if faq:
        blocks.append(json.dumps({"@context": "https://schema.org", "@type": "FAQPage", "inLanguage": lang,
                                  "mainEntity": [{"@type": "Question", "name": q,
                                                  "acceptedAnswer": {"@type": "Answer", "text": a}} for q, a in faq]},
                                 ensure_ascii=False, indent=2))
    alt = "".join('<link rel="alternate" hreflang="%s" href="%s">\n' % (l, u) for l, u in alternates)
    scripts = "".join('<script type="application/ld+json">\n%s\n</script>\n' % b for b in blocks)
    nav = ('<a href="%smac/">Games</a>\n  <a href="%sguides/soju-vs-whisky-crossover-heroic.html">Compare</a>\n'
           '  <a href="https://github.com/BCD1210/soju">GitHub</a>\n  <a href="%s">Discord</a>' % (base, base, DISCORD))
    if lang == "ko":
        nav = ('<a href="%sko/mac/">게임</a>\n  <a href="%sko/">한국어 홈</a>\n'
               '  <a href="https://github.com/BCD1210/soju">GitHub</a>\n  <a href="%s">디스코드</a>' % (base, base, DISCORD))
    foot = ("Soju is GPL-3.0 · <a href=\"https://github.com/BCD1210/soju\">github.com/BCD1210/soju</a> · "
            "not affiliated with the publishers named here. Game names are trademarks of their owners.")
    if lang == "ko":
        foot = ("소주는 GPL-3.0 오픈소스입니다 · <a href=\"https://github.com/BCD1210/soju\">github.com/BCD1210/soju</a> · "
                "여기 나온 게임사와 관계없습니다. 게임 이름은 각 소유자의 상표입니다.")
    return f"""<!DOCTYPE html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{esc(title)}</title>
<meta name="description" content="{esc(desc)}">
<link rel="canonical" href="{canonical}">
{alt}<meta property="og:title" content="{esc(title)}">
<meta property="og:description" content="{esc(desc)}">
<meta property="og:type" content="article">
<meta property="og:url" content="{canonical}">
<meta property="og:image" content="{SITE}assets/soju-share.png">
<link rel="icon" href="{base}assets/favicon.png?v={CSS}">
<link rel="apple-touch-icon" href="{base}assets/apple-touch-icon.png?v={CSS}">
<link rel="stylesheet" href="{base}style.css?v={CSS}">
{CSS_EXTRA}
{scripts}</head>
<body>
<nav><div class="inner">
  <a class="brand" href="{base}{'ko/' if lang == 'ko' else ''}"><img src="{base}assets/soju-logo-64.png?v={CSS}" width="28" height="28" alt="">Soju<span> by Didim Labs</span></a>
  {nav}
</div></nav>
<main>
{body}
</main>
<footer>
  {foot}
</footer>
</body>
</html>
"""


def steps(lang, g):
    name, cmd = LAUNCHER[g["launcher"]]
    if not cmd:
        return ""
    req = {"steam": " (macOS 26 or later)"}.get(g["launcher"], "")
    if lang == "ko":
        req = {"steam": " (macOS 26 이상)"}.get(g["launcher"], "")
        return f"""<h2>소주로 해 보는 방법</h2>
<pre><code>brew install BCD1210/soju/soju
soju install          # {name} 선택{req}
soju {cmd}{' ' * max(1, 14 - len(cmd))}# 로그인 → 게임 설치 → 플레이</code></pre>
<p>또는 <a href="../../ko/#install">소주 앱을 받아</a> 플랫폼에서 {name}을(를) 설치하세요. 애플 실리콘(M1 이상), macOS 14 이상이 필요합니다.</p>"""
    return f"""<h2>How to try it with Soju</h2>
<pre><code>brew install BCD1210/soju/soju
soju install          # choose {name}{req}
soju {cmd}{' ' * max(1, 14 - len(cmd))}# sign in, install the game, press Play</code></pre>
<p>Or <a href="../#install">download the Soju app</a> and add {name} under Platforms. Needs an Apple Silicon Mac (M1 or newer) and macOS 14 or later.</p>"""


def game_page(lang, g):
    k = g.get("ko", {}) if lang == "ko" else {}
    name, detail = k.get("name", g["name"]), k.get("detail", g["detail"])
    label, cls = STATUS[lang][g["status"]]
    launcher = LAUNCHER[g["launcher"]][0]
    base = "../../" if lang == "ko" else "../"
    path = ("ko/mac/" if lang == "ko" else "mac/") + g["slug"] + ".html"
    canonical = SITE + path
    alternates = [("en", SITE + "mac/" + g["slug"] + ".html")]
    if "ko" in g:
        alternates.append(("ko", SITE + "ko/mac/" + g["slug"] + ".html"))
    runnable = g["status"] in ("verified", "untested", "blocked-online")
    if lang == "ko":
        title = f"맥에서 {name} 되나요? 애플 실리콘 실행 여부 ({CHECKED[:4]}) | 소주"
        q1 = f"애플 실리콘 맥에서 {name} 되나요?"
        a1 = f"{label}. {detail}"
        faq = [(q1, a1)]
        if runnable:
            faq.append((f"맥에서 {name}을(를) 어떻게 설치하나요?",
                        f"무료 오픈소스 앱 소주로 윈도우용 {launcher}을(를) 설치하고, 로그인한 뒤 게임을 설치해 실행합니다."))
        lead = "소주는 애플 실리콘 맥에 공식 윈도우 게임 런처를 설치하는 무료 오픈소스 앱입니다."
        checked = (f"{g['verified_on']}, M4 Pro · macOS 26.5에서 실행" if g["status"] == "verified" else f"{CHECKED} 갱신")
        report = (f'해 보셨나요? <a href="{DISCORD}">디스코드</a>나 <a href="{REPORT}">호환성 보고</a>로 알려 주시면 이 페이지에 반영합니다.')
        more = f'<p><a href="./">다른 게임 목록</a></p>'
    else:
        verb = "use the Mac version" if g["status"] == "native" else "does it run?"
        title = f"{name} on Mac (Apple Silicon): {verb} ({CHECKED[:4]}) | Soju"
        q1 = f"Does {name} run on an Apple Silicon Mac?"
        a1 = f"{label}. {detail}"
        faq = [(q1, a1)]
        if runnable:
            faq.append((f"How do I install {name} on a Mac?",
                        f"Install the Windows {launcher} with Soju, a free open-source app, sign in, then install the game and press Play."))
        lead = "Soju is a free, open-source app that installs the official Windows game launchers on Apple Silicon Macs."
        checked = (f"Run on {g['verified_on']}, M4 Pro with macOS 26.5" if g["status"] == "verified" else f"Page updated {CHECKED}")
        report = (f'Tried it? Tell us in <a href="{DISCORD}">Discord</a> or <a href="{REPORT}">send a compatibility report</a> '
                  "and this page gets updated.")
        more = '<p><a href="./">All games</a></p>'
    guide = k.get("guide") if lang == "ko" else g.get("guide")
    guide_html = ""
    if guide:
        guide_html = (f'<p><a class="btn" href="{guide}">' + ("자세한 가이드" if lang == "ko" else "Full guide") + "</a></p>")
    body = f"""<p class="note">{lead}</p>
<h1>{esc(name)}{' 맥에서 하기' if lang == 'ko' else ' on Mac'}</h1>
<p><span class="status {cls}">{esc(label)}</span></p>
<p class="lead">{esc(detail)}</p>
{guide_html}
<table>
  <tr><th>{'런처' if lang == 'ko' else 'Launcher'}</th><td>{esc(launcher)}</td></tr>
  <tr><th>{'상태' if lang == 'ko' else 'Status'}</th><td>{esc(label)}</td></tr>
  <tr><th>{'날짜' if lang == 'ko' else 'Date'}</th><td>{esc(checked)}</td></tr>
</table>
{steps(lang, g) if runnable else ''}
<p>{report}</p>
<h2 id="questions">{'자주 묻는 질문' if lang == 'ko' else 'Questions'}</h2>
<div class="faq">
""" + "\n".join(f"  <details{' open' if i == 0 else ''}><summary>{esc(q)}</summary><p>{esc(a)}</p></details>"
               for i, (q, a) in enumerate(faq)) + f"""
</div>
{more}"""
    desc = a1[:300]
    return path, page(lang, title, desc, canonical, alternates, body, faq, base)


def index_page(lang, games):
    base = "../../" if lang == "ko" else "../"
    path = "ko/mac/index.html" if lang == "ko" else "mac/index.html"
    canonical = SITE + ("ko/mac/" if lang == "ko" else "mac/")
    order = ["verified", "untested", "blocked-online", "blocked", "native"]
    rows = []
    for g in sorted(games, key=lambda g: (order.index(g["status"]), g["name"])):
        k = g.get("ko", {}) if lang == "ko" else {}
        label, cls = STATUS[lang][g["status"]]
        rows.append(f'  <tr><td><a href="{g["slug"]}.html">{esc(k.get("name", g["name"]))}</a></td>'
                    f'<td>{esc(LAUNCHER[g["launcher"]][0])}</td><td><span class="status {cls}">{esc(label)}</span></td></tr>')
    if lang == "ko":
        title, h1 = f"맥에서 되는 윈도우 게임 목록 ({CHECKED[:4]}) | 소주", "맥(애플 실리콘)에서 이 게임 되나요?"
        desc = "디아블로, 오버워치, 배틀그라운드, 발로란트, GTA 5 등 윈도우 게임이 애플 실리콘 맥에서 되는지: 소주에서 직접 확인한 것, 아직 미확인, 안티치트로 안 되는 것, 맥 버전이 있는 것."
        intro = "직접 실행해 본 게임만 '됨'으로 적습니다. 나머지는 미확인이거나, 안티치트 때문에 Wine에서 안 되거나, 게임사가 맥 버전을 내는 경우입니다."
        head = "<tr><th>게임</th><th>런처</th><th>상태</th></tr>"
    else:
        title, h1 = f"Which Windows games run on Apple Silicon Macs ({CHECKED[:4]}) | Soju", "Does this game run on a Mac?"
        desc = "Diablo, Overwatch, GTA V, Apex, Fortnite, Valorant and more on Apple Silicon: what was run in Soju, what is not checked yet, what anti-cheat blocks, and what has a Mac version."
        intro = ("Only games we ran ourselves are marked as running. The rest are not checked yet, blocked by anti-cheat "
                 "that refuses Wine, or have a Mac version from the publisher.")
        head = "<tr><th>Game</th><th>Launcher</th><th>Status</th></tr>"
    body = f"""<h1>{h1}</h1>
<p class="lead">{intro}</p>
<table class="games">
  {head}
{chr(10).join(rows)}
</table>
<p>{'마지막 확인' if lang == 'ko' else 'Last checked'} {CHECKED}. <a href="{REPORT}">{'호환성 보고 보내기' if lang == 'ko' else 'Send a compatibility report'}</a></p>"""
    alternates = [("en", SITE + "mac/"), ("ko", SITE + "ko/mac/")]
    return path, page(lang, title, desc, canonical, alternates, body, None, base)


def main():
    games = DATA["games"]
    out = {}
    for g in games:
        p, h = game_page("en", g); out[p] = h
        if "ko" in g:
            p, h = game_page("ko", g); out[p] = h
    for lang in ("en", "ko"):
        sel = games if lang == "en" else [g for g in games if "ko" in g]
        p, h = index_page(lang, sel); out[p] = h
    for old in list((DOCS / "mac").glob("*.html")) + list((DOCS / "ko/mac").glob("*.html")):
        if str(old.relative_to(DOCS)) not in out:
            old.unlink()
    for p, h in out.items():
        f = DOCS / p
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text(h)
    sm = DOCS / "sitemap.xml"
    s = sm.read_text()
    urls = []
    for p in sorted(out):
        loc = SITE + (p[:-len("index.html")] if p.endswith("index.html") else p)
        urls.append(f"  <url><loc>{loc}</loc><lastmod>{CHECKED}</lastmod></url>")
    block = "  <!-- mac-games -->\n" + "\n".join(urls) + "\n  <!-- /mac-games -->\n"
    if "<!-- mac-games -->" in s:
        s = re.sub(r"  <!-- mac-games -->.*?<!-- /mac-games -->\n", block, s, flags=re.S)
    else:
        s = s.replace("</urlset>", block + "</urlset>")
    sm.write_text(s)
    print("built", len(out), "pages")


if __name__ == "__main__":
    main()
