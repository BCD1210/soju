"""Small synthetic checks independent of private artwork."""
import unittest
import numpy as np
from remastered_rig_fit import axes, project, raster, vertex_normals


class RigFitTests(unittest.TestCase):
    def test_pixel_centers_and_front_visibility(self):
        points=np.array([[0.,0.,1.],[4.,0.,1.],[0.,4.,1.],
                         [0.,0.,2.],[4.,0.,2.],[0.,4.,2.]])
        depth,face,bary=raster(points,np.array([[0,1,2],[3,4,5]],np.int32),5,5)
        self.assertEqual(face[0,0],1)
        self.assertEqual(face[4,4],-1)
        self.assertEqual(depth[0,0],2)
        np.testing.assert_allclose(bary[0,0],[.75,.125,.125])

    def test_camera_basis_and_global_target(self):
        matrix=axes(127,33)
        np.testing.assert_allclose(matrix@matrix.T,np.eye(3),atol=1e-12)
        camera={'yaws':[127],'elevation':33,'target':[1,2,3],
                'pixelsPerUnit':10,'width':100,'height':80}
        p=project(np.array([[1,2,3],np.array([1,2,3])+matrix[0]]),camera,0)
        np.testing.assert_allclose(p,[[50,40,0],[60,40,0]],atol=1e-12)

    def test_back_and_degenerate_triangles(self):
        points=np.array([[0.,0.,1.],[4.,0.,1.],[0.,4.,1.]])
        a=raster(points,np.array([[0,1,2]],np.int32),5,5)[1]>=0
        b=raster(points,np.array([[2,1,0],[0,0,0]],np.int32),5,5)[1]>=0
        np.testing.assert_array_equal(a,b)
        np.testing.assert_allclose(vertex_normals(points,np.array([[0,1,2]])),[[0,0,1]]*3)

if __name__=='__main__':unittest.main()
