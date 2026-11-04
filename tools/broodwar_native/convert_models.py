"""Convert external M3 models to self-contained SceneKit-compatible glTF meshes."""
import io,json,pathlib,struct,subprocess,hashlib
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
        "animations":[a.get("name","") for a in doc.get("animations",[])],"convertedImages":converted}

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
