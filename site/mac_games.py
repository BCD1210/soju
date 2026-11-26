#!/usr/bin/env python3
"""Build the per-game "does it run on a Mac" pages from site/mac-games.json.

docs/mac/<slug>.html and docs/mac/index.html (English), docs/ko/mac/... for entries
with a "ko" block, and the matching block of docs/sitemap.xml. Run: python3 site/mac_games.py
"""
import html
import json
from pathlib import Path
import re
import unicodedata

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"
SITE = "https://didimlabs.com/"
CSS = re.search(r"CSS_VERSION = '([^']+)'", (ROOT / "site/build.mjs").read_text()).group(1)
DATA = json.loads((ROOT / "site/mac-games.json").read_text())
EST = json.loads((ROOT / "site/protondb-estimates.json").read_text())
AC = json.loads((ROOT / "site/anticheat.json").read_text())["games"]   # site/anticheat_update.py
_rep = ROOT / "site/soju-reports.json"
REPORTS = json.loads(_rep.read_text())["reports"] if _rep.exists() else []
_plays = ROOT / "site/soju-plays.json"   # site/soju_plays.py: games Soju users have played
PLAYS = json.loads(_plays.read_text())["games"] if _plays.exists() else []
AWACY = "https://areweanticheatyet.com/"
HIDDEN = set()            # canonical URLs of noindex pages, left out of the sitemap
KO_ESTIMATES = 3000   # Korean pages for the most-reported estimated games
INDEX_ESTIMATES = 300
WELL_SUPPORTED = 40      # ProtonDB reports for a likely estimate to be indexable
CHECKED = DATA["checked"]
REPORT = "https://github.com/BCD1210/soju/issues/new?template=compatibility.yml"
DISCORD = "https://discord.gg/ttBDcF5ftz"
DISCORD_COMPAT = "https://discord.gg/PwBjET6YZ3"  # lands in #compatibility
DISCORD_KO = "https://discord.gg/eY6UHRCKZU"  # lands in #한국어
# The download the home page offers (scripts bump its version in content/index.json).
_hero = next(x for x in json.loads((ROOT / "site/content/index.json").read_text())["body"] if x["type"] == "Hero")
DOWNLOAD = next(a["href"] for a in _hero["props"]["actions"] if a.get("primary"))

LAUNCHER = {
    "battlenet": ("Battle.net", "battlenet"), "steam": ("Steam", "steam"), "epic": ("Epic Games Launcher", "epic"),
    "ea": ("EA app", "ea"), "rockstar": ("Rockstar Games Launcher", "rockstar"), "ubisoft": ("Ubisoft Connect", "ubisoft"),
    "none": ("its own launcher", None),
}
STATUS = {
    "en": {"verified": ("Plays", "ok"), "untested": ("Not checked in Soju yet", "unknown"),
           "blocked": ("Does not run: anti-cheat", "no"), "blocked-online": ("Online blocked by anti-cheat", "no"),
           "native": ("Has a Mac version", "native"),
           "likely": ("Estimate: likely runs", "likely"), "mixed": ("Estimate: may have problems", "unknown"),
           "unlikely": ("Estimate: unlikely to run", "no"), "few": ("Estimate: few reports", "unknown"),
           "soju-works": ("Plays", "ok")},
    "ko": {"verified": ("플레이됨", "ok"), "untested": ("소주에서 아직 미확인", "unknown"),
           "blocked": ("안 됨: 안티치트", "no"), "blocked-online": ("온라인은 안티치트로 막힘", "no"),
           "native": ("맥 버전 있음", "native"),
           "likely": ("추정: 될 가능성 높음", "likely"), "mixed": ("추정: 문제 있을 수 있음", "unknown"),
           "unlikely": ("추정: 안 될 가능성 높음", "no"), "few": ("추정: 보고 적음", "unknown"),
           "soju-works": ("플레이됨", "ok")},
}
ESTIMATE = ("likely", "mixed", "unlikely", "few")
MACDATA = ("soju-works",)


def header_image(appid):
    return f"https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/{appid}/header.jpg"
CSS_EXTRA = ""


def lname(lang, key):
    return "자체 런처" if lang == "ko" and key == "none" else LAUNCHER[key][0]


def esc(s):
    return html.escape(s, quote=True)


