import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("soju_reports", Path(__file__).resolve().parents[1] / "site/soju_reports.py")
sr = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sr)

BODY = """### Game

eFootball (Steam, App ID 1665460)

### Store

Steam

### Result

Works in-game

### Mac model, chip and memory

MacBook Air, M2, 16 GB

### macOS version

26.5

### Soju version

1.12.0

### Frame rate

55-60 FPS at 1080p

### How long you played

_No response_
"""


def issue(title, body=BODY):
    return {"number": 1, "html_url": "https://github.com/BCD1210/soju/issues/1", "created_at": "2026-11-11T00:00:00Z",
            "title": title, "body": body}


class Reports(unittest.TestCase):
    def test_form_fields(self):
        r = sr.parse(issue("[Compatibility]: eFootball"))
        self.assertEqual((r["name"], r["appid"], r["works"], r["fps"]), ("eFootball", "1665460", True, "55-60 FPS at 1080p"))
        self.assertNotIn("played", r)

    def test_failure_title_wins(self):
        self.assertFalse(sr.parse(issue("eFootball (Steam) exits immediately on Apple M2"))["works"])

    def test_incomplete_form_is_skipped(self):
        self.assertIsNone(sr.parse(issue("x", "### Game\n\nSomething\n")))


if __name__ == "__main__":
    unittest.main()
