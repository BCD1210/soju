"""Runner selection shared by CLI and Library Play, without starting a GUI."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]

class RunnerSelectionTests(unittest.TestCase):
    def test_runner_selection_and_argument_preservation(self):
        with tempfile.TemporaryDirectory(dir=ROOT / 'dist') as temp:
            home = Path(temp)
            (home / 'venv/bin').mkdir(parents=True)
            nro = home / 'own $(touch unwanted).nro'
            nro.write_bytes(b'synthetic')
            for name in ['soju-switch-core', 'venv/bin/python']:
                executable = home / name
                executable.write_text('#!/usr/bin/env python3\nimport json,sys\nprint(json.dumps(sys.argv))\n')
                executable.chmod(0o755)
            def run(mode=None):
                env = {**os.environ, 'SOJU_SWITCH_HOME': str(home)}
                env.pop('SOJU_SWITCH_RUNNER', None)
                if mode is not None:
                    env['SOJU_SWITCH_RUNNER'] = mode
                return subprocess.run(['bash', str(ROOT / 'scripts/switch.sh'), 'play', str(nro)],
                                      env=env, capture_output=True, text=True)
            for mode in [None, 'auto', 'rust']:
                result = run(mode)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(result.stdout), [str(home/'soju-switch-core'), '--play', str(nro)])
            result = run('python')
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)[1:], [str(ROOT/'tools/switch/trace_nro.py'), str(nro), '--play', '--max-steps', '10000000'])
            self.assertEqual(run('invalid').returncode, 64)
            (home/'soju-switch-core').unlink()
            self.assertEqual(json.loads(run().stdout)[0], str(home/'venv/bin/python'))
            self.assertNotEqual(run('rust').returncode, 0)
