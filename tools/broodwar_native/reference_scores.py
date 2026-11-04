# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "pillow", "scikit-image"]
# ///
# Run: runtime/asset-venv/bin/python tools/broodwar_native/reference_scores.py CAPTURE.json
"""Camera-registered regional metrics and held-out affine brightness calibration."""
import json
import math
from pathlib import Path
import sys

import numpy as np
from PIL import Image
from skimage.metrics import structural_similarity

from remastered_reference import ReferenceRenderer, Region


def compare(capture: Path) -> Path:
    metadata = json.loads(capture.read_text())
    with Image.open(capture.with_suffix('.png')) as source:
        rendered = np.asarray(source.convert('RGB'))/255.
    height,width = rendered.shape[:2]
    elevation = metadata['elevationDegrees']
    camera = metadata['camera']
    view_height = metadata['viewHeight']*2*32
    view_width = view_height*width/height
    sine = math.sin(math.radians(elevation))
    center_y = (camera[2]-camera[1]/math.tan(math.radians(elevation)))*32
    region = Region(camera[0]*32-view_width/2,center_y-view_height/sine/2,
                    view_width,view_height/sine,height/view_height,elevation)
    reference_path = ReferenceRenderer().render(capture,region)
    with Image.open(reference_path) as source:
        reference = np.asarray(source.convert('RGB'))/255.
    if reference.shape != rendered.shape:
        raise RuntimeError('Camera-derived output shape differs; registration/resizing is forbidden')
    weights = np.array([.2126,.7152,.0722])
    observed = rendered@weights
    expected = reference@weights
    yy,xx = np.indices(observed.shape)
    training = ((xx//16+yy//16)%2)==0
    design = np.column_stack((observed[training],np.ones(training.sum())))
    gain,offset = np.linalg.lstsq(design,expected[training],rcond=None)[0]
    predicted = observed*gain+offset
    residual = expected[~training]-predicted[~training]
    total = np.sum((expected[~training]-expected[~training].mean())**2)
    r_squared = float(1-np.sum(residual**2)/total) if total>0 else None
    rows = []
    for row in range(2):
        for column in range(3):
            bounds = (column*width//3,row*height//2,(column+1)*width//3,(row+1)*height//2)
            x0,y0,x1,y1 = bounds
            a,b = reference[y0:y1,x0:x1],rendered[y0:y1,x0:x1]
            rows.append({'region':f'{row}-{column}','bounds':bounds,
                         'SSIM':float(structural_similarity(a,b,channel_axis=2,data_range=1.)),
                         'referenceBrightness':float((a@weights).mean()),'viewerBrightness':float((b@weights).mean()),
                         'referenceContrast':float((a@weights).std()),'viewerContrast':float((b@weights).std())})
    report = {'frame':metadata['frame'],'capture':str(capture.with_suffix('.png')),'reference':str(reference_path),
              'registration':'Recorded camera projection and native snapshot; no image alignment or crop optimization.',
              'regions':rows,'fit':{'gain':float(gain),'offset':float(offset),'heldOutRSquared':r_squared,
                                   'passed':r_squared is not None and r_squared>.8,'applied':False,
                                   'split':'Alternating fixed 16-pixel blocks; fit half, evaluate the other half.'},
              'sunDirectionFit':None,'shadowSoftnessFit':None,'clientParityAccepted':False,
              'limitations':['Reference report lists client-compositor gaps; these scores are diagnostic, not Remastered acceptance.',
                             'Prelit billboard/terrain color does not identify a 3D sun direction or shadow softness.',
                             'Affine fit is reported without modifying scene colors; geometry and compositing residuals must not be hidden by exposure.']}
    path = capture.with_name(capture.stem+'-scores.json')
    path.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))
    return path


if __name__ == '__main__':
    compare(Path(sys.argv[1]))
