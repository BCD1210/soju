import importlib.util
import os
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


def load(base):
    os.environ["SOJU_BASE"] = str(base)
    spec = importlib.util.spec_from_file_location("game_storage", ROOT / "scripts/game-storage.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class GameStorageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="soju storage ' ")
        self.addCleanup(self.temp.cleanup)
        top = Path(self.temp.name).resolve()
        self.base = top / "base"
        self.drive = top / "drive"
        self.drive.mkdir()
        self.root = self.base / "bottle/drive_c/Program Files (x86)"
        self.game = self.root / "Diablo IV"
        (self.game / "Data").mkdir(parents=True)
        (self.game / ".build.info").write_text("build")
        (self.game / "Data/a.bin").write_bytes(b"x" * 4096)
        self.s = load(self.base)
        # The test "drive" is a folder on the same disk; pretend each top folder is a volume.
        self.s.mount_of = lambda p: (self.drive if self.drive in [Path(p), *Path(p).parents] else self.base, "exfat")
        self.s.game = lambda game_id: ("battlenet", self.game)
        self.s.running = lambda platform, name: False

    def test_move_leaves_a_link_and_back_restores_the_folder(self):
        state = self.s.move("battlenet:fenris", str(self.drive))
        target = self.drive / "Soju Games/battlenet/Diablo IV"
        self.assertTrue(self.game.is_symlink())
        self.assertEqual(self.game.resolve(), target)
        self.assertEqual((self.game / "Data/a.bin").stat().st_size, 4096)
        self.assertTrue(state["moved"])
        self.assertEqual([p.name for p in self.root.iterdir()], ["Diablo IV"])
        with self.assertRaisesRegex(ValueError, "already on another drive"):
            self.s.move("battlenet:fenris", str(self.drive))
        state = self.s.back("battlenet:fenris")
        self.assertFalse(self.game.is_symlink())
        self.assertEqual((self.game / "Data/a.bin").stat().st_size, 4096)
        self.assertFalse(target.exists())
        self.assertFalse(state["moved"])

    def test_move_refuses_and_keeps_the_game_in_place(self):
        cases = []
        self.s.running = lambda platform, name: True
        cases.append("Close Battle.net")
        with self.assertRaisesRegex(ValueError, cases[0]):
            self.s.move("battlenet:fenris", str(self.drive))
        self.s.running = lambda platform, name: False
        with self.assertRaisesRegex(ValueError, "same disk"):
            self.s.move("battlenet:fenris", str(self.base))
        self.s.mount_of = lambda p: (self.drive, "ntfs")
        with self.assertRaisesRegex(ValueError, "not supported"):
            self.s.move("battlenet:fenris", str(self.drive))
        self.s.mount_of = lambda p: (self.drive if self.drive in [Path(p), *Path(p).parents] else self.base, "exfat")
        (self.game / "link").symlink_to("Data")
        with self.assertRaisesRegex(ValueError, "exFAT"):
            self.s.move("battlenet:fenris", str(self.drive))
        (self.game / "link").unlink()
        existing = self.drive / "Soju Games/battlenet/Diablo IV"
        existing.mkdir(parents=True)
        (existing / "other").write_text("x")
        with self.assertRaisesRegex(ValueError, "already has files"):
            self.s.move("battlenet:fenris", str(self.drive))
        self.assertFalse(self.game.is_symlink())
        self.assertTrue((self.game / "Data/a.bin").is_file())

    def test_a_copy_that_does_not_match_is_not_used(self):
        real = self.s.tree
        self.s.tree = lambda p: (9, 9, 0) if "Soju Games" in str(p) else real(p)
        with self.assertRaisesRegex(ValueError, "does not match"):
            self.s.move("battlenet:fenris", str(self.drive))
        self.assertFalse(self.game.is_symlink())
        self.assertTrue((self.game / "Data/a.bin").is_file())

    def test_import_links_an_existing_game_without_copying(self):
        have = self.drive / "Games/Overwatch"
        have.mkdir(parents=True)
        with self.assertRaisesRegex(ValueError, "not a Battle.net game"):
            self.s.adopt("battlenet", str(have))
        (have / ".build.info").write_text("build")
        result = self.s.adopt("battlenet", str(have))
        self.assertEqual((self.root / "Overwatch").resolve(), have)
        self.assertEqual(result["files"], str(have))
        with self.assertRaisesRegex(ValueError, "already has a folder"):
            self.s.adopt("battlenet", str(have))
        with self.assertRaisesRegex(ValueError, "already in Soju"):
            self.s.adopt("battlenet", str(self.game))

    def test_place_prepares_a_folder_on_the_drive(self):
        with self.assertRaisesRegex(ValueError, "folder name"):
            self.s.place("battlenet", "../x", str(self.drive))
        with self.assertRaisesRegex(ValueError, "already has a folder"):
            self.s.place("battlenet", "Diablo IV", str(self.drive))
        self.s.place("battlenet", "Overwatch", str(self.drive))
        self.assertEqual((self.root / "Overwatch").resolve(), self.drive / "Soju Games/battlenet/Overwatch")

    def test_exfat_side_files_do_not_count(self):
        before = self.s.tree(self.game)
        (self.game / "Data/._a.bin").write_bytes(b"x" * 4096)
        self.assertEqual(self.s.tree(self.game), before)

    def test_only_battlenet_games(self):
        s = load(self.base)
        with self.assertRaisesRegex(ValueError, "Battle.net"):
            s.game("steam:1")
        with self.assertRaisesRegex(ValueError, "Battle.net"):
            s.place("steam", "X", str(self.drive))

    def test_the_app_and_cli_reach_it(self):
        self.assertIn("game-storage.py", (ROOT / "scripts/desktop-action.sh").read_text())
        self.assertIn("game-storage.py", (ROOT / "scripts/soju").read_text())
        swift = (ROOT / "app/GameSettings.swift").read_text()
        self.assertIn('"game-storage", "move"', swift)
        self.assertIn('"game-storage", "back"', swift)
        self.assertIn("importBattlenetGame", (ROOT / "app/Accounts.swift").read_text())


class UnpluggedDriveTests(unittest.TestCase):
    def test_a_moved_game_stays_listed_while_its_drive_is_away(self):
        source = (ROOT / "scripts/game-library.py").read_text()
        self.assertIn("unplugged = location.is_symlink() and not location.exists()", source)
        self.assertIn("Connect the drive, then try again.", source)
        self.assertIn("self.games[key].update(volume=target[2], available=not unplugged)", source)
        self.assertIn("var isOnUnpluggedDrive: Bool { available == false }", (ROOT / "app/Library.swift").read_text())


if __name__ == "__main__":
    unittest.main()
