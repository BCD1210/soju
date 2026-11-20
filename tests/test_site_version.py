"""The site and READMEs offer the current release: a release PR that bumps VERSION must also bump
the download links (site/content/*.json, READMEs) and rebuild the site (node site/build.mjs,
python3 site/mac_games.py), or new users download an old version."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parent.parent
VERSION = (ROOT / "VERSION").read_text().strip()
LINK = re.compile(r"Soju-(\d+\.\d+\.\d+)-macos-arm64\.zip")
FILES = ["README.md", "README.ko.md", "README.zh.md", "docs/DESKTOP.md", "site/content/index.json",
         "site/content/ko.json", "docs/index.html", "docs/ko/index.html", "docs/mac/index.html", "docs/ko/mac/index.html"]


class SiteVersion(unittest.TestCase):
    def test_download_links_name_current_version(self):
        for name in FILES:
            found = set(LINK.findall((ROOT / name).read_text()))
            with self.subTest(file=name):
                self.assertTrue(found, f"{name} has no download link")
                self.assertEqual(found, {VERSION}, f"{name} links {sorted(found)}, VERSION is {VERSION}")


if __name__ == "__main__":
    unittest.main()
