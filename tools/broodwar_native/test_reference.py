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
    assert np.asarray(result).tolist() == [[[0,0,255,128],[0,255,0,255]]]


def test_native_chain_draws_shadow_behind_sprite_and_repeats_deterministically(tmp_path: Path) -> None:
    from remastered_reference import Region
    folder = tmp_path/'world-sprites'
    folder.mkdir()
    terrain = tmp_path/'scene-assets'
    terrain.mkdir()
    Image.new('RGBA',(32,32),'white').save(terrain/'tile.png')
    Image.new('RGBA',(8,8),(255,0,0,255)).save(folder/'unit.png')
    Image.new('RGBA',(8,8),(0,0,0,255)).save(folder/'shadow.png')
    entries = {}
    for kind,filename in ((1,'unit.png'),(2,'shadow.png')):
        entries[str(kind)] = {'file':filename,'scale':1,'grpWidth':8,'grpHeight':8,'sourceSha256':'fixture',
                              'frames':[{'x':0,'y':0,'ox':0,'oy':0,'w':8,'h':8}]}
    (folder/'catalog.json').write_text(json.dumps({'images':entries,'creep':{'badlands':{}}}))
    (terrain/'manifest.json').write_text(json.dumps({'map':{'nativePath':'fixture','width':1,'height':1,'tiles':[0],'tileset':'badlands'},
                                                    'tiles':{'0':{'file':'tile.png'}}}))
    chain = [{'id':kind,'frame':0,'flags':0,'drawfunc':draw,'offsetX':0,'offsetY':0} for kind,draw in ((1,0),(2,10))]
    snapshot = tmp_path/'scene.json'
    snapshot.write_text(json.dumps({'frame':1,'map':{'path':'fixture'},'playerColors':[[255,255,255]],
                                    'units':[{'id':'unit','owner':0,'x':16,'y':16,'visual':{'images':chain}}]}))
    renderer = ReferenceRenderer(tmp_path)
    output = renderer.render(snapshot,Region(0,0,32,32,1))
    first = output.read_bytes()
    with Image.open(output) as result:
        assert result.getpixel((16,16)) == (255,0,0,255)
        assert result.getpixel((0,0)) == (255,255,255,255)
    assert renderer.render(snapshot,Region(0,0,32,32,1)).read_bytes() == first
