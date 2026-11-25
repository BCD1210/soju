import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('report', ROOT / 'scripts/report.py')
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)


class PlayedTests(unittest.TestCase):
    def test_play_time_comes_from_the_play_log(self):
        with tempfile.TemporaryDirectory() as d:
            log = Path(d) / 'play-log.jsonl'
            log.write_text('\n'.join(json.dumps(e) for e in [
                {'game': 'steam:1', 'minutes': 50}, {'game': 'steam:2', 'minutes': 7},
                {'game': 'steam:1', 'minutes': 40}]) + '\nnot json\n')
            with mock.patch.object(report, 'BASE', Path(d)):
                self.assertEqual(report.played('steam:1'), "about 1.5 hours (Soju's play log)")
                self.assertEqual(report.played('steam:2'), "about 7 minutes (Soju's play log)")
                self.assertEqual(report.played('steam:3'), '')
            with mock.patch.object(report, 'BASE', Path(d) / 'missing'):
                self.assertEqual(report.played('steam:1'), '')


if __name__ == '__main__':
    unittest.main()
