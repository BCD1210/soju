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
    def setUp(self):
        # report() writes the local play log under BASE: keep tests away from the real one.
        self._tmp = tempfile.TemporaryDirectory()
        self._base = mock.patch.object(ps, 'BASE', Path(self._tmp.name))
        self._base.start()

    def tearDown(self):
        self._base.stop()
        self._tmp.cleanup()

    def test_finished_games_go_to_the_local_play_log_with_sharing_off_too(self):
        from datetime import datetime, timezone, timedelta
        now = datetime(2026, 11, 20, 2, 30, tzinfo=timezone(timedelta(hours=-8)))
        ps.log_local('steam:1', 'A', 45, now=now)
        ps.log_local('steam:2', 'B', 0, now=now)
        with mock.patch.object(ps, 'enabled', lambda: False), \
             mock.patch.object(ps, 'send', side_effect=AssertionError('sent while off')):
            ps.report('steam:3', 'C', 'steam', True, 5, 'soju')
        lines = [json.loads(l) for l in (ps.BASE / 'play-log.jsonl').read_text().splitlines()]
        self.assertEqual([l['game'] for l in lines], ['steam:1', 'steam:3'])
        self.assertEqual((lines[0]['start'], lines[0]['minutes']), ('2026-11-20T01:45:00-08:00', 45))
        self.assertEqual(oct(os.stat(ps.BASE / 'play-log.jsonl').st_mode & 0o777), '0o600')

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

    def test_a_start_that_did_not_run_says_why_from_the_launch_log_and_the_game_folder(self):
        found = ['/x/steam-bottle/drive_c/games/thing/', 'c:\\games\\thing\\']
        noise = '02fc:err:module:import_dll Library SDL2.dll (which is needed by L"C:\\\\Program Files (x86)\\\\Steam\\\\bin\\\\gldriverquery.exe") not found\n'
        self.assertEqual(ps.why(noise, found), ('', ''))
        mine = noise + '0123:err:module:import_dll Library VCRUNTIME140.dll (which is needed by L"C:\\\\games\\\\thing\\\\Thing.exe") not found\n'
        self.assertEqual(ps.why(mine, found), ('missing_dll', 'vcruntime140.dll'))
        self.assertEqual(ps.why('wine: Unhandled page fault on read access to 0000000000000000 at address 0000000140001000 (thread 0124)', found), ('crash', 'page_fault'))
        self.assertEqual(ps.why('Unhandled exception: page fault on read access to 0x00000000 in 64-bit code (0x0000000140001000).', found), ('crash', 'page_fault'))
        self.assertEqual(ps.why('wine: Unhandled exception 0xc000001d in thread 124 at address 0000000140001000', found)[0], 'crash')
        self.assertEqual(ps.why('0124:err:d3d:wined3d_caps_gl_ctx_create Failed to find a suitable pixel format.', found)[0], 'graphics')
        with tempfile.TemporaryDirectory() as d:
            game = Path(d) / 'steamapps-common-thing'
            (game / 'EasyAntiCheat').mkdir(parents=True)
            self.assertEqual(ps.anticheat(game), 'eac')
            self.assertEqual(ps.anticheat(Path(d) / 'nowhere-at-all'), '')
            with mock.patch.object(ps, 'BASE', Path(d)):
                (Path(d) / 'logs').mkdir()
                log = Path(d) / 'logs/desktop-steam.log'
                log.write_text('earlier: wine: Unhandled page fault on read access\n')
                marks = ps.log_marks('steam:9', 'steam')
                with open(log, 'a') as f: f.write(mine)
                self.assertEqual(ps.reason(True, 0, marks, found, game), {'why': 'missing_dll', 'detail': 'vcruntime140.dll', 'ac': 'eac'})
                self.assertEqual(ps.reason(True, 5, marks, found, game), {})

    def test_watch_stops_when_steam_is_gone(self):
        clock = Clock()
        with tempfile.TemporaryDirectory() as d, mock.patch.object(ps, 'BASE', Path(d)), \
             mock.patch.object(ps, 'STEAM_GRACE', 60):
            ps.watch(clock=clock.now, sleep=clock.sleep, poll=20, table=lambda: '', snapshot=lambda: {}, deliver=None)
        self.assertLess(clock.t, 200)

    def test_turning_sharing_off_stops_the_watch_and_sends_nothing(self):
        snap = {'games': [{'id': 'steam:1', 'platform': 'steam', 'title': 'A',
                           'install_path': '/b/drive_c/Program Files (x86)/Steam/steamapps/common/A'}]}
        line = 'c:\\program files (x86)\\steam\\steam.exe\nc:\\program files (x86)\\steam\\steamapps\\common\\a\\a.exe'
        clock, got, polls = Clock(), [], []
        with tempfile.TemporaryDirectory() as d, mock.patch.object(ps, 'BASE', Path(d)), \
             mock.patch.object(ps, 'OFF', Path(d) / 'stats-off'):
            def table():
                polls.append(1)
                if len(polls) == 5:
                    (Path(d) / 'stats-off').touch()   # the user turns sharing off mid-game
                return line
            ps.watch(clock=clock.now, sleep=clock.sleep, poll=20, table=table,
                     snapshot=lambda: snap, deliver=lambda *x: got.append(x))
            self.assertEqual(list((Path(d) / 'stats-running').glob('*.pid')), [])
        self.assertEqual(got, [])
        self.assertLess(len(polls), 8)
        with mock.patch.object(ps, 'enabled', lambda: False), \
             mock.patch.object(ps, 'send', side_effect=AssertionError('sent while off')):
            ps.report('steam:1', 'A', 'steam', True, 9, 'soju')

    def test_the_watch_reads_steam_itself_for_games_installed_after_the_last_refresh(self):
        with tempfile.TemporaryDirectory() as d:
            base = Path(d)
            client = base / 'steam-bottle/drive_c/Program Files (x86)/Steam'
            (client / 'steamapps/common/New Game').mkdir(parents=True)
            (client / 'steam.exe').write_text('')
            (client / 'steamapps/appmanifest_42.acf').write_text(
                '"AppState" { "appid" "42" "name" "New Game" "StateFlags" "4" "installdir" "New Game" }')
            with mock.patch.object(ps, 'BASE', base):
                games = ps.steam_games(ps.load_snapshot())
        self.assertIn('steam:42', games)
        self.assertTrue(any('steamapps\\common\\new game' in n for n in games['steam:42'][1]))


if __name__ == '__main__':
    unittest.main()
