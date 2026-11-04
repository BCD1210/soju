"""Compare saved simulator captures against desktop proofs in sRGB and linear light."""
import argparse
import json
from pathlib import Path
import numpy as np
from PIL import Image


def luminance(path, crop_top=0):
    image=np.asarray(Image.open(path).convert('RGB'),dtype=float)[crop_top:]/255
    linear=np.where(image<=0.04045,image/12.92,((image+0.055)/1.055)**2.4)
    weights=np.array([0.2126,0.7152,0.0722])
    return {'file':str(path),'encodedMean':float((image*weights).sum(axis=2).mean()),
            'linearMean':float((linear*weights).sum(axis=2).mean()),'cropTop':crop_top}


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('before',type=Path);p.add_argument('after',type=Path)
    p.add_argument('--proofs',type=Path,required=True);p.add_argument('--output',type=Path,required=True)
    a=p.parse_args()
    before=luminance(a.before,145);after=luminance(a.after,145)
    references=[luminance(path) for path in sorted(a.proofs.glob('*-player-1.png'))]
    reference=float(np.mean([r['encodedMean'] for r in references]))
    ratio=after['encodedMean']/reference
    passed=after['encodedMean']>before['encodedMean']*1.5 and 0.7<ratio<1.5
    report={'passed':passed,'before':before,'after':after,'fullBefore':luminance(a.before),
            'fullAfter':luminance(a.after),'desktopMean':reference,'desktopRatio':ratio,'references':references,
            'method':'Rec.709 coefficients; scene crop excludes top simulator chrome; full frame also reported. Different scene compositions are an approximate brightness comparison.'}
    a.output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
    if not passed:raise SystemExit(1)
