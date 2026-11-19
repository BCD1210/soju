import importlib.util
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('play_stats', ROOT / 'scripts/play-stats.py')
ps = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ps)


class Clock:
    def __init__(self): self.t = 0.0
    def now(self): return self.t
    def sleep(self, s): self.t += s


class PlayStats(unittest.TestCase):
    def test_needles_match_the_wine_command_line(self):
        found = ps.needles('/Users/a/.battlenet-macos/bottle/drive_c/Program Files (x86)/Diablo II Resurrected')
        self.assertIn('c:\\program files (x86)\\diablo ii resurrected\\', found)
        line = 'C:\\Program Files (x86)\\Diablo II Resurrected\\D2R.exe -launch'.lower()
        self.assertTrue(any(n in line for n in found))
        # A sibling folder with a longer name is a different game.
        found = ps.needles('/x/drive_c/Program Files (x86)/StarCraft')
        self.assertFalse(any(n in 'c:\\program files (x86)\\starcraft ii\\sc2.exe' for n in found))
        self.assertEqual(ps.needles(''), [])

    def test_track_counts_minutes_until_the_game_quits(self):
        clock = Clock()
        up = iter([False, True] + [True] * 30 + [False] * 5)
        with mock.patch.object(ps, 'running', lambda found, pid=None: next(up)):
            minutes = ps.track(['x'], clock=clock.now, sleep=clock.sleep, poll=20)
        self.assertEqual(minutes, 10)   # 31 checks seen, 20 s apart

    def test_track_gives_up_when_the_game_never_starts(self):
        clock = Clock()
        with mock.patch.object(ps, 'running', lambda found, pid=None: False):
            self.assertEqual(ps.track(['x'], clock=clock.now, sleep=clock.sleep, poll=60), 0)

    def test_library_payload_lists_only_installed_games_without_paths(self):
        snap = {'games': [
            {'id': 'steam:1', 'platform': 'steam', 'title': 'A', 'install_path': '/x/A'},
            {'id': 'steam:2', 'platform': 'steam', 'title': 'B', 'install_path': ''},
            {'id': 'xbox:3', 'platform': 'xbox', 'title': 'C', 'install_path': '/x/C', 'downloaded': False},
            {'id': 'xbox:4', 'platform': 'xbox', 'title': 'D', 'install_path': '/x/D', 'downloaded': True}]}
        p = ps.library_payload(snap)
        self.assertEqual([g['game'] for g in p['games']], ['steam:1', 'xbox:4'])
        self.assertNotIn('/x/', json.dumps(p))

    def test_off_switch_and_queue(self):
        with tempfile.TemporaryDirectory() as d:
            base = Path(d)
            with mock.patch.object(ps, 'BASE', base), mock.patch.object(ps, 'OFF', base / 'stats-off'), \
                 mock.patch.object(ps, 'QUEUE', base / 'q.jsonl'), mock.patch.dict(os.environ, {}, clear=False):
                os.environ.pop('SOJU_NO_STATS', None)
                self.assertTrue(ps.enabled())
                ps.main(['off'])
                self.assertFalse(ps.enabled())
                ps.main(['on'])
                self.assertTrue(ps.enabled())
                with mock.patch.object(ps, 'post', side_effect=OSError('offline')):
                    self.assertFalse(ps.send({'kind': 'play', 'games': []}))
                self.assertEqual(len((base / 'q.jsonl').read_text().splitlines()), 1)
                sent = []
                with mock.patch.object(ps, 'post', side_effect=lambda b: sent.append(b) or True):
                    self.assertTrue(ps.send({'kind': 'play', 'games': []}))
                self.assertEqual(len(sent), 2)
                self.assertFalse((base / 'q.jsonl').exists())
                self.assertEqual(sent[0]['id'], (base / 'stats-id').read_text().strip())


    def test_library_is_sent_only_when_it_changes(self):
        with tempfile.TemporaryDirectory() as d:
            base = Path(d)
            (base / 'logs').mkdir()
            snap = {'games': [{'id': 'steam:1', 'platform': 'steam', 'title': 'A', 'install_path': '/x/A'}]}
            (base / 'logs/library-scan.log').write_text(json.dumps(snap))
            sent = []
            with mock.patch.object(ps, 'BASE', base), mock.patch.object(ps, 'OFF', base / 'off'), \
                 mock.patch.object(ps, 'send', side_effect=lambda p: sent.append(p) or True):
                ps.main(['library']); ps.main(['library'])
                self.assertEqual(len(sent), 1)
                snap['games'].append({'id': 'steam:2', 'platform': 'steam', 'title': 'B', 'install_path': '/x/B'})
                (base / 'logs/library-scan.log').write_text(json.dumps(snap))
                ps.main(['library'])
                self.assertEqual(len(sent), 2)

    def test_a_game_is_counted_once_at_a_time(self):
        with tempfile.TemporaryDirectory() as d:
            with mock.patch.object(ps, 'BASE', Path(d)):
                first = ps.claim('steam:1')
                self.assertIsNotNone(first)
                self.assertIsNone(ps.claim('steam:1'))
                first.write_text('999999')          # a count that died without cleaning up
                self.assertIsNotNone(ps.claim('steam:1'))

    def test_track_run_tells_a_start_that_never_happened_from_a_quick_exit(self):
        clock = Clock()
        with mock.patch.object(ps, 'running', lambda found, pid=None: False):
            self.assertEqual(ps.track_run(['x'], clock=clock.now, sleep=clock.sleep, poll=60), (False, 0))
        clock = Clock()
        up = iter([True, False, False, False])
        with mock.patch.object(ps, 'running', lambda found, pid=None: next(up)):
            self.assertEqual(ps.track_run(['x'], clock=clock.now, sleep=clock.sleep, poll=20), (True, 0))
        self.assertEqual([ps.outcome(False, 0), ps.outcome(True, 0), ps.outcome(True, 5)], ['no_start', 'quick_exit', 'ran'])

    def test_every_start_reports_its_outcome_and_minutes_only_when_there_are_some(self):
        sent = []
        with mock.patch.object(ps, 'send', side_effect=lambda p: sent.append(p) or True):
            ps.report('steam:1', 'A', 'steam', True, 0, 'soju')
            ps.report('steam:1', 'A', 'steam', True, 7, 'steam')
        self.assertEqual([p['kind'] for p in sent], ['launch', 'launch', 'play'])
        self.assertEqual(sent[0]['games'][0]['result'], 'quick_exit')
        self.assertEqual((sent[1]['games'][0]['result'], sent[1]['games'][0]['how']), ('ran', 'steam'))
        self.assertEqual(sent[2]['games'][0]['minutes'], 7)

    def test_watch_counts_games_started_from_steam_and_leaves_play_button_counts_alone(self):
        snap = {'games': [
            {'id': 'steam:1', 'platform': 'steam', 'title': 'A', 'install_path': '/b/drive_c/Program Files (x86)/Steam/steamapps/common/A'},
            {'id': 'steam:2', 'platform': 'steam', 'title': 'B', 'install_path': '/b/drive_c/Program Files (x86)/Steam/steamapps/common/B'},
            {'id': 'gog:3', 'platform': 'gog', 'title': 'C', 'install_path': '/g/drive_c/C'}]}
        steam = 'c:\\program files (x86)\\steam\\steam.exe -no-cef-sandbox'
        a = 'c:\\program files (x86)\\steam\\steamapps\\common\\a\\a.exe'
        b = 'c:\\program files (x86)\\steam\\steamapps\\common\\b\\b.exe'
        frames = [steam] * 2 + [steam + '\n' + a + '\n' + b] * 10 + [steam] * 4 + [''] * 4
        it = iter(frames)
        clock, got = Clock(), []
        with tempfile.TemporaryDirectory() as d, mock.patch.object(ps, 'BASE', Path(d)), \
             mock.patch.object(ps, 'STEAM_GRACE', 0):
            held = ps.claim('steam:2')            # the Play button is already counting B
            ps.watch(clock=clock.now, sleep=clock.sleep, poll=20, table=lambda: next(it, ''),
                     snapshot=lambda: snap, deliver=lambda *x: got.append(x))
            self.assertTrue(held.exists())
            self.assertFalse((Path(d) / 'stats-running' / 'steam-1.pid').exists())
        self.assertEqual(got, [('steam:1', 'A', 'steam', True, 3, 'steam')])

    def test_watch_stops_when_steam_is_gone(self):
        clock = Clock()
        with tempfile.TemporaryDirectory() as d, mock.patch.object(ps, 'BASE', Path(d)), \
             mock.patch.object(ps, 'STEAM_GRACE', 60):
            ps.watch(clock=clock.now, sleep=clock.sleep, poll=20, table=lambda: '', snapshot=lambda: {}, deliver=None)
        self.assertLess(clock.t, 200)


if __name__ == '__main__':
    unittest.main()
