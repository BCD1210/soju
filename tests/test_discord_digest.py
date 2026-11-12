import importlib.util
from datetime import datetime, timezone
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("discord_digest", ROOT / "scripts/discord-digest.py")
d = importlib.util.module_from_spec(spec)
spec.loader.exec_module(d)
NOW = datetime(2026, 11, 12, 20, 0, tzinfo=timezone.utc)


def release(tag, at, body="- One thing changed.\nSHA-256 Soju.zip: abc"):
    return {"tag_name": tag, "published_at": at, "body": body, "draft": False}


class DigestTests(unittest.TestCase):
    def test_only_app_releases_in_the_window(self):
        found = d.recent_releases(24, NOW, [
            release("v1.14.16", "2026-11-12T19:11:00Z"), release("v1.14.13", "2026-11-12T07:53:00Z"),
            release("steam-support-v4", "2026-11-12T10:00:00Z"), release("v1.14.8", "2026-11-11T05:00:00Z")])
        self.assertEqual([r["tag_name"] for r in found], ["v1.14.13", "v1.14.16"])
        early = dict(release("v1.15.0", "2026-11-12T19:30:00Z"), prerelease=True)
        self.assertEqual(d.recent_releases(24, NOW, [early]), [])

    def test_release_message_has_the_notes_and_no_checksums(self):
        text = d.releases_message([release("v1.14.15", "2026-11-12T16:00:00Z", "- Older change."),
                                   release("v1.14.16", "2026-11-12T19:11:00Z")])
        self.assertIn("**Soju 1.14.16 is out** (2 releases", text)
        self.assertLess(text.index("**1.14.16**"), text.index("**1.14.15**"))
        self.assertIn("• One thing changed.", text)
        self.assertNotIn("SHA-256", text)
        self.assertEqual(d.releases_message([]), "")

    def test_issue_text_cannot_ping_or_break_out(self):
        event = {"action": "opened", "issue": {"title": "[Compatibility]: Elden Ring @everyone", "html_url": "https://x/1",
                 "user": {"login": "someone"}, "body": "### Game\nElden Ring\n\n_No response_\n@here ```"}}
        text = d.issue_message(event)
        self.assertNotIn("@everyone", text)
        self.assertNotIn("@here", text)
        self.assertNotIn("```", text)
        self.assertIn("Elden Ring", text)
        self.assertEqual(d.issue_message({"action": "opened", "issue": {"title": "A bug"}}), "")
        self.assertEqual(d.issue_message({"action": "edited", "issue": {"title": "[Compatibility]: X"}}), "")

    def test_weekly_lists_releases_and_checked_games_or_stays_quiet(self):
        games = [{"slug": "diablo-4", "name": "Diablo IV", "status": "verified", "verified_on": "2026-11-11"},
                 {"slug": "old", "name": "Old", "status": "verified", "verified_on": "2026-08-01"}]
        text = d.weekly_message([release("v1.14.16", "2026-11-12T19:11:00Z")], games, 4, NOW)
        self.assertIn("1 release, now at 1.14.16", text)
        self.assertIn("Diablo IV", text)
        self.assertNotIn("Old", text)
        self.assertIn("4 game reports are open", text)
        self.assertEqual(d.weekly_message([], [], 3, NOW), "")

    def test_long_text_is_split_under_the_discord_limit(self):
        parts = d.chunks("\n".join("• line %d " % i + "x" * 80 for i in range(60)))
        self.assertGreater(len(parts), 1)
        self.assertTrue(all(len(p) <= d.LIMIT for p in parts))

    def test_the_workflow_never_puts_issue_text_in_a_shell_command(self):
        workflow = (ROOT / ".github/workflows/discord.yml").read_text()
        self.assertNotIn("github.event.issue.title", workflow)
        self.assertNotIn("github.event.issue.body", workflow)
        self.assertIn("discord-digest.py issue", workflow)


if __name__ == "__main__":
    unittest.main()
