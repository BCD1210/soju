"""Map identity and atomic activation regressions without launching a game."""
import hashlib
import json
import pathlib
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import map_assets

class MapAssetsTest(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=pathlib.Path(self.temp.name)
        self.map=self.root/'fixture.scm';self.map.write_bytes(b'fixture')
        self.native='Z:'+str(self.map).replace('/','\\')
        self.state={'map':{'path':self.native}}
    def build(self,args,**kwargs):
        out=pathlib.Path(args[args.index('--out')+1])
        out.joinpath('manifest.json').write_text(json.dumps({'map':{'nativePath':self.native,
            'fileHash':hashlib.sha256(self.map.read_bytes()).hexdigest()}}))
    def test_unsupported_native_paths_rejected(self):
        for path in ('https://example.com/map.scm','relative.scm','Z:/missing/file.scx'):
            with self.assertRaises(ValueError):map_assets.local_map(path,self.root)
    def test_same_size_different_map_has_new_identity(self):
        with patch.object(map_assets,'dependency',return_value=self.map),patch.object(map_assets,'request',side_effect=lambda:self.state),patch.object(map_assets.subprocess,'run',side_effect=self.build) as run:
            self.assertTrue(map_assets.ensure_map(self.root,self.state)['changed'])
            self.assertFalse(map_assets.ensure_map(self.root,self.state)['changed'])
            first=(self.root/'scene-assets').resolve()
            self.map=self.root/'other.scm';self.map.write_bytes(b'fixture')
            self.native='Z:'+str(self.map).replace('/','\\');self.state={'map':{'path':self.native}}
            self.assertTrue(map_assets.ensure_map(self.root,self.state)['changed'])
            self.assertNotEqual(first,(self.root/'scene-assets').resolve())
            self.assertTrue(first.exists());self.assertEqual(run.call_count,2)
    def test_failed_prepare_retains_existing_terrain(self):
        old=self.root/'scene-assets';old.mkdir();(old/'marker').write_text('keep')
        with patch.object(map_assets,'dependency',return_value=self.map),patch.object(map_assets.subprocess,'run',side_effect=subprocess.CalledProcessError(1,'prepare')):
            with self.assertRaises(subprocess.CalledProcessError):map_assets.ensure_map(self.root,self.state)
        self.assertEqual((old/'marker').read_text(),'keep')

if __name__=='__main__':unittest.main()
