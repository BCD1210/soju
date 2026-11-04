"""MME execution checks using real Snakey initialization macro instructions."""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / 'tools' / 'switch'))
from gpu_mme import MacroUpload
from gpu_mme_interpreter import run_macro

# Extracted from the public Snakey homebrew initialization stream. The second
# variant changes the other shader pipeline register and output method.
PROGRAM = [0x00dac215, 0x00db0315, 0x0012d710, 0x020c0415,
           0x00004211, 0x00180611, 0x0014bf10, 0x0000f807,
           0x02100021, 0x00800611, 0x00130f10, 0x0014bf10,
           0x0000f807, 0x00000841, 0x00000611, 0x002ec0a1, 0x00003041]


class MmeInterpreterTests(unittest.TestCase):
    def program(self):
        mme = MacroUpload()
        mme.entries[5] = 45
        mme.instructions.update({45 + i: word for i, word in enumerate(PROGRAM)})
        return mme

    def test_real_vertex_macro_fetches_four_ordered_parameters(self):
        mme = MacroUpload()
        mme.entries[1] = 16
        words = [0x00000201, 0x00000301, 0x00000401, 0x00000501,
                 0x11004612, 0x09004712, 0x05c07621, 0x00002041,
                 0x00002841, 0x05f03f21, 0x000010c1, 0x00001841]
        mme.instructions.update({16 + i: word for i, word in enumerate(words)})
        params = (2, 0x0583001f, 2, 0x05830000)
        self.assertEqual(run_macro(mme, 1, 0, lambda address: 0, params),
                         [(0x1c04, 2), (0x1c08, 0x05830000),
                          (0x1f00, 2), (0x1f04, 0x0583001f)])
        with self.assertRaisesRegex(ValueError, 'underflow'):
            run_macro(mme, 1, 0, lambda address: 0, params[:-1])
        with self.assertRaisesRegex(ValueError, 'unused'):
            run_macro(mme, 1, 0, lambda address: 0, params + (0,))

    def test_real_blend_macro_uses_merge_and_method_increment(self):
        mme = MacroUpload()
        mme.entries[2] = 7
        words = [0x05360021, 0x00404042, 0x00424042, 0x00444042, 0x00464042,
                 0x00484042, 0x004a4042, 0x004c40c2, 0x004e4042]
        mme.instructions.update({7 + i: word for i, word in enumerate(words)})
        for arg in (0xff, 0x81, 0):
            self.assertEqual(run_macro(mme, 2, arg, lambda address: 0),
                             [(0x1360 + 4*i, (arg >> i) & 1) for i in range(8)])

    def test_real_macro_branch_and_exit_delay_slots(self):
        mme = self.program()
        state = {0xdac: 0x1b02, 0xdb0: 0x1b02, 0x20c0: 0}
        self.assertEqual(run_macro(mme, 5, 0x40, state.__getitem__),
                         [(0x2100, 0x40), (0x2ec, 0x60)])
        self.assertEqual(run_macro(mme, 5, 0x41, state.__getitem__),
                         [(0x2100, 0x41), (0x2ec, 0)])

    def test_rejects_nested_not_taken_delay_and_read_after_emit(self):
        mme = MacroUpload()
        mme.entries[0] = 0
        mme.instructions = dict(enumerate([0x8807, 0x8027, 0x11, 0x91, 0x11]))
        with self.assertRaisesRegex(ValueError, 'nested MME branch'):
            run_macro(mme, 0, 1, lambda address: 0)
        mme.instructions = dict(enumerate([0x400021, 0x841, 0x400215, 0x404021, 0x1041, 0x91, 0x11]))
        with self.assertRaisesRegex(ValueError, 'read after emitted'):
            run_macro(mme, 0, 7, lambda address: 0)

    def test_zero_register_exit_delay_and_instruction_budget(self):
        mme = MacroUpload()
        mme.entries[0] = 0
        mme.instructions = dict(enumerate([0x4011, 0x100021, 0x41, 0x91, 0x11]))
        self.assertEqual(run_macro(mme, 0, 0, lambda address: 0), [(0x100, 0)])
        mme.instructions = {0: 0x91}
        with self.assertRaisesRegex(ValueError, 'PC not initialized'):
            run_macro(mme, 0, 0, lambda address: 0)
        mme.instructions = {0: 7, 1: 0x11}
        with self.assertRaisesRegex(ValueError, 'budget'):
            run_macro(mme, 0, 0, lambda address: 0)

    def test_branch_not_taken_and_invalid_program_fail(self):
        mme = self.program()
        state = {0xdac: 0x1b01, 0xdb0: 0x1b02, 0x20c0: 0}
        self.assertEqual(run_macro(mme, 5, 0x40, state.__getitem__),
                         [(0x2100, 0x40), (0x2ec, 0x200)])
        with self.assertRaises(ValueError):
            run_macro(mme, 9, 0, state.__getitem__)
        del mme.instructions[45]
        with self.assertRaises(ValueError):
            run_macro(mme, 5, 0, state.__getitem__)


if __name__ == '__main__':
    unittest.main()
