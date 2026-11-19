import importlib.util
from datetime import datetime, timezone
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("release_health", ROOT / "scripts" / "release-health.py")
health = importlib.util.module_from_spec(spec)
spec.loader.exec_module(health)

NOW = datetime(2026, 11, 19, 12, 0, tzinfo=timezone.utc)


def release(tag, published, **extra):
    return dict({"tag_name": tag, "published_at": published, "draft": False, "prerelease": False}, **extra)


class ReleaseHealth(unittest.TestCase):
    def test_engine_url_comes_from_the_pinned_tag(self):
        self.assertEqual(health.engine_urls('REPO="x"\nENGINE_TAG="engine-v1.10"\n'),
                         ["https://github.com/BCD1210/soju/releases/download/engine-v1.10/wine-engine-x86_64.tar.xz"])
        self.assertEqual(health.engine_urls("echo no engine here"), [])

    def test_json_urls_are_found_at_any_depth(self):
        text = '{"url": "https://a/1", "parts": [{"url": "https://a/2", "source_url": "https://a/3"}]}'
        self.assertEqual(health.json_urls(text), ["https://a/1", "https://a/2"])

    def test_only_our_own_release_assets_are_ours_to_keep(self):
        self.assertTrue(health.own_release("https://github.com/BCD1210/soju/releases/download/steam-support-v9/x.tar.gz"))
        self.assertFalse(health.own_release("https://github.com/Gcenx/macOS_Wine_builds/releases/download/11.0_1/w.tar.xz"))

    def test_formula_version(self):
        self.assertEqual(health.formula_version('url "https://github.com/BCD1210/soju/archive/refs/tags/v1.14.31.tar.gz"'), "1.14.31")
        self.assertEqual(health.formula_version("nothing"), "")

    def test_newest_stable_orders_by_number_and_skips_other_tags(self):
        found = health.newest_stable([release("v1.14.9", "2026-11-01T00:00:00Z"), release("v1.14.31", "2026-11-18T00:00:00Z"),
                                      release("engine-v1.10", "2026-11-18T01:00:00Z"),
                                      release("v1.15.0", "2026-11-19T00:00:00Z", prerelease=True)])
        self.assertEqual(found["tag_name"], "v1.14.31")
        self.assertIsNone(health.newest_stable([]))

    def test_tap_behind_an_old_release_is_late(self):
        newest = release("v1.14.31", "2026-11-18T00:00:00Z")
        self.assertTrue(health.tap_is_late("1.14.16", newest, NOW, 3))
        self.assertFalse(health.tap_is_late("1.14.31", newest, NOW, 3))

    def test_tap_gets_a_grace_period_right_after_a_release(self):
        self.assertFalse(health.tap_is_late("1.14.30", release("v1.14.31", "2026-11-19T11:00:00Z"), NOW, 3))

    def test_this_checkout_pins_the_engine_and_the_support_packages(self):
        urls = health.pinned_here()
        self.assertTrue(any("/wine-engine-x86_64.tar.xz" in u for u in urls))
        self.assertTrue(any("soju-steam-support-" in u for u in urls))
        self.assertTrue(all(health.own_release(u) for u in urls))


if __name__ == "__main__":
    unittest.main()
