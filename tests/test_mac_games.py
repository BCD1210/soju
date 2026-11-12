"""Game pages under docs/mac/ match site/mac-games.json (run site/mac_games.py after editing it)."""
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parent.parent
DATA = json.loads((ROOT / "site/mac-games.json").read_text())


class MacGamePages(unittest.TestCase):
    def test_entries(self):
        slugs = [g["slug"] for g in DATA["games"]]
        self.assertEqual(len(slugs), len(set(slugs)))
        for g in DATA["games"]:
            self.assertIn(g["status"], ("verified", "untested", "blocked", "blocked-online", "native"))
            self.assertRegex(g["slug"], r"^[a-z0-9-]+$")
            if g["status"] == "verified":
                self.assertRegex(g.get("verified_on", ""), r"^\d{4}-\d{2}-\d{2}$")

    def test_pages_and_sitemap_are_built(self):
        sitemap = (ROOT / "docs/sitemap.xml").read_text()
        for g in DATA["games"]:
            page = ROOT / "docs/mac" / (g["slug"] + ".html")
            self.assertTrue(page.is_file(), page)
            self.assertIn("https://didimlabs.com/mac/%s.html" % g["slug"], sitemap)
            self.assertIn(g["name"].replace("'", "&#x27;"), page.read_text())
            if "ko" in g:
                self.assertTrue((ROOT / "docs/ko/mac" / (g["slug"] + ".html")).is_file())

    def test_every_sitemap_address_is_a_page(self):
        # A slug ending in "index" (Valve Index games) once lost ".html" and led Google to 404s.
        import re
        for loc in re.findall(r"<loc>https://didimlabs\.com/([^<]*)</loc>", (ROOT / "docs/sitemap.xml").read_text()):
            path = ROOT / "docs" / (loc + "index.html" if loc == "" or loc.endswith("/") else loc)
            self.assertTrue(path.is_file(), loc)


if __name__ == "__main__":
    unittest.main()