def page(lang, title, desc, canonical, alternates, body, faq, base, game=None, image=None, noindex=False):
    ld = {"@context": "https://schema.org", "@type": "TechArticle", "headline": title, "description": desc,
          "url": canonical, "mainEntityOfPage": canonical, "inLanguage": lang, "dateModified": CHECKED,
          "isPartOf": {"@type": "WebSite", "name": "Soju", "url": SITE}}
    if game:
        ld["about"] = game
    if image:
        ld["image"] = image
    blocks = [json.dumps(ld, ensure_ascii=False, separators=(",", ":"))]
    if faq:
        blocks.append(json.dumps({"@context": "https://schema.org", "@type": "FAQPage", "inLanguage": lang,
                                  "mainEntity": [{"@type": "Question", "name": q,
                                                  "acceptedAnswer": {"@type": "Answer", "text": a}} for q, a in faq]},
                                 ensure_ascii=False, separators=(",", ":")))
    alt = "".join('<link rel="alternate" hreflang="%s" href="%s">\n' % (l, u) for l, u in alternates)
    # Pages that only carry a Linux-based estimate stay out of search: thousands of near-identical
    # pages with no Mac evidence make a search engine trust the checked pages less.
    robots = '<meta name="robots" content="noindex,follow">\n' if noindex else ""
    if noindex:
        HIDDEN.add(canonical)
    scripts = "".join('<script type="application/ld+json">\n%s\n</script>\n' % b.replace("<", "\\u003c") for b in blocks)
    # Same bar as the home page: find a game, guides, help, and the download on the right.
    nav = (f'<a href="{base}mac/">Find a game</a>\n  <a href="{base}#guides">Guides</a>\n  <a href="{base}#faq">FAQ</a>\n'
           f'  <a href="{DISCORD}">Discord</a>\n  <a href="https://github.com/BCD1210/soju">GitHub</a>\n'
           f'  <a href="{base}ko/mac/" lang="ko" hreflang="ko">한국어</a>\n  <a class="nav-download" href="{DOWNLOAD}">Download</a>')
    if lang == "ko":
        nav = (f'<a href="{base}ko/mac/">게임 검색</a>\n  <a href="{base}ko/#guides">가이드</a>\n  <a href="{base}ko/#faq">자주 묻는 질문</a>\n'
               f'  <a href="{DISCORD_KO}">디스코드</a>\n  <a href="https://github.com/BCD1210/soju">GitHub</a>\n'
               f'  <a href="{base}mac/" lang="en" hreflang="en">English</a>\n  <a class="nav-download" href="{DOWNLOAD}">다운로드</a>')
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
{robots}{alt}<meta property="og:title" content="{esc(title)}">
<meta property="og:description" content="{esc(desc)}">
<meta property="og:type" content="article">
<meta property="og:url" content="{canonical}">
<meta property="og:image" content="{image or SITE + 'assets/soju-share.png'}">
<link rel="icon" href="{base}assets/favicon.png?v={CSS}">
<link rel="apple-touch-icon" href="{base}assets/apple-touch-icon.png?v={CSS}">
<meta name="color-scheme" content="light dark">
<link rel="stylesheet" href="{base}style.css?v={CSS}">
{CSS_EXTRA}
{scripts}<script defer src="https://static.cloudflareinsights.com/beacon.min.js" data-cf-beacon='{{"token": "a7048ad8ec7b480c9ad71182b1641147"}}'></script>
</head>
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
    launcher = lname(lang, g["launcher"])
    base = "../../" if lang == "ko" else "../"
    path = ("ko/mac/" if lang == "ko" else "mac/") + g["slug"] + ".html"
    canonical = SITE + path
    alternates = [("en", SITE + "mac/" + g["slug"] + ".html")]
    if "ko" in g:
        alternates.append(("ko", SITE + "ko/mac/" + g["slug"] + ".html"))
    runnable = g["status"] in ("verified", "untested", "blocked-online") + ESTIMATE + MACDATA
    # Estimates stay out of search, except well-supported ones: 40+ reports, 85%+ saying it runs.
    est = g["status"] in ESTIMATE and not (g["status"] == "likely" and g.get("reports", 0) >= WELL_SUPPORTED)
    if lang == "ko":
        title = (f"맥북에서 {name} 될까? M1·M2·M3·M4 맥 실행 추정 ({CHECKED[:4]}) | 소주" if est else
                 f"맥북에서 {name} 되나요? M1·M2·M3·M4 맥 실행 여부 ({CHECKED[:4]}) | 소주")
        q1 = f"애플 실리콘 맥에서 {name} 되나요?"
        a1 = f"{label}. {detail}"
        faq = [(q1, a1)]
        if runnable:
            faq.append((f"맥에서 {name}을(를) 어떻게 설치하나요?",
                        f"무료 오픈소스 앱 소주로 윈도우용 {launcher}을(를) 설치하고, 로그인한 뒤 게임을 설치해 실행합니다."))
        lead = "소주는 애플 실리콘 맥에 공식 윈도우 게임 런처를 설치하는 무료 오픈소스 앱입니다."
        checked = (f"{g['verified_on']}, M4 Pro · macOS 26.5에서 실행" if g["status"] == "verified" else f"{CHECKED} 갱신")
        report = (f'해 보셨나요? <a href="{DISCORD_KO}">디스코드 한국어 채널</a>나 <a href="{REPORT}">호환성 보고</a>로 알려 주시면 이 페이지에 반영합니다.')
        more = f'<p><a href="./">다른 게임 목록</a></p>'
    else:
        verb = ("use the Mac version" if g["status"] == "native" else
                "will it run on M1, M2, M3, M4 MacBook? Estimate" if est else "does it run on M1, M2, M3, M4 MacBook?")
        title = f"{name} on Mac: {verb} ({CHECKED[:4]}) | Soju"
        q1 = f"Does {name} run on an Apple Silicon Mac?"
        a1 = f"{label}. {detail}"
        faq = [(q1, a1)]
        if runnable:
            faq.append((f"How do I install {name} on a Mac?",
                        f"Install the Windows {launcher} with Soju, a free open-source app, sign in, then install the game and press Play."))
        lead = "Soju is a free, open-source app that installs the official Windows game launchers on Apple Silicon Macs."
        checked = (f"Run on {g['verified_on']}, M4 Pro with macOS 26.5" if g["status"] == "verified" else f"Page updated {CHECKED}")
        report = (f'Tried it? Tell us in <a href="{DISCORD_COMPAT}">#compatibility on Discord</a> or <a href="{REPORT}">send a compatibility report</a> '
                  "and this page gets updated.")
        more = '<p><a href="./">All games</a></p>'
    extra = ""
    if g.get("linux"):
        n, y, when = g["linux"]
        if lang == "ko":
            extra = (f"<p>리눅스(Proton) 참고: 최근 2년 사용자 보고 {n}건 중 {y}건이 실행된다고 했습니다 "
                     f"(<a href=\"https://www.protondb.com/app/{g['appid']}\">ProtonDB</a>, {when} 기준, ODbL).</p>")
        else:
            extra = (f"<p>Linux (Proton) reference: {y} of {n} player reports from the last two years say it runs "
                     f"(<a href=\"https://www.protondb.com/app/{g['appid']}\">ProtonDB</a>, as of {when}, ODbL).</p>")
    if g.get("ac"):
        extra += (f'<p>{"안티치트 자료" if lang == "ko" else "Anti-cheat data"}: <a href="{AWACY}">Are We Anti-Cheat Yet?</a> (MIT).</p>')
    if g.get("appid"):
        extra = (f'<p><img class="shot" src="{header_image(g["appid"])}" width="460" height="215" loading="lazy" '
                 f'alt="{esc(name)}"></p>\n' + extra +
                 f'<p><a href="https://store.steampowered.com/app/{g["appid"]}/">'
                 + ("스팀 상점 페이지" if lang == "ko" else "Steam store page") + "</a></p>")
    if g.get("screenshot"):
        shot = ("../../assets/" if lang == "ko" else "../assets/") + g["screenshot"]
        cap = ("소주에서 실행한 화면" if lang == "ko" else "Running in Soju") + (f', {g["verified_on"]}' if g.get("verified_on") else "")
        extra = (f'<figure><img class="shot" src="{shot}" loading="lazy" alt="{esc(name)} {cap}">'
                 f'<figcaption>{cap}</figcaption></figure>\n') + extra
    rel = [r for r in g.get("related", []) if lang == "en" or "ko" in r]
    related = ""
    if rel:
        related = (("<h2>다른 게임</h2>" if lang == "ko" else "<h2>Other games</h2>") + "\n<p>" +
                   " · ".join(f'<a href="{r["slug"]}.html">{esc(r.get("ko", {}).get("name", r["name"]) if lang == "ko" else r["name"])}</a>' for r in rel)
                   + "</p>")
    guide = k.get("guide") if lang == "ko" else g.get("guide")
    guide_html = ""
    if guide:
        guide_html = (f'<p><a class="btn" href="{guide}">' + ("자세한 가이드" if lang == "ko" else "Full guide") + "</a></p>")
    body = f"""<p class="note">{lead}</p>
<h1>{esc(name)}{' 맥에서 하기' if lang == 'ko' else ' on Mac'}</h1>
<p><span class="status {cls}">{esc(label)}</span></p>
<p class="lead">{esc(detail)}</p>
{guide_html}
{extra}
<table>
  <tr><th>{'런처' if lang == 'ko' else 'Launcher'}</th><td>{esc(launcher)}</td></tr>
  <tr><th>{'상태' if lang == 'ko' else 'Status'}</th><td>{esc(label)}</td></tr>
  <tr><th>{'날짜' if lang == 'ko' else 'Date'}</th><td>{esc(checked)}</td></tr>
</table>
{steps(lang, g) if runnable else ''}
{guide_sections(path, guide)}<p>{report}</p>
<h2 id="questions">{'자주 묻는 질문' if lang == 'ko' else 'Questions'}</h2>
<div class="faq">
""" + "\n".join(f"  <details{' open' if i == 0 else ''}><summary>{esc(q)}</summary><p>{esc(a)}</p></details>"
               for i, (q, a) in enumerate(faq)) + f"""
</div>
{related}
{more}"""
    desc = a1[:300]
    game = {"@type": "VideoGame", "name": name, "operatingSystem": "Windows"}
    if g.get("appid"):
        game["sameAs"] = f"https://store.steampowered.com/app/{g['appid']}/"
    image = header_image(g["appid"]) if g.get("appid") else None
    return path, page(lang, title, desc, canonical, alternates, body, faq, base, game, image, noindex=est)


