"""Tests for the optional Unicorn-based diagnostic tracer."""
import importlib.util
import os
import pathlib
import struct
import subprocess
import sys
import tempfile
import types
import unittest
from unittest import mock

from test_switch_core import nro

ROOT = pathlib.Path(__file__).resolve().parents[1]
TRACER = ROOT / "tools" / "switch" / "trace_nro.py"
sys.path.insert(0, str(TRACER.parent))

@unittest.skipUnless(importlib.util.find_spec("unicorn"), "optional unicorn package not installed")
class SwitchTraceTests(unittest.TestCase):
    def run_trace(self, image, *args):
        with tempfile.TemporaryDirectory() as tmp:
            path = pathlib.Path(tmp) / "test.nro"
            path.write_bytes(image)
            return subprocess.run([sys.executable, str(TRACER), str(path), *args],
                                  capture_output=True, text=True)

    def test_stops_on_real_svc_encoding(self):
        result = self.run_trace(nro([0xd4000fe1]))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unimplemented SVC 0x7f at offset 0x80", result.stdout)

    def test_memory_permission_and_invalid_permission(self):
        # x0 = image base + 4096, x1 = 4096, x2 = read+write.
        code = [0xd2c00020, 0xf2820000, 0xd2820001, 0xd2800062, 0xd4000041, 0xd4000fe1]
        result = self.run_trace(nro(code, text_size=0x2000))
        self.assertIn("unimplemented SVC 0x7f", result.stdout)
        # A write-only permission is outside the implemented Horizon subset.
        code[3] = 0xd2800042
        result = self.run_trace(nro(code, text_size=0x2000))
        self.assertIn("unsupported SetMemoryPermission", result.stdout)

    def test_permission_lock_prevents_later_reprotect(self):
        # Set permanent permission lock on the data page, then attempt a reprotect.
        code = [0xd2c00020, 0xf2820000, 0xd2820001, 0xd2800202,
                0xd2800203, 0xd4000061, 0xd2c00020, 0xf2820000,
                0xd2800062, 0xd4000041, 0xd4000fe1]
        image = nro(code, text_size=0x2000)
        struct.pack_into('<IIIIII', image, 0x20, 0, 0x1000, 0x1000, 0, 0x1000, 0x1000)
        result = self.run_trace(image)
        self.assertIn('unsupported SetMemoryPermission', result.stdout)

    def test_homebrew_loader_passes_config(self):
        # Guest x0 is a config pointer at entry; a compare with zero is false.
        result = self.run_trace(nro([0xf100001f, 0x54000040, 0xd4000fe1, 0]))
        self.assertIn('unimplemented SVC 0x7f', result.stdout)

    def test_truncated_and_oversized_images_fail(self):
        self.assertIn("header", self.run_trace(b"NRO0").stderr)
        image = nro([0xd4000fe1])
        struct.pack_into("<I", image, 0x38, 0xffffffff)
        self.assertIn("image size", self.run_trace(image).stderr)

    @unittest.skipUnless(os.environ.get('SOJU_SWITCH_TEST_NRO'), 'set SOJU_SWITCH_TEST_NRO to the 2048 v1.1.0 NRO')
    def test_actual_game_renders_and_responds_to_inputs(self):
        with tempfile.TemporaryDirectory() as tmp:
            before, after = pathlib.Path(tmp) / 'before.png', pathlib.Path(tmp) / 'after.png'
            result = subprocess.run([sys.executable, str(TRACER), os.environ['SOJU_SWITCH_TEST_NRO'],
                                     '--max-steps', '100000000', '--moves', 'left,down,right',
                                     '--capture-first', str(before), '--capture', str(after)],
                                    capture_output=True, text=True, timeout=30,
                                    env={**os.environ, 'SOJU_SWITCH_HOME': tmp})
            self.assertIn('after 3 inputs', result.stdout, result.stderr + result.stdout)
            first, last = before.read_bytes(), after.read_bytes()
            self.assertTrue(first.startswith(bytes.fromhex('89504e470d0a1a0a')))
            self.assertTrue(last.startswith(bytes.fromhex('89504e470d0a1a0a')))
            self.assertNotEqual(first, last, 'board did not change after controller inputs')

    @unittest.skipUnless(os.environ.get('SOJU_SWITCH_TEST_NRO2'), 'set SOJU_SWITCH_TEST_NRO2 to the 2048 v1.2.0 NRO')
    def test_second_game_reaches_board(self):
        with tempfile.TemporaryDirectory() as tmp:
            frame = pathlib.Path(tmp) / 'frame.png'
            result = subprocess.run([sys.executable, str(TRACER), os.environ['SOJU_SWITCH_TEST_NRO2'],
                                     '--max-steps', '5000000000', '--capture', str(frame)],
                                    capture_output=True, text=True, timeout=90,
                                    env={**os.environ, 'SOJU_SWITCH_HOME': tmp})
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn('captured frame', result.stdout)
            self.assertTrue(frame.read_bytes().startswith(bytes.fromhex('89504e470d0a1a0a')))

    @unittest.skipUnless(os.environ.get('SOJU_SWITCH_TEST_GPU_NRO'), 'set SOJU_SWITCH_TEST_GPU_NRO to Snakey NX 0.4')
    def test_gpu_homebrew_first_submission_executes_without_falsely_completing_game(self):
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run([sys.executable, str(TRACER), os.environ['SOJU_SWITCH_TEST_GPU_NRO'],
                                     '--max-steps', '200000000'], capture_output=True, text=True, timeout=45,
                                    env={**os.environ, 'SOJU_SWITCH_HOME': tmp, 'SOJU_SWITCH_TRACE_GPU': '1'})
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn('CPU-only submission committed 847 methods, fence 5:1', result.stdout)
            self.assertNotIn('guest exited normally', result.stdout)

    def test_window_can_close_when_guest_never_draws(self):
        spec = importlib.util.spec_from_file_location('soju_trace_test', TRACER)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        fake = mock.Mock()
        fake.poll.return_value = True
        fake_module = types.SimpleNamespace(Window=lambda: fake)
        with tempfile.TemporaryDirectory() as tmp, mock.patch.dict(sys.modules, {'sdl_display': fake_module}):
            path = pathlib.Path(tmp) / 'loop.nro'
            path.write_bytes(nro([0x14000000]))
            self.assertEqual(module.trace(path, 1000, play=True), 0)
        fake.poll.assert_called_once()
        fake.close.assert_called_once()

    def test_stack_mapping_copies_back_and_restores_heap(self):
        def movz(reg, value, shift=0):
            return 0xd2800000 | (shift // 16 << 21) | (value << 5) | reg
        def movk(reg, value, shift=0):
            return 0xf2800000 | (shift // 16 << 21) | (value << 5) | reg
        def svc(number):
            return 0xd4000001 | (number << 5)
        # Allocate a heap, write 42, map one page into the stack region, write
        # 43 through the mapped page, unmap, and verify it in the original heap.
        code = [movz(1, 0x20, 16), svc(1), movz(3, 42), 0xf9000023,
                movz(0, 2, 32), movk(0, 0x40, 16), movz(2, 0x1000),
                svc(4), movz(0, 2, 32), movk(0, 0x40, 16),
                movz(3, 43), 0xf9000003, svc(5), 0xf9400025,
                0xf1000000 | (43 << 10) | (5 << 5) | 31,
                0x54000041, svc(0x7f), svc(0x26)]
        result = self.run_trace(nro(code))
        self.assertIn('unimplemented SVC 0x7f', result.stdout, result.stdout)

    def test_stack_mapping_rejects_null_destination_with_kernel_result(self):
        # Allocate heap, request a stack mapping at zero, compare x0 with 0xdc01.
        code = [0xd2a00401, 0xd4000021, 0xd2800000, 0xd2820002,
                0xd4000081, 0xd2800003 | (0xdc01 << 5),
                0xeb03001f, 0x54000041, 0xd4000fe1, 0xd40004c1]
        result = self.run_trace(nro(code))
        self.assertIn('unimplemented SVC 0x7f', result.stdout, result.stdout)

    def test_started_thread_executes_with_its_argument(self):
        def movz(reg, value, shift=0):
            return 0xd2800000 | (shift // 16 << 21) | (value << 5) | reg
        def movk(reg, value, shift=0):
            return 0xf2800000 | (shift // 16 << 21) | (value << 5) | reg
        def svc(number):
            return 0xd4000001 | (number << 5)
        # Allocate a guest stack, start a thread, and have it verify x0=42.
        code = [movz(1, 0x20, 16), svc(1), movz(0, 2, 32), movk(0, 0x40, 16),
                movz(2, 0x1000), svc(4), 0, movz(2, 42), movz(3, 2, 32),
                movk(3, 0x40, 16), movk(3, 0x1000), movz(4, 44), movz(5, 0),
                svc(8), 0xaa0103e0, svc(9), 0x14000000,
                0xf100001f | (42 << 10), 0x54000041, svc(0x7f), svc(0x26)]
        delta = (17 - 6) * 4
        code[6] = 0x10000001 | ((delta & 3) << 29) | (((delta >> 2) & 0x7ffff) << 5)
        result = self.run_trace(nro(code), '--max-steps', '2000000')
        self.assertIn('unimplemented SVC 0x7f', result.stdout, result.stdout)

    def test_cmif_parser_uses_descriptor_layout_not_stale_magic(self):
        spec = importlib.util.spec_from_file_location('soju_ipc_test', TRACER)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        req = bytearray(256)
        struct.pack_into('<II', req, 0, 4 | (1 << 20) | (1 << 24), 0x80000000)
        struct.pack_into('<I', req, 8, 3)  # PID and one copied handle
        req[0x10:0x14] = b'SFCI'  # stale bytes in special header
        req[0x30:0x34] = b'SFCI'  # aligned actual request after both descriptors
        self.assertEqual(module.cmif_offset(req, False), 0x30)
        req[0x30:0x34] = b'junk'
        self.assertEqual(module.cmif_offset(req, False), -1)

    def test_return_to_homebrew_loader_is_recognized(self):
        result = self.run_trace(nro([0xd2800000, 0xd65f03c0]))
        self.assertIn('guest exited normally', result.stdout)

    def test_guest_break_is_reported_as_guest_failure(self):
        result = self.run_trace(nro([0xd2800000, 0xd2800002, 0xd40004c1]))
        self.assertIn('guest Break reason=0x0', result.stdout)
        self.assertNotIn('unimplemented SVC', result.stdout)

    def test_guest_output_requires_writable_mapped_memory(self):
        from unicorn import Uc, UC_ARCH_ARM64, UC_MODE_ARM
        spec = importlib.util.spec_from_file_location('soju_memory_test', TRACER)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        cpu = Uc(UC_ARCH_ARM64, UC_MODE_ARM)
        cpu.mem_map(0x1000, 0x1000, 3)
        cpu.mem_map(0x2000, 0x1000, 1)
        self.assertTrue(module.writable_guest_range(cpu, 0x1000, 0x1000))
        self.assertFalse(module.writable_guest_range(cpu, 0x1fff, 2))
        self.assertFalse(module.writable_guest_range(cpu, 0x2000, 1))
        self.assertFalse(module.writable_guest_range(cpu, 0x3000, 1))

    def test_instruction_budget(self):
        result = self.run_trace(nro([0x14000000]), "--max-steps", "10")
        self.assertIn("instruction budget", result.stdout)

if __name__ == "__main__":
    unittest.main()
