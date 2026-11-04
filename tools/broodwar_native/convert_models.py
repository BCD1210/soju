"""Convert external M3 models to self-contained SceneKit-compatible glTF meshes."""
import io,json,pathlib,struct,subprocess,hashlib,math
from PIL import Image
from client import RUNTIME

def read_glb(path):
    data=path.read_bytes()
    magic,version,size=struct.unpack_from("<4sII",data)
    if magic!=b"glTF" or version!=2 or size!=len(data):raise ValueError("Invalid GLB")
    length,kind=struct.unpack_from("<II",data,12)
    if kind!=0x4e4f534a:raise ValueError("Missing JSON")
    document=json.loads(data[20:20+length])
    offset=20+length
    length,kind=struct.unpack_from("<II",data,offset)
    if kind!=0x004e4942:raise ValueError("Missing BIN")
    return document,bytearray(data[offset+8:offset+8+length])

def compatible(source,dest):
    doc,blob=read_glb(source)
    converted=0
    texture_root=source.parent/"textures"
    originals={hashlib.sha256(p.read_bytes()).hexdigest():p for p in texture_root.glob("*") if p.is_file()}
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
            candidates=[path.with_name(path.name.replace(a,b)) for a,b in (("@Diff","@Spec"),("_Diff","_Spec"),("_diff","_spec")) if a in path.name]
            spec=next((p for p in candidates if p.is_file()),None)
        if spec:
            image=Image.open(spec).convert("RGB")
            # Legacy specular intensity is not a measured metalness workflow.
            # Bound the estimate to avoid turning painted armor into mirrors.
            pixels=[]
            for r,g,b in image.getdata():
                intensity=(0.2126*r+0.7152*g+0.0722*b)/255
                roughness=max(0.28,min(0.95,math.sqrt(1-intensity)))
                metallic=max(0,min(0.65,(intensity-0.18)*0.9))
                pixels.append((255,round(roughness*255),round(metallic*255)))
            packed=Image.new("RGB",image.size);packed.putdata(pixels)
            pbr.update(metallicRoughnessTexture={"index":embed(packed)},metallicFactor=1,roughnessFactor=1)
        material_report.append({"material":material.get("name"),"specularSource":spec.name if spec else None,
            "normal":bool(material.get("normalTexture")),"emission":bool(material.get("emissiveTexture"))})
    for sampler in doc.setdefault("samplers",[{}]):sampler.update(minFilter=9987,magFilter=9729)
    doc["buffers"][0]["byteLength"]=len(blob)
    # Native engine controls movement; only the rig animates within each unit root.
    doc.setdefault("extras",{})["sojuSource"]="SCEvoComplete creator M3; native Brood War simulation"
    text=json.dumps(doc,separators=(",",":")).encode()
    text+=b" "*((-len(text))%4);blob.extend(b"\0"*((-len(blob))%4))
    output=struct.pack("<4sII",b"glTF",2,28+len(text)+len(blob))+struct.pack("<II",len(text),0x4e4f534a)+text+struct.pack("<II",len(blob),0x004e4942)+blob
    dest.write_bytes(output)
    positions=[doc["accessors"][p["attributes"]["POSITION"]] for m in doc["meshes"] for p in m["primitives"]]
    triangles=sum(doc["accessors"][p["indices"]]["count"]//3 for m in doc["meshes"] for p in m["primitives"] if p.get("mode",4)==4)
    bounds=[[min(a["min"][i] for a in positions) for i in range(3)],[max(a["max"][i] for a in positions) for i in range(3)]]
    if triangles<50 or any(bounds[1][i]-bounds[0][i]<0.01 for i in range(3)):raise ValueError("Non-volume or empty model")
    return {"file":dest.name,"sha256":hashlib.sha256(output).hexdigest(),"triangles":triangles,
        "bounds":bounds,"bones":sum(len(s["joints"]) for s in doc.get("skins",[])),
        "animations":[a.get("name","") for a in doc.get("animations",[])],"convertedImages":converted,"materials":material_report}

def main():
    source=RUNTIME/"models-source";out=RUNTIME/"models-3d";out.mkdir(exist_ok=True)
    report={}
    for model in sorted(source.glob("*.m3")):
        raw=source/(model.stem+".glb")
        subprocess.run([str(RUNTIME/"converter/bin/m3-to-glb"),str(model),"-t",str(source/"textures"),"-o",str(raw),"--no-fx"],check=True)
        report[model.stem]=compatible(raw,out/(model.stem+".glb"))
        print(model.stem,json.dumps(report[model.stem]),flush=True)
    (out/"manifest.json").write_text(json.dumps({"sourceCommit":"773c8e8ae8828121e8f37bb74ab82ceb761a45bc","models":report},indent=2))
if __name__=="__main__":main()