GUIDE_SECTIONS = ("What you need", "Requirements", "What we checked", "What we did not check",
                  "필요한 것", "준비물", "확인한 것", "확인하지 않은 것", "확인 안 한 것", "확인하지 못한 것")


def guide_sections(page_path, guide):
    """The requirements and checked / not checked sections of a game's guide, for its game page.

    The guide stays the one place they are written. Links that were relative to the guide's
    folder are pointed back at it."""
    if not guide:
        return ""
    src = (DOCS / page_path).parent / guide
    if not src.is_file():
        return ""
    folder = guide.rsplit("/", 1)[0] + "/"
    out = []
    for m in re.finditer(r"<h2[^>]*>(.*?)</h2>(.*?)(?=<h2|</main>)", src.read_text(), re.S):
        if m.group(1).strip() in GUIDE_SECTIONS:
            block = re.sub(r'(href|src)="(?!https?:|mailto:|#|\.\./)([^"]+)"', lambda x: f'{x.group(1)}="{folder}{x.group(2)}"', m.group(2).strip())
            out.append(f"<h2>{m.group(1).strip()}</h2>\n{block}")
    return "\n".join(out) + ("\n" if out else "")


def slugify(title):
    t = re.sub(r"[\u2122\u00ae\u00a9]", "", title)   # ™ ® © would become "tm" etc.
    t = unicodedata.normalize("NFKD", t).encode("ascii", "ignore").decode().lower()
    return re.sub(r"[^a-z0-9]+", "-", t).strip("-")


