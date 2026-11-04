"""GPU execution staging must not commit partial or unknown commands."""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / 'tools' / 'switch'))
from gpu_executor import GpuExecutionState, UnsupportedGpuMethod
from test_switch_gpu_mme_interpreter import PROGRAM


class GpuExecutorTests(unittest.TestCase):
    def test_supported_bind_and_upload_are_staged(self):
        state = GpuExecutionState()
        decoded = [(0x1000, False, False, [(0, 0, 0xb197), (0, 0x114, 2),
                                           (0, 0x118, 0x1234), (0, 0x11c, 0), (0, 0x120, 2)])]
        staged = state.stage(decoded)
        self.assertEqual(staged.mme.instructions, {2: 0x1234})
        self.assertEqual(staged.mme.entries, {0: 2})
        self.assertEqual(state.bindings, {})
        self.assertEqual(state.mme.instructions, {})

    def test_2d_configuration_is_checked_and_staged(self):
        state = GpuExecutionState()
        methods = [(3, 0, 0x902d), (3, 0x260, 0), (3, 0x2ac, 3),
                   (3, 0x290, 0), (3, 0x29c, 0), (3, 0x884, 63)]
        staged = state.stage([(0x1000, False, False, methods)])
        self.assertEqual(staged.registers[(3, 0x884)], 63)
        self.assertEqual(state.registers, {})
        with self.assertRaises(UnsupportedGpuMethod):
            state.stage([(0x1000, False, False, methods + [(3, 0x290, 2)])])
        self.assertEqual(state.registers, {})

    def test_constant_buffer_upload_is_staged_and_checked(self):
        state = GpuExecutionState()
        methods = [(0, 0, 0xb197), (0, 0x2380, 0x100), (0, 0x2384, 2),
                   (0, 0x2388, 0x380000), (0, 0x238c, 0), (0, 0x2390, 0x12345678),
                   (0, 0x2390, 0)]
        check = lambda address, size: 0x200380000 <= address and address + size <= 0x200380100
        staged = state.stage([(0x1000, False, False, methods)], check)
        self.assertEqual(staged.pending_writes, [(0x200380000, bytes.fromhex('78563412')),
                                                (0x200380004, bytes(4))])
        self.assertEqual(staged.registers[(0, 0x238c)], 8)
        self.assertEqual(state.pending_writes, [])
        with self.assertRaises(UnsupportedGpuMethod):
            state.stage([(0x1000, False, False, methods)], lambda address, size: False)
        with self.assertRaises(UnsupportedGpuMethod):
            state.stage([(0x1000, False, False, methods + [(0, 0x2c4, 0)])], check)
        self.assertEqual(state.pending_writes, [])

    def test_vertex_macro_collects_parameters_across_entries(self):
        state = GpuExecutionState().stage([(0x1000, False, False, [(0, 0, 0xb197)])])
        state.mme.entries[1] = 16
        words = [0x00000201, 0x00000301, 0x00000401, 0x00000501,
                 0x11004612, 0x09004712, 0x05c07621, 0x00002041,
                 0x00002841, 0x05f03f21, 0x000010c1, 0x00001841]
        state.mme.instructions.update({16 + i: word for i, word in enumerate(words)})
        decoded = [(0x1000, False, False, [(0, 0x3808, 0), (0, 0x380c, 2)]),
                   (0x2000, False, False, [(0, 0x380c, 0x0583001f), (0, 0x380c, 2), (0, 0x380c, 0x05830000)])]
        staged = state.stage(decoded)
        self.assertEqual(staged.registers[(0, 0x1c04)], 2)
        self.assertEqual(staged.registers[(0, 0x1c08)], 0x05830000)
        self.assertEqual(staged.registers[(0, 0x1f04)], 0x0583001f)
        with self.assertRaises(UnsupportedGpuMethod):
            state.stage(decoded[:1])
        self.assertNotIn((0, 0x1c04), state.registers)

    def test_blend_macro_stages_all_eight_targets(self):
        state = GpuExecutionState().stage([(0x1000, False, False, [(0, 0, 0xb197)])])
        state.mme.entries[2] = 7
        words = [0x05360021, 0x00404042, 0x00424042, 0x00444042, 0x00464042,
                 0x00484042, 0x004a4042, 0x004c40c2, 0x004e4042]
        state.mme.instructions.update({7 + i: word for i, word in enumerate(words)})
        staged = state.stage([(0x1000, False, False, [(0, 0x3810, 0x81)])])
        self.assertEqual([staged.registers[(0, 0x1360 + 4*i)] for i in range(8)], [1, 0, 0, 0, 0, 0, 0, 1])
        self.assertNotIn((0, 0x1360), state.registers)

    def test_two_real_macros_stage_in_sequence_and_roll_back(self):
        state = GpuExecutionState().stage([(0x1000, False, False, [(0, 0, 0xb197)])])
        state.mme.entries.update({5: 45, 6: 28})
        state.mme.instructions.update({45 + i: word for i, word in enumerate(PROGRAM)})
        second = list(PROGRAM)
        second[3], second[8] = 0x02100415, 0x020c0021
        state.mme.instructions.update({28 + i: word for i, word in enumerate(second)})
        commands = [(0, 0x3828, 0x40), (0, 0x3830, 0x30)]
        staged = state.stage([(0x1000, False, False, commands)])
        self.assertEqual(staged.registers[(0, 0x2100)], 0x40)
        self.assertEqual(staged.registers[(0, 0x20c0)], 0x30)
        self.assertEqual(staged.registers[(0, 0x2ec)], 0x60)
        self.assertEqual(state.registers[(0, 0x2100)], 0)
        with self.assertRaises(UnsupportedGpuMethod):
            state.stage([(0x1000, False, False, commands + [(0, 0x2c4, 0)])])
        self.assertEqual(state.registers[(0, 0x2100)], 0)

    def test_compute_counters_and_idle_barrier_are_transactional(self):
        state = GpuExecutionState()
        methods = [(1, 0, 0xb1c0), (1, 0x248, 0x3803f), (1, 0x248, 0x38000), (1, 0x110, 0)]
        staged = state.stage([(0x1000, False, False, methods)])
        self.assertEqual(staged.compute_ref_counters, {(1, 63): 0x380, (1, 0): 0x380})
        self.assertEqual(staged.pending_actions, [('wait_idle', 1)])
        self.assertEqual(state.compute_ref_counters, {})
        state.inflight_work = True
        with self.assertRaises(UnsupportedGpuMethod):
            state.stage([(0x1000, False, False, methods)])
        self.assertEqual(state.compute_ref_counters, {})

    def test_inline_upload_validates_and_stages_little_endian(self):
        state = GpuExecutionState()
        setup = [(1, 0, 0xb1c0), (1, 0x188, 2), (1, 0x18c, 0x3700c0),
                 (1, 0x180, 8), (1, 0x184, 1), (1, 0x1b0, 0x41)]
        check = lambda address, size: address == 0x2003700c0 and size == 8
        staged = state.stage([(0x1000, False, False, setup + [(1, 0x1b4, 0x12345678), (1, 0x1b4, 2), (1, 0x1698, 0x1000)])], check)
        self.assertEqual(staged.pending_writes, [(0x2003700c0, bytes.fromhex('78563412')),
                                                (0x2003700c4, bytes.fromhex('02000000'))])
        self.assertEqual(staged.pending_actions[-1], ('invalidate_constant_cache', 1))
        for commands in (setup + [(1, 0x1b4, 1)], setup + [(1, 0x1b4, 1)] * 3,
                         [(1, 0, 0xb1c0), (1, 0x1b4, 1)]):
            with self.assertRaises(UnsupportedGpuMethod):
                state.stage([(0x1000, False, False, commands)], check)
        self.assertEqual(state.pending_writes, [])

    def test_bindings_snapshot_and_syncpoint_are_staged_only(self):
        state = GpuExecutionState()
        commands = [(0, 0, 0xb197), (0, 0x2380, 0x10000), (0, 0x2384, 2),
                    (0, 0x2388, 0x320000), (0, 0x2410, 0xf1),
                    (0, 0x2388, 0x330000), (0, 0x1144, 0), (0, 0x2c8, 0x100005)]
        staged = state.stage([(0x1000, False, False, commands)])
        self.assertEqual(staged.constant_bindings[(0, 0, 15)], (0x200320000, 0x10000))
        self.assertEqual(staged.pending_actions[-2:], [('flush_pending_writes', 0),
                                                       ('increment_syncpoint', 5, 1, False)])
        self.assertEqual(state.pending_actions, [])
        self.assertEqual(state.constant_bindings, {})

    def test_a140_upload_preserves_required_memory_barrier(self):
        state = GpuExecutionState()
        commands = [(2, 0, 0xa140), (2, 0x188, 2), (2, 0x18c, 0x2bfc00),
                    (2, 0x180, 4), (2, 0x184, 1), (2, 0x1b0, 0x1001), (2, 0x1b4, 0x1234)]
        staged = state.stage([(0x1000, False, False, commands)],
                             lambda address, size: address == 0x2002bfc00 and size == 4)
        self.assertEqual(staged.pending_actions, [('write', 0x2002bfc00, bytes.fromhex('34120000')),
                                                  ('memory_barrier', 2)])
        self.assertEqual(state.pending_actions, [])

    def test_tiled_color_clear_is_staged_and_never_touches_depth(self):
        state = GpuExecutionState()
        regs = {0x800: 2, 0x804: 0x44b0000, 0x808: 1280, 0x80c: 720, 0x810: 0xd5,
                0x814: 0x40, 0x818: 1, 0x820: 0, 0xff4: 0x05000000, 0xff8: 0x02d00000,
                0x10f8: 0, 0x121c: 1, 0x15d0: 0, 0x1558: 1, 0x19e0: 0, 0xd80: 0, 0xd84: 0, 0xd88: 0,
                0xd8c: 0x3f800000}
        commands = [(0, 0, 0xb197)] + [(0, reg, val) for reg, val in regs.items()] + [(0, 0x19d0, 0x3c)]
        check = lambda address, size: 0x2044b0000 <= address and address + size <= 0x2044b0000 + 0x3c0000
        staged = state.stage([(0x1000, False, False, commands)], lambda a,n: True, check)
        actions = [a for a in staged.pending_actions if a[0] == 'tiled_write']
        self.assertEqual(len(actions), 81)
        self.assertEqual(actions[0][1], 0x2044b0000)
        self.assertEqual(actions[0][2][:4], bytes.fromhex('000000ff'))
        self.assertEqual(state.pending_actions, [])
        with self.assertRaises(UnsupportedGpuMethod):
            state.stage([(0x1000, False, False, commands)], lambda a,n: True, lambda a,n: False)
        self.assertEqual(state.pending_actions, [])

    def test_pitch_to_tiled_copy_is_planned_without_reading_or_writing(self):
        state = GpuExecutionState()
        regs = {0x708: 0x03303210, 0x70c: 0x1040, 0x710: 301, 0x714: 76, 0x718: 1,
                0x71c: 0, 0x720: 0, 0x400: 2, 0x404: 0x5410000, 0x408: 2,
                0x40c: 0x53e0000, 0x410: 1204, 0x414: 1216, 0x418: 301, 0x41c: 76}
        commands = [(4, 0, 0xb0b5)] + [(4, reg, val) for reg, val in regs.items()] + [(4, 0x300, 0x686)]
        staged = state.stage([(0x1000, False, False, commands)], lambda a,n: True,
                             lambda a,n: True, lambda a,n: True)
        action, copies = staged.pending_actions[-1]
        self.assertEqual(action, 'pitch_to_tiled')
        self.assertEqual(sum(size for _, _, size in copies), 301 * 76 * 4)
        self.assertEqual(copies[0], (0x205410000, 0x2053e0000, 16))
        self.assertEqual(state.pending_actions, [])
        with self.assertRaises(UnsupportedGpuMethod):
            state.stage([(0x1000, False, False, commands)], lambda a,n: True,
                        lambda a,n: True, lambda a,n: False)
        self.assertEqual(state.pending_actions, [])

    def test_unsupported_effect_does_not_change_original_state(self):
        state = GpuExecutionState()
        decoded = [(0x1000, False, False, [(0, 0, 0xb197), (0, 0x118, 9), (0, 0x2c4, 0)])]
        with self.assertRaises(UnsupportedGpuMethod):
            state.stage(decoded)
        self.assertEqual(state.mme.instructions, {})
        self.assertEqual(state.bindings, {})


if __name__ == '__main__':
    unittest.main()
