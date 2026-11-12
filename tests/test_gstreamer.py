import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]


def load(base):
    with mock.patch.dict(os.environ, {"SOJU_BASE": str(base)}):
        spec = importlib.util.spec_from_file_location("fetch_gstreamer", ROOT / "scripts/fetch-gstreamer.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
    return module


class GStreamerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="soju gstreamer ' ")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.g = load(self.base)

    def test_pinned_official_package(self):
        self.assertTrue(self.g.URL.startswith("https://gstreamer.freedesktop.org/data/pkg/osx/"))
        self.assertEqual(len(self.g.SHA256), 64)
        # libav carries the WMA decoder (avdec_wmav2); app/audioconvert/audioresample build Wine's pipeline.
        for plugin in ("libav", "app", "audioconvert", "audioresample", "coreelements"):
            self.assertIn(plugin, self.g.PLUGINS)
        # Video (#94): winegstreamer creates videoconvert, videoflip and deinterlace; H.264 needs h264parse.
        for plugin in ("videoconvertscale", "videofilter", "deinterlace", "videoparsersbad"):
            self.assertIn(plugin, self.g.PLUGINS)

    def test_installed_needs_marker_version_and_every_plugin(self):
        self.assertFalse(self.g.installed())
        plugins = self.base / "gstreamer/lib/gstreamer-1.0"
        plugins.mkdir(parents=True)
        (self.base / "gstreamer/.soju-gstreamer").write_text(self.g.VERSION + "\n")
        self.assertFalse(self.g.installed())
        (plugins / "libgstlibav.dylib").write_text("")
        # An audio-only install from 1.14.6 lacks the video plugins and is redone.
        self.assertFalse(self.g.installed())
        for name in self.g.PLUGINS:
            (plugins / f"libgst{name}.dylib").write_text("")
        self.assertTrue(self.g.installed())
        (self.base / "gstreamer/.soju-gstreamer").write_text("1.0.0\n")
        self.assertFalse(self.g.installed())

    def test_own_install_name_is_not_a_dependency(self):
        listing = ("/x/libgstopus.dylib (architecture x86_64):\n"
                   "\t@rpath/libgstopus.dylib (compatibility version 0.0.0)\n"
                   "\t@rpath/libopus.0.dylib (compatibility version 1.0.0)\n"
                   "\t/usr/lib/libSystem.B.dylib (compatibility version 1.0.0)\n")
        with mock.patch.object(self.g.subprocess, "run", return_value=mock.Mock(stdout=listing)):
            self.assertEqual(self.g.deps(Path("/x/libgstopus.dylib")), ["libopus.0.dylib"])

    def test_steam_gets_it(self):
        play = (ROOT / "scripts/play.sh").read_text()
        for var in ("GST_PLUGIN_SYSTEM_PATH_1_0", "GST_PLUGIN_SCANNER_1_0", "GST_REGISTRY_1_0"):
            self.assertIn(var, play)
        # The Steam environment drops DYLD_FALLBACK_LIBRARY_PATH first, so GStreamer's must be added after.
        self.assertIn('STEAM_ENV+=(DYLD_FALLBACK_LIBRARY_PATH="$GST/lib:/usr/lib"', play)
        self.assertIn("fetch-gstreamer.py", (ROOT / "scripts/setup-steam-games.sh").read_text())
        self.assertIn("fetch-gstreamer.py", (ROOT / "scripts/update.sh").read_text())


if __name__ == "__main__":
    unittest.main()
