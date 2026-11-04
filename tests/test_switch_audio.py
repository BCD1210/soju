"""Playback queue timing and pause/resume behavior."""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / 'tools' / 'switch'))
from audio_queue import AudioQueue

class AudioQueueTests(unittest.TestCase):
    def test_queue_before_start_and_pause_resume(self):
        q = AudioQueue()
        q.append(10, 100, 0)
        q.append(11, 50, 0)
        self.assertEqual(q.pump(10000), 0)
        q.set_running(True, 1000)
        self.assertEqual(q.pump(1099), 0)
        q.set_running(False, 1050)
        self.assertEqual(q.pump(100000), 0)
        q.set_running(True, 2000)
        self.assertEqual(q.pump(2049), 0)
        self.assertEqual(q.pump(2050), 1)
        self.assertEqual(q.pop_released(1), [10])
        self.assertTrue(q.contains(11))
        self.assertEqual(q.pump(2100), 1)
        self.assertEqual(q.pop_released(10), [11])

    def test_queue_bounds(self):
        q = AudioQueue()
        for i in range(64):
            q.append(i, 1, 0)
        with self.assertRaises(ValueError):
            q.append(65, 1, 0)

if __name__ == '__main__':
    unittest.main()
