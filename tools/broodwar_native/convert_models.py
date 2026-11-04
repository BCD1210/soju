"""Convert external M3 models to self-contained SceneKit-compatible glTF meshes."""
import io,json,pathlib,struct,subprocess,hashlib,math,argparse,functools,tempfile
from PIL import Image
from client import RUNTIME

def read_glb(path):
    data=path.read_bytes()
    if len(data)<28:raise ValueError("Truncated GLB")
    magic,version,size=struct.unpack_from("<4sII",data)
    if magic!=b"glTF" or version!=2 or size!=len(data):raise ValueError("Invalid GLB")
    length,kind=struct.unpack_from("<II",data,12)
    if kind!=0x4e4f534a:raise ValueError("Missing JSON")
    document=json.loads(data[20:20+length])
    offset=20+length
    if offset+8>len(data):raise ValueError("GLB has no geometry buffer")
    length,kind=struct.unpack_from("<II",data,offset)
    if kind!=0x004e4942:raise ValueError("Missing BIN")
    return document,bytearray(data[offset+8:offset+8+length])

@functools.lru_cache(maxsize=1)
def texture_sources(root):
    return {hashlib.sha256(p.read_bytes()).hexdigest():p for p in root.glob("*") if p.is_file()}

def compatible(source,dest):
    doc,blob=read_glb(source)
    converted=0
    texture_root=source.parent/"textures"
    originals=texture_sources(texture_root)
    image_paths={}
    for i,image in enumerate(doc.get("images",[])):
        view=doc["bufferViews"][image["bufferView"]]
        raw=blob[view.get("byteOffset",0):view.get("byteOffset",0)+view["byteLength"]]
        path=originals.get(hashlib.sha256(raw).hexdigest())
        if path:image_paths[i]=path
    def embed(image):
        out=io.BytesIO();image.save(out,format="PNG");png=out.getvalue()
        blob.extend(b"\0"*((-len(blob))%4))
        view=len(doc["bufferViews"])
        doc["bufferViews"].append({"buffer":0,"byteOffset":len(blob),"byteLength":len(png)});blob.extend(png)
        index=len(doc.setdefault("images",[]));doc["images"].append({"bufferView":view,"mimeType":"image/png"})
        texture=len(doc.setdefault("textures",[]));doc["textures"].append({"source":index,"sampler":0})
        return texture
    for image in doc.get("images",[]):
        if image.get("mimeType") in ("image/png","image/jpeg"):continue
        view=doc["bufferViews"][image["bufferView"]]
        raw=blob[view.get("byteOffset",0):view.get("byteOffset",0)+view["byteLength"]]
        im=Image.open(io.BytesIO(raw)).convert("RGBA")
        out=io.BytesIO();im.save(out,format="PNG");png=out.getvalue()
        blob.extend(b"\0"*((-len(blob))%4))
        image["bufferView"]=len(doc["bufferViews"]);image["mimeType"]="image/png"
        doc["bufferViews"].append({"buffer":0,"byteOffset":len(blob),"byteLength":len(png)})
        blob.extend(png);converted+=1
    material_report=[]
    for material in doc.get("materials",[]):
        pbr=material.setdefault("pbrMetallicRoughness",{})
        base=pbr.get("baseColorTexture",{}).get("index")
        path=image_paths.get(doc["textures"][base]["source"]) if base is not None else None
        spec=None
        if path:
            candidates=[path.with_name(path.name.replace(a,b)) for a,b in (("@Diffuse","@Specular"),("@Diff","@Spec"),("@Dif","@Spec"),("_Diffuse","_Specular"),("_Diff","_Spec"),("_diff","_spec")) if a in path.name]
            spec=next((p for p in candidates if p.is_file()),None)
        if spec:
            image=Image.open(spec).convert("RGB")
            # Legacy specular intensity is not a measured metalness workflow.
            # Bound the estimate to avoid turning painted armor into mirrors.
            luminance=image.convert("L",matrix=(0.2126,0.7152,0.0722,0))
            roughness=luminance.point([round(max(0.28,min(0.95,math.sqrt(1-i/255)))*255) for i in range(256)])
            metallic=luminance.point([round(max(0,min(0.65,(i/255-0.18)*0.9))*255) for i in range(256)])
            packed=Image.merge("RGB",(Image.new("L",image.size,255),roughness,metallic))
            pbr.update(metallicRoughnessTexture={"index":embed(packed)},metallicFactor=1,roughnessFactor=1)
        material_report.append({"material":material.get("name"),"specularSource":spec.name if spec else None,
            "baseColor":base is not None,"normal":bool(material.get("normalTexture")),"emission":bool(material.get("emissiveTexture")),
            "teamMask":base is not None and material.get("alphaMode","OPAQUE")=="OPAQUE"})
    # Missing SC2 dissolve/warp masks turn authored effect cards into opaque slabs.
    # Omit those unsupported layers, retaining the underlying textured unit mesh.
    omitted=[]
    for mesh in doc.get("meshes",[]):
        retained=[]
        for primitive in mesh["primitives"]:
            material=doc["materials"][primitive.get("material",0)]
            if material.get("alphaMode","OPAQUE") in ("BLEND","MASK") and not material.get("pbrMetallicRoughness",{}).get("baseColorTexture"):
                omitted.append({"material":material.get("name"),"triangles":doc["accessors"][primitive["indices"]]["count"]//3})
            else:retained.append(primitive)
        mesh["primitives"]=retained
    retained_primitives=[p for m in doc.get("meshes",[]) for p in m["primitives"]]
    if not retained_primitives:raise ValueError("No textured geometry after unsupported effect layers removed")
    untextured=sum(doc["accessors"][p["indices"]]["count"]//3 for p in retained_primitives
        if not doc["materials"][p.get("material",0)].get("pbrMetallicRoughness",{}).get("baseColorTexture"))
    for sampler in doc.setdefault("samplers",[{}]):sampler.update(minFilter=9987,magFilter=9729)
    doc["buffers"][0]["byteLength"]=len(blob)
    # Native engine controls movement; only the rig animates within each unit root.
    doc.setdefault("extras",{})["sojuSource"]="SCEvoComplete creator M3; native Brood War simulation"
    text=json.dumps(doc,separators=(",",":")).encode()
    text+=b" "*((-len(text))%4);blob.extend(b"\0"*((-len(blob))%4))
    output=struct.pack("<4sII",b"glTF",2,28+len(text)+len(blob))+struct.pack("<II",len(text),0x4e4f534a)+text+struct.pack("<II",len(blob),0x004e4942)+blob
    positions=[doc["accessors"][p["attributes"]["POSITION"]] for m in doc["meshes"] for p in m["primitives"]]
    triangles=sum(doc["accessors"][p["indices"]]["count"]//3 for m in doc["meshes"] for p in m["primitives"] if p.get("mode",4)==4)
    bounds=[[min(a["min"][i] for a in positions) for i in range(3)],[max(a["max"][i] for a in positions) for i in range(3)]]
    if triangles<50 or any(bounds[1][i]-bounds[0][i]<0.01 for i in range(3)):raise ValueError("Non-volume or empty model")
    temporary=dest.with_suffix(".tmp");temporary.write_bytes(output);temporary.replace(dest)
    return {"file":dest.name,"sha256":hashlib.sha256(output).hexdigest(),"triangles":triangles,
        "omittedEffectLayers":omitted,"untexturedTriangles":untextured,"bounds":bounds,"bones":sum(len(s["joints"]) for s in doc.get("skins",[])),
        "animations":[a.get("name","") for a in doc.get("animations",[])],"convertedImages":converted,"materials":material_report}

def aligned_m3(data):
    """Relocate an unaligned index for the pinned reader; preserve all payload offsets."""
    if len(data)<12 or data[:4] not in (b'43DM',b'33DM',b'23DM'):
        raise ValueError('Invalid M3 header')
    offset,count=struct.unpack_from('<II',data,4)
    end=offset+count*16
    if offset<12 or end>len(data):raise ValueError('Truncated M3 index')
    if offset%4==0:return data
    result=bytearray(data);result.extend(b'\0'*(-len(result)%4))
    aligned=len(result);result.extend(data[offset:end]);struct.pack_into('<I',result,4,aligned)
    return bytes(result)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output",type=pathlib.Path,default=RUNTIME/"models-3d")
    parser.add_argument("--resume",action="store_true")
    args=parser.parse_args()
    source=RUNTIME/"models-source";out=args.output;out.mkdir(parents=True,exist_ok=True)
    manifest=out/"manifest.json"
    report=json.loads(manifest.read_text()).get("models",{}) if args.resume and manifest.exists() else {}
    failures=json.loads(manifest.read_text()).get("failures",{}) if args.resume and manifest.exists() else {}
    from prepare_models import MODELS
    variants=json.loads((source/"variants.json").read_text())
    inputs=[source/(name+".m3") for name in {**MODELS,**variants}]
    for model in sorted(inputs,key=lambda p:("--" in p.stem,p.stem)):
        digest=hashlib.sha256(model.read_bytes()).hexdigest()
        old=report.get(model.stem,{})
        dest=out/(model.stem+".glb")
        if args.resume and old.get("sourceSha256")==digest and dest.exists() and hashlib.sha256(dest.read_bytes()).hexdigest()==old.get("sha256"):continue
        raw=source/(model.stem+".glb")
        try:
            normalized=aligned_m3(model.read_bytes())
            with tempfile.TemporaryDirectory(dir=source,prefix="conversion-") as temporary:
                input_path=pathlib.Path(temporary)/model.name;input_path.write_bytes(normalized)
                subprocess.run([str(RUNTIME/"converter/bin/m3-to-glb"),str(input_path),"-t",str(source/"textures"),"-o",str(raw),"--no-fx"],check=True)
            report[model.stem]={**compatible(raw,dest),"sourceSha256":digest,"indexRealigned":normalized!=model.read_bytes()}
            failures.pop(model.stem,None)
            print(model.stem,json.dumps(report[model.stem]),flush=True)
        except (ValueError,OSError,struct.error,KeyError,subprocess.CalledProcessError) as error:
            failures[model.stem]=str(error);report.pop(model.stem,None)
            print(model.stem,"FAILED",str(error),flush=True)
        manifest.write_text(json.dumps({"sourceCommit":"773c8e8ae8828121e8f37bb74ab82ceb761a45bc","models":report,"failures":failures},indent=2))
    from model_coverage import write_catalog
    write_catalog(out,report,failures)
if __name__=="__main__":main()
