# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "pillow", "pytest"]
# ///
# Run: runtime/asset-venv/bin/python -m pytest tools/broodwar_native/test_reference.py
"""Protect prelit color, alpha, mirroring and recorded-frame selection."""
import json
from pathlib import Path

import numpy as np
from PIL import Image
import pytest

from remastered_reference import ReferenceRenderer, team_pixels


@pytest.mark.parametrize(('mask','expected'),[(0,(200,100,50,64)),(255,(200,0,0,64))])
def test_team_color_preserves_unmasked_light_and_alpha(mask: int, expected: tuple[int,int,int,int]) -> None:
    diffuse = np.array([[[200,100,50,64]]],dtype=np.uint8)
    weights = np.full_like(diffuse,mask)
    assert tuple(team_pixels(diffuse,weights,(255,0,0))[0,0]) == expected


def test_native_frame_and_mirror_select_exact_crop(tmp_path: Path) -> None:
    folder = tmp_path/'world-sprites'
    folder.mkdir()
    terrain = tmp_path/'scene-assets'
    terrain.mkdir()
    (terrain/'manifest.json').write_text('{}')
    image = Image.new('RGBA',(3,1))
    image.putdata([(255,0,0,255),(0,255,0,255),(0,0,255,128)])
    image.save(folder/'atlas.png')
    (folder/'catalog.json').write_text(json.dumps({'images':{'1':{'file':'atlas.png','frames':[
        {'x':0,'y':0,'w':1,'h':1},{'x':1,'y':0,'w':2,'h':1}]}}}))
    native = {'id':1,'frame':1,'flags':2,'drawfunc':0,'mapX':0,'mapY':0,'offsetX':0,'offsetY':0}
    result = ReferenceRenderer(tmp_path).sprite(native,(255,255,255))
    assert list(result.getdata()) == [(0,0,255,128),(0,255,0,255)]