def apply_mac_data(g, h, ac, detail, ko):
    """The anti-cheat list outranks Linux estimates."""
    g["_ko"] = ko
    if ac:
        names = ", ".join(ac.get("anticheats") or []) or "kernel anti-cheat"
        g.update(status="blocked", ac=True,
                 detail=f"Its anti-cheat ({names}) does not let it run under Wine or Proton, so it does not run on a Mac "
                        "this way. " + ("The Linux numbers below are for Proton." if detail else ""))
        g["_ko"] = f"안티치트({names})가 Wine·Proton에서 실행을 막아, 이 방법으로는 맥에서 안 됩니다."
        return


def estimates(curated):
    """Pages for Steam games with ProtonDB reports; curated entries win and get the Linux line."""
    dump = EST["dump_newest_report"]
    by_slug = {g["slug"]: g for g in curated}
    by_name = {slugify(g["name"]): g for g in curated}
    out, used = [], set(by_slug)
    ac_app = {a["appid"]: a for a in AC.values() if a.get("appid")}
    seen_apps = set()
    for appid, title, n, y, recent, _ in EST["apps"]:
        seen_apps.add(appid)
        slug = slugify(title)
        if slug in by_name and "linux" not in by_name[slug] and recent:
            by_name[slug].update(appid=appid, linux=(n, y, dump))
            continue
        if not slug or slug in used:
            slug = (slug + "-" if slug else "steam-") + appid
        used.add(slug)
        r = y / n
        status = "few" if n < 3 else "likely" if r >= 0.85 else "mixed" if r >= 0.6 else "unlikely"
        span = "from the last two years" if recent else "in total"
        span_ko = "최근 2년" if recent else "전체 기간"
        detail = (f"Not run on a Mac yet; this is an estimate. On Linux with Proton, {y} of {n} player report{'s' if n != 1 else ''} {span} "
                  f"say the game runs (ProtonDB, as of {dump}). Soju runs Windows Steam games through "
                  "Wine on macOS, with Direct3D 11 through DXMT. Direct3D 12 games are not checked in Soju's Steam "
                  "setup yet, and anti-cheat or Mac-specific problems can make the result differ from Linux. If the "
                  "game's Steam page lists macOS, use that version instead.")
        ko = (f"맥에서는 아직 실행해 보지 않은 추정입니다. 리눅스(Proton)에서 {span_ko} 사용자 보고 {n}건 중 {y}건이 "
              f"실행된다고 했습니다(ProtonDB, {dump} 기준). 소주는 윈도우 스팀 게임을 맥의 Wine으로 돌리고, "
              "Direct3D 11은 DXMT로 처리합니다. Direct3D 12 게임은 소주 스팀 환경에서 아직 확인하지 않았고, "
              "안티치트나 맥 고유 문제로 리눅스와 결과가 다를 수 있습니다. 스팀 페이지에 macOS 버전이 있으면 그걸 쓰세요.")
        g = {"slug": slug, "name": title, "launcher": "steam", "status": status, "detail": detail,
             "appid": appid, "reports": n, "linux": (n, y, dump) if recent else None}
        apply_mac_data(g, None, ac_app.get(appid), detail, ko)
        if g["status"] in ESTIMATE:
            g["linux"] = None          # the estimate text already gives the Linux numbers
        if len([e for e in out if "ko" in e]) < KO_ESTIMATES:
            g["ko"] = {"name": title, "detail": g.pop("_ko")}
        g.pop("_ko", None)
        out.append(g)
    # Games only the anti-cheat list knows about.
    for h in [dict(a, kind="ac") for a in AC.values()]:
        if h.get("appid") in seen_apps or slugify(h["title"] or "") in by_name:
            continue
        slug = slugify(h["title"] or "")
        if not slug or slug in used:
            continue
        used.add(slug)
        seen_apps.add(h.get("appid"))
        g = {"slug": slug, "name": h["title"], "launcher": "steam" if h.get("appid") else "none",
             "status": "untested", "detail": "", "appid": h.get("appid"), "reports": 0}
        apply_mac_data(g, h if h["kind"] != "ac" else None, h if h["kind"] == "ac" else None, "", "")
        g["ko"] = {"name": h["title"], "detail": g.pop("_ko")}
        out.append(g)
    alpha = sorted(out, key=lambda g: g["slug"])
    for i, g in enumerate(alpha):
        g["related"] = [alpha[j] for j in range(max(0, i - 3), min(len(alpha), i + 4)) if j != i]
    return out


