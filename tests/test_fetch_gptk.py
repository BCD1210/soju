import importlib.util
import io
import tarfile
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("fetch_gptk", ROOT / "scripts/fetch-gptk.py")
fetch_gptk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fetch_gptk)

PREFIX = "Toolkit.app/Contents/Resources/wine/lib"
MANIFEST = {"prefix": PREFIX, "shims": ["d3d11.dll", "d3d12.dll", "dxgi.dll", "nvapi64.dll"]}
FRAMEWORK = PREFIX + "/external/D3DMetal.framework"


def archive(path, entries):
    """entries: (name, bytes) for a file, (name, None) for a directory, (name, 'link:target') for a link."""
    with tarfile.open(path, "w:xz") as tar:
        for name, body in entries:
            info = tarfile.TarInfo(name)
            if body is None:
                info.type = tarfile.DIRTYPE; info.mode = 0o755; tar.addfile(info)
            elif isinstance(body, str):
                info.type = tarfile.SYMTYPE; info.linkname = body[5:]; tar.addfile(info)
            else:
                info.size = len(body); info.mode = 0o755; tar.addfile(info, io.BytesIO(body))


def payload(extra=()):
    return [
        (PREFIX + "/external", None),
        (PREFIX + "/external/libd3dshared.dylib", b"shared"),
        (FRAMEWORK + "/Versions/A/D3DMetal", b"framework"),
        (FRAMEWORK + "/Versions/Current", "link:A"),
        (FRAMEWORK + "/D3DMetal", "link:Versions/Current/D3DMetal"),
        (PREFIX + "/wine/x86_64-windows/d3d11.dll", b"11"),
        (PREFIX + "/wine/x86_64-windows/d3d12.dll", b"12"),
        (PREFIX + "/wine/x86_64-windows/dxgi.dll", b"gi"),
        (PREFIX + "/wine/x86_64-windows/ntdll.dll", b"wine's own"),
        (PREFIX + "/wine/x86_64-windows/nvngx.dll", b"not an Apple shim"),
        ("Toolkit.app/Contents/MacOS/wine", b"launcher"),
    ] + list(extra)


class FetchGptkTests(unittest.TestCase):
    def unpack(self, entries):
        tmp = tempfile.TemporaryDirectory(); self.addCleanup(tmp.cleanup)
        tar = Path(tmp.name) / "a.tar.xz"; stage = Path(tmp.name) / "lib"; stage.mkdir()
        archive(tar, entries)
        fetch_gptk.unpack(tar, stage, MANIFEST)
        return stage

    def test_takes_only_the_payload_and_listed_shims(self):
        stage = self.unpack(payload())
        files = sorted(str(p.relative_to(stage)) for p in stage.rglob("*") if not p.is_dir() or p.is_symlink())
        self.assertEqual(files, [
            "external/D3DMetal.framework/D3DMetal",
            "external/D3DMetal.framework/Versions/A/D3DMetal",
            "external/D3DMetal.framework/Versions/Current",
            "external/libd3dshared.dylib",
            "wine/x86_64-windows/d3d11.dll", "wine/x86_64-windows/d3d12.dll", "wine/x86_64-windows/dxgi.dll",
        ])
        self.assertTrue((stage / "external/D3DMetal.framework/Versions/Current").is_symlink())
        self.assertEqual((stage / "external/D3DMetal.framework/D3DMetal").read_bytes(), b"framework")

    def test_rejects_a_link_that_leaves_the_framework(self):
        for target in ("../../../../../../etc/passwd", "/etc/passwd", "../libd3dshared.dylib"):
            with self.assertRaises(ValueError, msg=target):
                self.unpack(payload([(FRAMEWORK + "/Resources", "link:" + target)]))

    def test_rejects_a_link_outside_the_framework(self):
        with self.assertRaises(ValueError):
            self.unpack(payload([(PREFIX + "/external/extra.dylib", "link:libd3dshared.dylib")]))

    def test_rejects_traversal_and_duplicates(self):
        with self.assertRaises(ValueError):
            self.unpack(payload([(PREFIX + "/external/../../evil", b"x")]))
        with self.assertRaises(ValueError):
            self.unpack(payload([(PREFIX + "/external/libd3dshared.dylib", b"again")]))

    def test_requires_the_metal_side_and_the_three_shims(self):
        with self.assertRaises(ValueError):
            self.unpack([e for e in payload() if not e[0].endswith("libd3dshared.dylib")])
        with self.assertRaises(ValueError):
            self.unpack([e for e in payload() if not e[0].endswith("dxgi.dll")])


if __name__ == "__main__":
    unittest.main()
