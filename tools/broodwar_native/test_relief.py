"""Data-layer regressions using synthetic surfaces, without owned game pixels."""
import numpy as np
from prepare_relief import depth_from_normals, pad_depth
from remastered_layers import normals


def test_normal_alpha_is_signed_y_not_opacity():
    pixels = np.array([[[128,0,0,255],[255,0,0,128]]], dtype=np.uint8)
    n = normals(pixels)
    assert n[0,0,1] == 1
    assert n[0,1,0] == 1
    assert np.isfinite(n).all()


def test_depth_extension_preserves_surface_and_ignores_transparent_depth():
    depth = np.array([[255,255,255],[0,30,0],[255,255,255]], dtype=np.uint8)
    mask = np.zeros((3,3), bool)
    mask[1,1] = True
    assert np.array_equal(pad_depth(depth,mask), np.full((3,3),30))
    assert not pad_depth(depth,np.zeros_like(mask)).any()


def test_normal_integration_has_finite_nonconstant_depth():
    normal = np.full((16,16,4),128,dtype=np.uint8)
    normal[...,0] = np.arange(16)[None,:]*8+64
    result = depth_from_normals(normal,np.ones((16,16),bool))
    assert result.dtype == np.uint8
    assert np.ptp(result)>20
    assert not depth_from_normals(normal,np.zeros((16,16),bool)).any()
