"""Play warns first for a game whose makers keep its anti-cheat off Wine (scripts/soju_anticheat.py)."""
import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'scripts'))
import soju_anticheat as ac

spec = importlib.util.spec_from_file_location('game_library', ROOT / 'scripts/game-library.py')
lib = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lib)


class Denied(unittest.TestCase):
    def test_denied_games_by_store_id_or_title(self):
        self.assertEqual(ac.denied('Grand Theft Auto V Enhanced'), 'BattlEye')
        self.assertEqual(ac.denied('Fortnite'), 'Easy Anti-Cheat')
        self.assertIn('BattlEye', ac.denied('Rainbow Six', '359550'))
        # Easy Anti-Cheat that only allows Linux (Proton) still stops it on a Mac.
        self.assertEqual(ac.denied('Rocket League®'), 'Easy Anti-Cheat')
        self.assertEqual(ac.denied('x', '4262460'), 'Easy Anti-Cheat')
        self.assertEqual(ac.denied('Heaven Burns Red', '1973710'), 'Anti-Cheat Expert')
        self.assertEqual(ac.denied("Tom Clancy's Rainbow Six® Siege"), ac.denied('x', '359550'))
        # Games that run (or whose anti-cheat allows Proton) are not touched.
        for title in ('Hogwarts Legacy', 'Diablo II: Resurrected', 'Elden Ring'):
            self.assertEqual(ac.denied(title), '', title)

    def test_vanguard_in_the_folder_blocks_any_game(self):
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / 'Riot Vanguard').mkdir()
            (Path(d) / 'Riot Vanguard' / 'vgk.sys').write_text('')
            self.assertEqual(ac.denied('Some Riot Game', path=d), 'Riot Vanguard')

    def test_library_entry_says_why(self):
        text = lib.Library.blocked('epic', 'Fortnite', 'Fortnite', None)
        self.assertIn('Easy Anti-Cheat', text)
        self.assertIsNone(lib.Library.blocked('steam', '1385950', 'Guilt Free', None))


if __name__ == '__main__':
    unittest.main()
