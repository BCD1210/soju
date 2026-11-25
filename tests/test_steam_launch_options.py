"""Soju gives known Steam games the launch options they need (scripts/steam-profiles.py options)."""
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('steam_profiles', ROOT / 'scripts/steam-profiles.py')
sp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sp)

CONFIG = '''"UserLocalConfigStore"
{
\t"Software"
\t{
\t\t"Valve"
\t\t{
\t\t\t"Steam"
\t\t\t{
\t\t\t\t"apps"
\t\t\t\t{
\t\t\t\t\t"220"
\t\t\t\t\t{
\t\t\t\t\t\t"LaunchOptions"\t\t"-novid"
\t\t\t\t\t}
\t\t\t\t\t"4000"
\t\t\t\t\t{
\t\t\t\t\t\t"LastPlayed"\t\t"1700000000"
\t\t\t\t\t}
\t\t\t\t}
\t\t\t}
\t\t}
\t}
}
'''


def options(text, appid):
    node = sp.vdf_tree(text)
    for key in ('userlocalconfigstore', 'software', 'valve', 'steam', 'apps', appid):
        node = node['blocks'][key]
    return node['values'].get('launchoptions')


class LaunchOptions(unittest.TestCase):
    def test_adds_to_an_existing_game_block(self):
        out = sp.with_launch_options(CONFIG, '4000', '-nochromium')
        self.assertEqual(options(out, '4000'), '-nochromium')
        self.assertEqual(options(out, '220'), '-novid')

    def test_adds_a_new_game_block(self):
        out = sp.with_launch_options(CONFIG, '312520', '-a "b"')
        self.assertEqual(options(out, '312520'), '-a "b"')
        self.assertEqual(options(out, '4000'), None)

    def test_keeps_the_players_own_options(self):
        self.assertIsNone(sp.with_launch_options(CONFIG, '220', '-other'))

    def test_writes_once_for_installed_games_only(self):
        with tempfile.TemporaryDirectory() as d:
            steam = Path(d) / 'drive_c/Program Files (x86)/Steam'
            (steam / 'steamapps').mkdir(parents=True)
            (steam / 'steamapps/appmanifest_4000.acf').write_text('"AppState"\n{\n\t"appid"\t\t"4000"\n\t"installdir"\t\t"GarrysMod"\n}\n')
            config = steam / 'userdata/123/config/localconfig.vdf'
            config.parent.mkdir(parents=True)
            config.write_text(CONFIG)
            table = {'4000': {'title': 'GMod', 'options': '-nochromium', 'why': 'x'},
                     '312520': {'title': 'Rain World', 'options': '-x', 'why': 'x'}}
            self.assertEqual(len(sp.set_launch_options(d, table)), 1)
            self.assertEqual(options(config.read_text(), '4000'), '-nochromium')
            self.assertNotIn('"312520"', config.read_text())
            # The player removes it: Soju does not put it back.
            config.write_text(CONFIG)
            self.assertEqual(sp.set_launch_options(d, table), [])
            self.assertIsNone(options(config.read_text(), '4000'))

    def test_shipped_table(self):
        table = json.loads((ROOT / 'resources/steam-launch-options.json').read_text())['options']
        self.assertEqual(table['4000']['options'], '-nochromium')


if __name__ == '__main__':
    unittest.main()
