import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


def load(base):
    os.environ["SOJU_BASE"] = str(base)
    spec = importlib.util.spec_from_file_location("game_settings", ROOT / "scripts/game-settings.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class GameSettingsTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="soju settings ' ")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.g = load(self.base)

    def test_only_steam_games_have_a_renderer(self):
        self.assertIsNone(self.g.show("battlenet:osi")["renderer"])
        self.assertIsNone(self.g.show("steam:../../x")["renderer"])
        # No Steam renderer components yet: nothing to choose.
        self.assertIsNone(self.g.show("steam:1385950")["renderer"])

    def test_gptk_mode_explains_instead_of_offering_a_choice(self):
        (self.base / "steam-gptk").write_text("")
        steam = self.base / "steam-gptk-bottle/drive_c/Program Files (x86)/Steam"
        steam.mkdir(parents=True)
        (steam / "steam.exe").write_text("")
        self.assertEqual(self.g.show("steam:1385950")["renderer"], {"available": False, "reason": "gptk"})
        with self.assertRaises(ValueError):
            self.g.set_renderer("steam:1385950", "dxvk")

    def test_bad_requests_are_refused(self):
        for game, mode in (("epic:1", "dxvk"), ("steam:1385950", "wined3d"), ("steam:abc", "dxvk")):
            with self.assertRaises(ValueError):
                self.g.set_renderer(game, mode)

    def test_app_and_action_are_wired(self):
        self.assertIn('game-settings) exec python3 "$ROOT/scripts/game-settings.py" "$@"',
                      (ROOT / "scripts/desktop-action.sh").read_text())
        self.assertIn('"$ROOT/app/GameSettings.swift"', (ROOT / "scripts/build-app.sh").read_text())
        self.assertIn("GameSettingsSection(game: game)", (ROOT / "app/Library.swift").read_text())


class HelpEntryPointTests(unittest.TestCase):
    def test_discord_is_offered_where_people_get_stuck(self):
        invite = "https://discord.gg/ttBDcF5ftz"
        self.assertIn(invite, (ROOT / ".github/ISSUE_TEMPLATE/config.yml").read_text())
        library = (ROOT / "app/Library.swift").read_text()
        self.assertIn('static let discord = URL(string: "' + invite + '")!', library)
        # The launch-failure alert and the Diagnostics sheet both offer it.
        self.assertEqual(library.count('Button("Ask on Discord")'), 2)


if __name__ == "__main__":
    unittest.main()
