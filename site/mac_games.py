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
KO_ESTIMATES = 3000   # Korean pages for the most-reported estimated games
INDEX_ESTIMATES = 300
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
           "native": ("Has a Mac version", "native"),
           "likely": ("Estimate: likely runs", "likely"), "mixed": ("Estimate: may have problems", "unknown"),
           "unlikely": ("Estimate: unlikely to run", "no"), "few": ("Estimate: few reports", "unknown")},
    "ko": {"verified": ("소주에서 됨", "ok"), "untested": ("소주에서 아직 미확인", "unknown"),
           "blocked": ("안 됨: 안티치트", "no"), "blocked-online": ("온라인은 안티치트로 막힘", "no"),
           "native": ("맥 버전 있음", "native"),
           "likely": ("추정: 될 가능성 높음", "likely"), "mixed": ("추정: 문제 있을 수 있음", "unknown"),
           "unlikely": ("추정: 안 될 가능성 높음", "no"), "few": ("추정: 보고 적음", "unknown")},
}
ESTIMATE = ("likely", "mixed", "unlikely", "few")


def header_image(appid):
    return f"https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/{appid}/header.jpg"
CSS_EXTRA = ""


def lname(lang, key):
    return "자체 런처" if lang == "ko" and key == "none" else LAUNCHER[key][0]


def esc(s):
    return html.escape(s, quote=True)


def page(lang, title, desc, canonical, alternates, body, faq, base, game=None, image=None):
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
<meta property="og:image" content="{image or SITE + 'assets/soju-share.png'}">
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
    launcher = lname(lang, g["launcher"])
    base = "../../" if lang == "ko" else "../"
    path = ("ko/mac/" if lang == "ko" else "mac/") + g["slug"] + ".html"
    canonical = SITE + path
    alternates = [("en", SITE + "mac/" + g["slug"] + ".html")]
    if "ko" in g:
        alternates.append(("ko", SITE + "ko/mac/" + g["slug"] + ".html"))
    runnable = g["status"] in ("verified", "untested", "blocked-online") + ESTIMATE
    est = g["status"] in ESTIMATE
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
        report = (f'해 보셨나요? <a href="{DISCORD}">디스코드</a>나 <a href="{REPORT}">호환성 보고</a>로 알려 주시면 이 페이지에 반영합니다.')
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
        report = (f'Tried it? Tell us in <a href="{DISCORD}">Discord</a> or <a href="{REPORT}">send a compatibility report</a> '
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
    if g.get("appid"):
        extra = (f'<p><img class="shot" src="{header_image(g["appid"])}" width="460" height="215" loading="lazy" '
                 f'alt="{esc(name)}"></p>\n' + extra +
                 f'<p><a href="https://store.steampowered.com/app/{g["appid"]}/">'
                 + ("스팀 상점 페이지" if lang == "ko" else "Steam store page") + "</a></p>")
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
<p>{report}</p>
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
    return path, page(lang, title, desc, canonical, alternates, body, faq, base, game, image)


def slugify(title):
    t = re.sub(r"[\u2122\u00ae\u00a9]", "", title)   # ™ ® © would become "tm" etc.
    t = unicodedata.normalize("NFKD", t).encode("ascii", "ignore").decode().lower()
    return re.sub(r"[^a-z0-9]+", "-", t).strip("-")


def estimates(curated):
    """Pages for Steam games with ProtonDB reports; curated entries win and get the Linux line."""
    dump = EST["dump_newest_report"]
    by_slug = {g["slug"]: g for g in curated}
    by_name = {slugify(g["name"]): g for g in curated}
    out, used = [], set(by_slug)
    for appid, title, n, y, recent, _ in EST["apps"]:
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
             "appid": appid, "reports": n}
        if len([e for e in out if "ko" in e]) < KO_ESTIMATES:
            g["ko"] = {"name": title, "detail": ko}
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


def index_page(lang, curated, est, letters):
    base = "../../" if lang == "ko" else "../"
    path = "ko/mac/index.html" if lang == "ko" else "mac/index.html"
    canonical = SITE + ("ko/mac/" if lang == "ko" else "mac/")
    order = ["verified", "untested", "blocked-online", "blocked", "native"]
    cur = sorted(curated, key=lambda g: (order.index(g["status"]), g["name"]))
    top = sorted(est, key=lambda g: -g["reports"])[:INDEX_ESTIMATES]
    if lang == "ko":
        title, h1 = f"맥에서 되는 윈도우 게임 목록 ({CHECKED[:4]}) | 소주", "맥(애플 실리콘)에서 이 게임 되나요?"
        desc = "디아블로, 오버워치, 배틀그라운드, 발로란트, GTA 5와 스팀 게임 수천 개가 애플 실리콘 맥에서 되는지: 직접 확인한 것과 리눅스 사용자 보고(ProtonDB) 기반 추정."
        intro = ("'됨'은 소주에서 직접 실행해 본 게임입니다. '추정'은 리눅스(Proton) 사용자 보고를 바탕으로 한 예상이고 맥에서는 아직 확인하지 않았습니다.")
        h_cur, h_est = "직접 확인했거나 확실한 게임", f"추정 (ProtonDB 보고가 많은 {INDEX_ESTIMATES}개)"
        more = "알파벳순 전체 목록(영문): " 
    else:
        title, h1 = f"Which Windows games run on Apple Silicon Macs ({CHECKED[:4]}) | Soju", "Does this game run on a Mac?"
        desc = f"Diablo, Overwatch, GTA V, Apex, Fortnite, Valorant and {len(est):,} Steam games on Apple Silicon: what was run in Soju, and estimates from Linux player reports (ProtonDB)."
        intro = ("'Runs in Soju' means we ran it. 'Estimate' pages are based on Linux (Proton) player reports from "
                 "ProtonDB and have not been run on a Mac yet.")
        h_cur, h_est = "Checked or known", f"Estimates (the {INDEX_ESTIMATES} most reported)"
        more = "All estimated games A–Z: "
    lp = "../../mac/" if lang == "ko" else ""
    links = " ".join(f'<a href="{lp}list-{c}.html">{c.upper() if c != "0" else "0–9"}</a>' for c in letters)
    body = f"""<h1>{h1}</h1>
<p class="lead">{intro}</p>
<h2>{h_cur}</h2>
{table(lang, cur)}
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
                                      SITE + f"mac/list-{c}.html", [], body, None, "../")


def main():
    curated = DATA["games"]
    est = estimates(curated)
    allg = curated + est
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
        loc = SITE + (p[:-len("index.html")] if p.endswith("index.html") else p)
        urls.append(f"  <url><loc>{loc}</loc><lastmod>{CHECKED}</lastmod></url>")
    block = "  <!-- mac-games -->\n" + "\n".join(urls) + "\n  <!-- /mac-games -->\n"
    if "<!-- mac-games -->" in s:
        s = re.sub(r"  <!-- mac-games -->.*?<!-- /mac-games -->\n", lambda m: block, s, flags=re.S)
    else:
        s = s.replace("</urlset>", block + "</urlset>")
    sm.write_text(s)
    print("built", len(out), "pages,", len(est), "estimates")


if __name__ == "__main__":
    main()