def table(lang, games, link_prefix=""):
    rows = []
    for g in games:
        k = g.get("ko", {}) if lang == "ko" else {}
        label, cls = STATUS[lang][g["status"]]
        rows.append(f'  <tr><td><a href="{link_prefix}{g["slug"]}.html">{esc(k.get("name", g["name"]))}</a></td>'
                    f'<td>{esc(lname(lang, g["launcher"]))}</td><td><span class="status {cls}">{esc(label)}</span></td></tr>')
    head = ("<tr><th>게임</th><th>런처</th><th>상태</th></tr>" if lang == "ko"
            else "<tr><th>Game</th><th>Launcher</th><th>Status</th></tr>")
    return f'<table class="games">\n  {head}\n' + "\n".join(rows) + "\n</table>"


def bucket(name):
    c = slugify(name)[:1]
    return c if c.isalpha() else "0"


def search_box(lang):
    """The game search on the list pages. The script (game-search.js) loads mac/search.json the first time someone types."""
    ko = lang == "ko"
    return f"""<form class="game-search" role="search" method="get" action="{'/ko/mac/' if ko else '/mac/'}" data-limit="25">
<label for="game-q">{'게임 이름으로 찾기' if ko else 'Search by game name'}</label>
<span class="search-row"><input id="game-q" name="q" type="search" autocomplete="off" placeholder="{'영문 이름 (예: Elden Ring). 한글 이름은 일부만' if ko else 'For example Elden Ring'}"><button class="btn" type="submit">{'찾기' if ko else 'Search'}</button></span>
<ul id="search-results" aria-live="polite" hidden></ul>
</form>
<script src="/game-search.js?v={search_js_version()}" defer></script>"""


