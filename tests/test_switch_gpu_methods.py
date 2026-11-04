"""Conservative GPU method analysis must never hide known side effects."""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / 'tools' / 'switch'))
from gpu_methods import inspect_effects


class GpuMethodTests(unittest.TestCase):
    def test_class_binding_and_effects_are_class_specific(self):
        decoded = [(0x1000, False, False, [
            (2, 0, 0xa140), (2, 0x1b0, 1), (2, 0x1b4, 42),
            (3, 0, 0x902d), (3, 0x8dc, 3), (4, 0, 0xb0b5), (4, 0x300, 1),
            (5, 0, 0xb197), (5, 0x8dc, 4)])]
        bindings, effects = inspect_effects(decoded)
        self.assertEqual(bindings, {2: 0xa140, 3: 0x902d, 4: 0xb0b5, 5: 0xb197})
        self.assertEqual([effect[-1] for effect in effects],
                         ['inline_memory_launch', 'inline_memory_data', '2d_blit_launch', 'copy_launch'])

    def test_syncpoint_and_macro_upload_are_not_noops(self):
        decoded = [(1, False, False, [(0, 0, 0xb197), (0, 0x118, 7),
                                     (0, 0x2c8, 0), (0, 0x74, 0)])]
        self.assertEqual([item[-1] for item in inspect_effects(decoded)[1]],
                         ['mme_instruction_upload', '3d_syncpoint_increment', 'fifo_syncpoint_operation'])

    def test_rebinding_changes_classification(self):
        decoded = [(1, False, False, [(2, 0, 0xa140), (2, 0, 0x902d), (2, 0x1b4, 4)])]
        self.assertEqual(inspect_effects(decoded)[1], [])


if __name__ == '__main__':
    unittest.main()
