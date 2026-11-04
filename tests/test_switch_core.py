"""Executable, synthetic NRO tests; no Nintendo files or tools required."""
import pathlib
import struct
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCE = ROOT / "tools" / "switch" / "soju-switch.c"

def nro(instructions, *, text_size=0x100):
    data = bytearray(text_size)
    struct.pack_into("<I", data, 0, 0x14000020)  # b 0x80
    data[0x10:0x14] = b"NRO0"
    struct.pack_into("<III", data, 0x14, 0, text_size, 0)
    struct.pack_into("<IIIIIII", data, 0x20, 0, text_size, text_size, 0, text_size, 0, 0)
    for i, ins in enumerate(instructions):
        struct.pack_into("<I", data, 0x80 + 4*i, ins)
    return data

class SwitchCoreTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.binary = pathlib.Path(cls.temp.name) / "soju-switch"
        subprocess.run(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror",
                        str(SOURCE), "-o", str(cls.binary)], check=True)

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def run_image(self, image):
        path = pathlib.Path(self.temp.name) / "fixture.nro"
        path.write_bytes(image)
        return subprocess.run([str(self.binary), str(path)], capture_output=True, text=True)

    def test_executes_branch_movz_nop_and_exit(self):
        result = self.run_image(nro([0xd2800540, 0xd503201f, 0xd40000e1]))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("ExitProcess (x0=0x2a, steps=4)", result.stdout)

    def test_compare_and_conditional_branch(self):
        # mov x0, #42; cmp x0, #42; b.eq skips an invalid opcode.
        result = self.run_image(nro([0xd2800540, 0xf100a81f, 0x54000040, 0, 0xd40000e1]))
        self.assertEqual(result.returncode, 0, result.stderr)
        # An untaken equality branch must not skip the invalid opcode.
        result = self.run_image(nro([0xd2800540, 0xf100041f, 0x54000040, 0, 0xd40000e1]))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unsupported instruction", result.stderr)

    def test_rejects_invalid_header_and_segments(self):
        self.assertIn("header", self.run_image(b"NRO0").stderr)
        image = nro([0xd40000e1])
        struct.pack_into("<I", image, 0x24, 0x104)
        self.assertIn("segment outside", self.run_image(image).stderr)

    def test_unknown_instruction_and_svc_fail(self):
        self.assertIn("unsupported instruction", self.run_image(nro([0])).stderr)
        self.assertIn("unsupported SVC 0x8", self.run_image(nro([0xd4000101])).stderr)

    def test_branch_cannot_leave_text(self):
        self.assertIn("branch outside", self.run_image(nro([0x14000400])).stderr)

    def test_instruction_limit(self):
        self.assertIn("instruction limit", self.run_image(nro([0x14000000])).stderr)

if __name__ == "__main__":
    unittest.main()