SEARCH_JS = r"""// Game search on the home page and the game list. Generated by site/mac_games.py.
(function(){var C=__DATA__,f=document.querySelector('form.game-search'),q=document.getElementById('game-q'),out=document.getElementById('search-results');
if(!f||!q||!out)return;var L=C[document.documentElement.lang==='ko'?'ko':'en'],max=+f.getAttribute('data-limit')||8,all=null,t,first=null;
function norm(s){return (s||'').normalize('NFKD').replace(/[̀-ͯ]/g,'').normalize('NFC').toLowerCase().replace(/[^a-z0-9가-힣]+/g,'');}
function load(cb){if(all)return cb();fetch('/mac/search.json').then(function(r){return r.json();}).then(function(j){all=j.map(function(g){return g.concat([norm(g[0]),norm(g[1])]);});cb();});}
function el(tag,cls,text){var e=document.createElement(tag);if(cls)e.className=cls;if(text)e.textContent=text;return e;}
function show(){var v=norm(q.value);first=null;if(v.length<2){out.textContent='';out.hidden=true;return;}load(function(){
 if(norm(q.value)!==v)return;out.textContent='';out.hidden=false;
 var hit=all.filter(function(g){return g[6].indexOf(v)>=0||(g[7]&&g[7].indexOf(v)>=0);});
 hit.sort(function(a,b){var pa=(a[6].indexOf(v)===0||a[7].indexOf(v)===0)?0:1,pb=(b[6].indexOf(v)===0||b[7].indexOf(v)===0)?0:1;return pa-pb;});
 if(!hit.length){var li=el('li','none',(L.hangul&&/[가-힣]/.test(q.value)?L.hangul:L.none)+' ');var a=el('a',null,L.reportLabel);a.href=L.report;li.appendChild(a);out.appendChild(li);return;}
 hit.slice(0,max).forEach(function(g){var li=el('li'),a=el('a',null,L.ko&&g[1]?g[1]:g[0]);
  a.href=(L.ko&&g[5]?'/ko/mac/':'/mac/')+g[2]+'.html';if(!first)first=a.href;li.appendChild(a);
  li.appendChild(el('span','status '+(L.classes[g[3]]||'unknown'),L.labels[g[3]]||g[3]));
  li.appendChild(el('span','launcher',L.launchers[g[4]]||''));out.appendChild(li);});
 if(hit.length>max)out.appendChild(el('li','more',(hit.length-max)+L.more));});}
q.addEventListener('input',function(){clearTimeout(t);t=setTimeout(show,150);});
f.addEventListener('submit',function(e){e.preventDefault();if(norm(q.value).length<2){q.focus();return;}if(first&&!out.hidden){location.href=first;return;}clearTimeout(t);show();});
var m=location.search.match(/[?&]q=([^&]*)/);if(m){q.value=decodeURIComponent(m[1].replace(/[+]/g,' '));show();}})();
"""


def search_js():
    data = {}
    for lang in ("en", "ko"):
        ko = lang == "ko"
        data[lang] = {"labels": {k: v[0] for k, v in STATUS[lang].items()},
                      "classes": {k: v[1] for k, v in STATUS[lang].items()},
                      "launchers": {k: lname(lang, k) for k in LAUNCHER}, "ko": ko,
                      "none": "목록에 아직 없는 게임입니다." if ko else "Not in the list yet.",
                      "hangul": "한글 이름이 없는 게임이 많습니다. 영문 이름으로 찾아 보세요 (예: Elden Ring)." if ko else "",
                      "report": REPORT, "reportLabel": "해 보신 결과 알려 주기" if ko else "Tell us how it goes",
                      "more": "개 더 있습니다. 이름을 더 길게 입력해 보세요." if ko else " more. Type more of the name."}
    return SEARCH_JS.replace("__DATA__", json.dumps(data, ensure_ascii=False, separators=(",", ":")).replace("<", "\\u003c"))


def search_js_version():
    import hashlib
    return hashlib.sha256(search_js().encode()).hexdigest()[:8]


def search_index(curated, macdata_and_est):
    """[name, Korean name, slug, status, launcher, has a Korean page] for every game page."""
    rows = []
    for g in curated + macdata_and_est:
        rows.append([g["name"], g.get("ko", {}).get("name", ""), g["slug"], g["status"], g["launcher"], 1 if "ko" in g else 0])
    return json.dumps(rows, ensure_ascii=False, separators=(",", ":"))


