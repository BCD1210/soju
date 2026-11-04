"""MME upload storage: bounded state only, no macro execution."""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / 'tools' / 'switch'))
from gpu_mme import MacroUpload, INSTRUCTION_WORD_LIMIT, ENTRY_LIMIT


class MacroUploadTests(unittest.TestCase):
    def test_instruction_and_entry_uploads_preserve_words_and_increment(self):
        mme = MacroUpload()
        mme.write(0x114, 7)
        mme.write(0x118, 0x12345678)
        mme.write(0x118, 0xffffffff)
        mme.write(0x11c, 3)
        mme.write(0x120, 7)
        mme.write(0x120, 99)
        self.assertEqual(mme.instructions, {7: 0x12345678, 8: 0xffffffff})
        self.assertEqual(mme.entries, {3: 7, 4: 99})
        self.assertEqual((mme.instruction_pointer, mme.entry_pointer), (9, 5))

    def test_rejected_writes_do_not_mutate_or_wrap(self):
        mme = MacroUpload()
        mme.write(0x114, INSTRUCTION_WORD_LIMIT - 1)
        mme.write(0x118, 1)
        with self.assertRaises(ValueError):
            mme.write(0x118, 2)
        with self.assertRaises(ValueError):
            mme.write(0x11c, ENTRY_LIMIT)
        with self.assertRaises(ValueError):
            mme.write(0x118, -1)
        self.assertEqual(mme.instructions, {INSTRUCTION_WORD_LIMIT - 1: 1})
        self.assertEqual(mme.entry_pointer, 0)


if __name__ == '__main__':
    unittest.main()
