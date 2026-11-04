# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "pillow"]
# ///
"""Measure 10–90% alpha transitions in owned native drawfunc-10 shadow frames."""
import json
import numpy as np
from PIL import Image
from client import RUNTIME


def transitions(alpha):
    widths=[]
    for oriented in (alpha,alpha.T):
        for row in oriented:
            for values in (row,row[::-1]):
                full=np.flatnonzero(values>=.9)
                if not len(full):continue
                end=int(full[0]);low=np.flatnonzero(values[:end+1]<=.1)
                if not len(low):continue
                start=int(low[-1]);widths.append(end-start)
    return widths


def main():
    folder=RUNTIME/'world-sprites';catalog=json.loads((folder/'catalog.json').read_text())['images']
    rows=[]
    # These image IDs are paired with drawfunc 10 in native-bridge/scene.json.
    for image_id in (240,248,277,280):
        entry=catalog[str(image_id)];atlas=np.asarray(Image.open(folder/entry['file']))
        widths=[]
        for frame in entry['frames'][:17]:
            x,y,w,h=(frame[k] for k in ('x','y','w','h'))
            alpha=atlas[y:y+h,x:x+w,3].astype(float)/255
            widths.extend(transitions(alpha))
        values=np.asarray(widths)/entry['scale']
        rows.append({'image':image_id,'sourceSha256':entry['sourceSha256'],'samples':len(values),
                     'gamePixels10to90':{'p10':float(np.percentile(values,10)),'median':float(np.median(values)),
                                        'p90':float(np.percentile(values,90))}})
    report={'source':'native drawfunc-10 Remastered shadow alpha; axial edge sampling',
            'measurements':rows,'shadowSoftnessMeasured':True,
            'limitations':['Axial samples depend on edge angle; these are texture-space measurements.',
                'SceneKit shadow radius requires a matched rendered-ground comparison before calibration.',
                'These measurements do not determine sun elevation. No fitted lighting preset was installed.']}
    path=RUNTIME/'3d-proof/m4/shadow-edges.json';path.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))

if __name__=='__main__':main()