def index_page(lang, curated, est, letters):
    base = "../../" if lang == "ko" else "../"
    path = "ko/mac/index.html" if lang == "ko" else "mac/index.html"
    canonical = SITE + ("ko/mac/" if lang == "ko" else "mac/")
    order = ["verified", "soju-works", "untested", "blocked-online", "blocked", "native"]   # soju-works: a curated game players ran in Soju (Counter-Strike 2, 2026-11-26)
    cur = sorted(curated, key=lambda g: (order.index(g["status"]), g["name"]))
    macdata = sorted([g for g in est if g["status"] in MACDATA and STATUS["en"][g["status"]][1] == "ok"], key=lambda g: (MACDATA.index(g["status"]), g["name"].lower()))
    top = sorted([g for g in est if g["status"] in ESTIMATE], key=lambda g: -g["reports"])[:INDEX_ESTIMATES]
    if lang == "ko":
        title, h1 = f"맥에서 되는 윈도우 게임 목록 ({CHECKED[:4]}) | 소주", "맥(애플 실리콘)에서 이 게임 되나요?"
        desc = "디아블로, 오버워치, 배틀그라운드, 발로란트, GTA 5와 스팀 게임 수천 개가 애플 실리콘 맥에서 되는지: 직접 확인한 것과 리눅스 사용자 보고(ProtonDB) 기반 추정."
        intro = ("'플레이됨'은 소주에서 실행되는 게임입니다. '추정'은 리눅스(Proton) 사용자 보고를 바탕으로 한 예상이고 맥에서는 아직 확인하지 않았습니다.")
        h_cur, h_est = "직접 확인했거나 확실한 게임", f"추정 (ProtonDB 보고가 많은 {INDEX_ESTIMATES}개)"
        h_mac = "소주에서 플레이되는 게임"
        more = "알파벳순 전체 목록(영문): " 
    else:
        title, h1 = f"Which Windows games run on Apple Silicon Macs ({CHECKED[:4]}) | Soju", "Does this game run on a Mac?"
        desc = f"Diablo, Overwatch, GTA V, Apex, Fortnite, Valorant and {len(est):,} Steam games on Apple Silicon: what was run in Soju, and estimates from Linux player reports (ProtonDB)."
        intro = ("'Plays' means it runs in Soju. 'Estimate' pages are based on Linux (Proton) player reports from "
                 "ProtonDB and have not been run on a Mac yet.")
        h_cur, h_est = "Checked or known", f"Estimates (the {INDEX_ESTIMATES} most reported)"
        h_mac = "Played in Soju"
        more = "All estimated games A–Z: "
    lp = "../../mac/" if lang == "ko" else ""
    links = " ".join(f'<a href="{lp}list-{c}.html">{c.upper() if c != "0" else "0–9"}</a>' for c in letters)
    body = f"""<h1>{h1}</h1>
<p class="lead">{intro}</p>
{search_box(lang)}
<h2>{h_cur}</h2>
{table(lang, cur)}
<h2>{h_mac}</h2>
{table(lang, [g for g in macdata if lang == 'en' or 'ko' in g])}
<h2>{h_est}</h2>
{table(lang, [g for g in top if lang == 'en' or 'ko' in g])}
<p>{more}{links}</p>
<p>{'마지막 갱신' if lang == 'ko' else 'Updated'} {CHECKED}. Linux data: <a href="https://github.com/bdefore/protondb-data">ProtonDB</a> (ODbL). <a href="{REPORT}">{'호환성 보고 보내기' if lang == 'ko' else 'Send a compatibility report'}</a></p>"""
    alternates = [("en", SITE + "mac/"), ("ko", SITE + "ko/mac/")]
    return path, page(lang, title, desc, canonical, alternates, body, None, base)


def letter_page(c, games, letters):
    label = "0–9" if c == "0" else c.upper()
    links = " ".join(f'<a href="list-{x}.html">{"0–9" if x == "0" else x.upper()}</a>' for x in letters)
    body = f"""<h1>Windows games on Mac: {label}</h1>
<p class="lead">Estimates from Linux (Proton) player reports on ProtonDB; not run on a Mac yet. <a href="./">Back to the list</a></p>
<p>{links}</p>
{table("en", sorted(games, key=lambda g: g["name"].lower()))}
<p>Linux data: <a href="https://github.com/bdefore/protondb-data">ProtonDB</a> (ODbL).</p>"""
    return f"mac/list-{c}.html", page("en", f"Windows games on Mac starting with {label} | Soju",
                                      f"Which Windows games starting with {label} may run on Apple Silicon Macs, estimated from ProtonDB reports.",
                                      SITE + f"mac/list-{c}.html", [], body, None, "../", noindex=True)


