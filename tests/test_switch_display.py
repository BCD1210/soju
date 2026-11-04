"""Buffer ownership and framebuffer-layout tests for the experimental core."""
import importlib.util
import pathlib
import struct
import sys
import unittest
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools' / 'switch'))
from buffer_queue import BufferQueue

class BufferQueueTests(unittest.TestCase):
    def test_two_slots_ownership_and_alternation(self):
        q = BufferQueue()
        q.register(0, 'first')
        q.register(1, 'second')
        self.assertEqual(q.dequeue(), 0)
        with self.assertRaises(ValueError):
            q.register(0, 'replacement')
        self.assertEqual(q.dequeue(), 1)
        with self.assertRaises(ValueError):
            q.dequeue()
        self.assertEqual(q.queue(1), 'second')
        with self.assertRaises(ValueError):
            q.queue(1)
        self.assertEqual(q.queue(0), 'first')
        self.assertEqual(q.dequeue(), 0)
        self.assertEqual(q.queue(0), 'first')
        self.assertEqual(q.dequeue(), 1)

    def test_invalid_slot(self):
        q = BufferQueue()
        with self.assertRaises(ValueError):
            q.register(64, 'bad')

@unittest.skipUnless(importlib.util.find_spec('unicorn'), 'optional unicorn package not installed')
class FramebufferTests(unittest.TestCase):
    def test_binder_queued_slot_and_truncated_parcel(self):
        import trace_nro
        name = 'android.gui.IGraphicBufferProducer'
        token = struct.pack('<II', 0x100, len(name)) + name.encode('utf-16le') + bytes(2)
        token += bytes((-len(token)) % 4)
        payload = token + struct.pack('<I', 1)
        parcel = struct.pack('<IIII', len(payload), 16, 0, 16 + len(payload)) + payload
        request = bytearray(32)
        struct.pack_into('<I', request, 0, 1 << 16)
        struct.pack_into('<II', request, 8, (len(parcel) << 16) | 0x1000, 0x1234)
        cpu = mock.Mock()
        cpu.mem_read.return_value = parcel
        self.assertEqual(trace_nro.binder_queue_slot(cpu, request), 1)
        cpu.mem_read.assert_called_once_with(0x100001234, len(parcel))
        cpu.mem_read.return_value = parcel[:-4]
        with self.assertRaises(ValueError):
            trace_nro.binder_queue_slot(cpu, request)

    def test_two_rows_of_16_pixel_rgba(self):
        import trace_nro
        raw = bytes((i % 251 for i in range(512)))
        cpu = mock.Mock()
        cpu.mem_read.return_value = raw
        pixels = trace_nro.frame_pixels(cpu, (1, 16, 2, 64, 0, 0), (512, 0x1000))
        expected = b''.join(raw[a:a+16] for a in (0, 32, 256, 288, 16, 48, 272, 304))
        self.assertEqual(pixels, expected)
        cpu.mem_read.assert_called_once_with(0x1000, 512)

    def test_native_converter_matches_python(self):
        import trace_nro
        if trace_nro._native_convert is None:
            self.skipTest('optional native converter is not installed')
        for width, height, pitch, block in ((16, 2, 64, 0), (31, 9, 128, 1), (64, 33, 256, 2)):
            block_rows = 8 * (1 << block)
            length = pitch * ((height + block_rows - 1) // block_rows) * block_rows
            cpu = mock.Mock()
            cpu.mem_read.return_value = bytes(i % 251 for i in range(length))
            for fmt in (1, 5):
                args = (cpu, (1, width, height, pitch, 0, block, fmt), (length, 0x1000))
                accelerated = trace_nro.frame_pixels(*args)
                with mock.patch.object(trace_nro, '_native_convert', None):
                    reference = trace_nro.frame_pixels(*args)
                self.assertEqual(accelerated, reference)

    def test_rejects_out_of_allocation(self):
        import trace_nro
        with self.assertRaises(ValueError):
            trace_nro.frame_pixels(mock.Mock(), (1, 16, 2, 64, 1, 0), (512, 0x1000))

if __name__ == '__main__':
    unittest.main()
