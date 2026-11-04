"""Checks for reconstruction coordinates and an honest foreground fidelity gate."""
import unittest
import json
import pathlib
import tempfile
from remastered_catalog import write_catalog
import numpy as np
from remastered_reconstruct import basis, project
from remastered_validate import metrics
from remastered_layers import frame, normals


class ReconstructionTests(unittest.TestCase):
    def test_projection_axes(self):
        for d in range(17):
            right,up,toward=basis(d,32)
            np.testing.assert_allclose(np.stack((right,up,toward))@np.stack((right,up,toward)).T,np.eye(3),atol=1e-7)
            x,y,z=project(np.array([right,up,toward]),d,32,200,100)
            np.testing.assert_allclose(x,[101,100,100]);np.testing.assert_allclose(y,[50,49,50]);np.testing.assert_allclose(z,[0,0,1],atol=1e-7)
            self.assertGreater(up[2],0)
            np.testing.assert_allclose(np.cross(right,up),toward,atol=1e-7)

    def test_canvas_offsets_and_crop(self):
        atlas=np.zeros((8,8,4),np.uint8);atlas[2:4,3:5]=255
        info={'width':10,'height':10,'frames':[{'x':3,'y':2,'w':2,'h':2,'ox':6,'oy':7}]}
        result=frame(info,atlas,0)
        self.assertEqual(result[7:9,6:8].sum(),2*2*4*255)
        self.assertEqual(result.sum(),2*2*4*255)

    def test_missing_object_cannot_pass_on_background(self):
        ref=np.zeros((100,100,4));ref[45:55,45:55]=1
        ours=np.zeros_like(ref)
        self.assertFalse(metrics(ref,ours)['passed'])
        self.assertEqual(metrics(ref,ours)['silhouetteIoU'],0)
        self.assertTrue(metrics(ref,ref)['passed'])

    def test_rejected_candidate_never_enters_approved_catalog(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=pathlib.Path(tmp);candidate=root/'0';candidate.mkdir()
            (candidate/'validation.json').write_text(json.dumps({'passed':False,'minimumIoU':.8,'minimumSSIM':.3,'missingAnimations':['death']}))
            catalog=write_catalog(root)
            self.assertEqual(catalog['models'],{})
            self.assertIn('0',catalog['rejected'])
            self.assertFalse((root/'preview').exists())

    def test_changed_model_invalidates_previous_acceptance(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=pathlib.Path(tmp);candidate=root/'0';candidate.mkdir()
            (candidate/'model.glb').write_bytes(b'changed geometry')
            (candidate/'validation.json').write_text(json.dumps({'passed':True,'modelSha256':'old hash','minimumIoU':1,'minimumSSIM':1,'missingAnimations':[]}))
            self.assertEqual(write_catalog(root)['models'],{})

    def test_normal_alpha_is_component(self):
        p=np.array([[[128,0,0,128],[128,0,0,255]]],np.uint8)
        n=normals(p)
        self.assertGreater(n[0,0,2],.99)
        self.assertGreater(n[0,1,1],.99)


if __name__=='__main__':unittest.main()