def attach_reports(games):
    """A game Soju users report running, or have played in Soju (site/soju_plays.py), is labelled
    as playing, unless it is already checked, blocked by anti-cheat or has a Mac version."""
    by_app, by_slug = {}, {}
    for r in REPORTS:
        if r.get("appid"):
            by_app.setdefault(r["appid"], []).append(r)
        slug = slugify(r["name"])
        by_slug.setdefault(slug, []).append(r)
        roman = re.sub(r"(?<=-)(2|3|4|5)$", lambda m: {"2": "ii", "3": "iii", "4": "iv", "5": "v"}[m.group(1)], slug)
        if roman != slug:
            by_slug.setdefault(roman, []).append(r)   # "Dark Souls 3" is the page dark-souls-iii
    played_app = {p["game"].split(":", 1)[1] for p in PLAYS if p["game"].startswith("steam:")}
    played_slug = {slugify(p["title"]) for p in PLAYS if p.get("title")}
    for g in games:
        if g["status"] in ("verified", "blocked", "blocked-online", "native"):
            continue
        reps = by_app.get(g.get("appid") or "") or by_slug.get(g["slug"]) or by_slug.get(slugify(g["name"])) or []
        played = (g.get("appid") or "") in played_app or g["slug"] in played_slug or slugify(g["name"]) in played_slug
        if played or any(r["works"] for r in reps):
            g["status"] = "soju-works"
            g["detail"] = ("Soju users play this game on Apple Silicon Macs. Install Soju, sign in to the game's "
                           "launcher, install the game and press Play.")
            g["linux"] = None
            if "ko" in g:
                g["ko"]["detail"] = "소주로 애플 실리콘 맥에서 플레이되는 게임입니다. 소주를 설치하고 런처에 로그인한 뒤 게임을 설치해 실행하면 됩니다."


def main():
    curated = DATA["games"]
    est = estimates(curated)
    allg = curated + est
    attach_reports(allg)
    out = {}
    for g in allg:
        p, h = game_page("en", g); out[p] = h
        if "ko" in g:
            p, h = game_page("ko", g); out[p] = h
    groups = {}
    for g in est:
        groups.setdefault(bucket(g["name"]), []).append(g)
    letters = sorted(groups, key=lambda c: (c == "0", c))
    for c in letters:
        p, h = letter_page(c, groups[c], letters); out[p] = h
    p, h = index_page("en", curated, est, letters); out[p] = h
    # curated first, then estimates with the most reports first
    (DOCS / "game-search.js").write_text(search_js())
    (DOCS / "mac/search.json").write_text(search_index(curated, sorted(est, key=lambda g: -g.get("reports", 0))))
    p, h = index_page("ko", [g for g in curated if "ko" in g], [g for g in est if "ko" in g], letters); out[p] = h
    for old in list((DOCS / "mac").glob("*.html")) + list((DOCS / "ko/mac").glob("*.html")):
        if str(old.relative_to(DOCS)) not in out:
            old.unlink()
    for p, h in out.items():
        f = DOCS / p
        f.parent.mkdir(parents=True, exist_ok=True)
        if not f.exists() or f.read_text() != h:
            f.write_text(h)
    sm = DOCS / "sitemap.xml"
    s = sm.read_text()
    urls = []
    for p in sorted(out):
        # Only a folder index loses its file name: valve-index.html is a page of its own.
        loc = SITE + (p[:-len("index.html")] if p == "index.html" or p.endswith("/index.html") else p)
        if loc in HIDDEN:
            continue
        urls.append(f"  <url><loc>{loc}</loc><lastmod>{CHECKED}</lastmod></url>")
    block = "  <!-- mac-games -->\n" + "\n".join(urls) + "\n  <!-- /mac-games -->\n"
    if "<!-- mac-games -->" in s:
        s = re.sub(r"  <!-- mac-games -->.*?<!-- /mac-games -->\n", lambda m: block, s, flags=re.S)
    else:
        s = s.replace("</urlset>", block + "</urlset>")
    sm.write_text(s)
    print("built", len(out), "pages,", len(est), "estimates,", len(HIDDEN), "kept out of search")


if __name__ == "__main__":
    main()
