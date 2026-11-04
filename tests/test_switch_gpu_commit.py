"""CPU-only GPU submission commits writes before publishing completion."""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / 'tools' / 'switch'))
from gpu_memory import GpuChannel
from gpu_executor import GpuExecutionState
from gpu_commit import commit_cpu_submission


class GpuCommitTests(unittest.TestCase):
    def test_commits_once_and_publishes_fence_after_writes(self):
        memory = bytearray(b'abcdefgh')
        channel = GpuChannel(1)
        channel.syncpoint = 5
        staged = GpuExecutionState()
        staged.pending_actions = [('write', 0, b'XY'), ('flush_pending_writes', 0),
                                  ('increment_syncpoint', 5, 1, False), ('flush_tiled_cache', 0)]
        staged.pending_writes = [(0, b'XY')]
        def write(address, data):
            self.assertEqual(channel.completed, 0)
            memory[address:address + len(data)] = data
        fence = commit_cpu_submission(channel, staged, 0x104, 0, 1,
                                      lambda a, n: memory[a:a+n], write,
                                      lambda a, n: 0 <= a and a+n <= len(memory))
        self.assertEqual(memory, bytearray(b'XYcdefgh'))
        self.assertEqual(fence, (5, 1))
        self.assertEqual((channel.submitted, channel.completed), (1, 1))
        self.assertEqual(staged.pending_actions, [])
        self.assertEqual(staged.pending_writes, [])

    def test_failed_fence_rollback_faults_channel_after_trying_memory_restore(self):
        channel = GpuChannel(1)
        channel.syncpoint = 5
        staged = GpuExecutionState()
        staged.pending_actions = [('write', 0, b'XY'), ('increment_syncpoint', 5, 1, False)]
        memory = bytearray(b'abcd')
        def write(address, data):
            memory[address:address+len(data)] = data
        def fail_fence(*args):
            raise ValueError('fence failed')
        def fail_restore():
            raise ValueError('fence restore failed')
        with self.assertRaisesRegex(RuntimeError, 'channel faulted'):
            commit_cpu_submission(channel, staged, 0x104, 0, 1, lambda a,n: memory[a:a+n],
                                  write, lambda a,n: True, fail_fence, fail_restore)
        self.assertEqual(memory, bytearray(b'abcd'))
        self.assertTrue(channel.faulted)
        self.assertEqual(channel.completed, 0)
        with self.assertRaises(ValueError):
            commit_cpu_submission(channel, staged, 0x104, 0, 1, lambda a,n: memory[a:a+n],
                                  write, lambda a,n: True)

    def test_copy_reads_earlier_staged_writes_and_rejects_alias_overlap(self):
        channel = GpuChannel(1)
        channel.syncpoint = 5
        staged = GpuExecutionState()
        staged.pending_actions = [('write', 0, b'XY'), ('pitch_to_tiled', ((0, 8, 2),)),
                                  ('increment_syncpoint', 5, 1, False)]
        memory = bytearray(b'abcdefghijklmnop')
        def write(address, data):
            memory[address:address+len(data)] = data
        valid = lambda a,n: 0 <= a and a+n <= len(memory)
        commit_cpu_submission(channel, staged, 0x104, 0, 1, lambda a,n: memory[a:a+n],
                              write, valid, check_gpu_tiled_write=valid,
                              check_gpu_read=valid, resolve_backing=lambda a,n: a)
        self.assertEqual(memory[:2], b'XY')
        self.assertEqual(memory[8:10], b'XY')
        self.assertEqual(channel.completed, 1)
        overlapping = GpuExecutionState()
        overlapping.pending_actions = [('pitch_to_tiled', ((0, 1, 2),)), ('increment_syncpoint', 5, 1, False)]
        before = bytes(memory)
        with self.assertRaisesRegex(ValueError, 'overlapping'):
            commit_cpu_submission(channel, overlapping, 0x104, 0, 1, lambda a,n: memory[a:a+n],
                                  write, valid, check_gpu_tiled_write=valid,
                                  check_gpu_read=valid, resolve_backing=lambda a,n: a)
        self.assertEqual(bytes(memory), before)
        self.assertEqual(channel.completed, 1)

    def test_tiled_write_requires_explicit_kind_validation(self):
        channel = GpuChannel(1)
        channel.syncpoint = 5
        staged = GpuExecutionState()
        staged.pending_actions = [('tiled_write', 0, b'XY'), ('increment_syncpoint', 5, 1, False)]
        memory = bytearray(b'abcd')
        def write(address, data):
            memory[address:address+len(data)] = data
        with self.assertRaises(ValueError):
            commit_cpu_submission(channel, staged, 0x104, 0, 1, lambda a,n: memory[a:a+n],
                                  write, lambda a,n: True)
        self.assertEqual(memory, bytearray(b'abcd'))
        fence = commit_cpu_submission(channel, staged, 0x104, 0, 1, lambda a,n: memory[a:a+n],
                                      write, lambda a,n: False, check_gpu_tiled_write=lambda a,n: True)
        self.assertEqual(fence, (5, 1))
        self.assertEqual(memory, bytearray(b'XYcd'))

    def test_preflight_and_write_failure_do_not_publish_or_leave_partial_data(self):
        channel = GpuChannel(1)
        channel.syncpoint = 5
        staged = GpuExecutionState()
        staged.pending_actions = [('write', 0, b'XY'), ('write', 4, b'ZZ'),
                                  ('increment_syncpoint', 5, 1, False)]
        memory = bytearray(b'abcdefgh')
        failed = []
        def write(address, data):
            if address == 4 and not failed:
                failed.append(True)
                memory[address:address+1] = data[:1]
                raise ValueError('injected failure after partial write')
            memory[address:address+len(data)] = data
        with self.assertRaises(ValueError):
            commit_cpu_submission(channel, staged, 0x104, 0, 1, lambda a,n: memory[a:a+n],
                                  write, lambda a,n: a+n <= len(memory))
        self.assertEqual(memory, bytearray(b'abcdefgh'))
        self.assertEqual((channel.submitted, channel.completed), (0, 0))
        with self.assertRaises(ValueError):
            commit_cpu_submission(channel, staged, 0x104, 0, 2, lambda a,n: memory[a:a+n],
                                  write, lambda a,n: True)
        self.assertEqual(memory, bytearray(b'abcdefgh'))


if __name__ == '__main__':
    unittest.main()
