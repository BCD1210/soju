"""Download pinned creator models and retain provenance; never copy them into git."""
import hashlib,json,pathlib,re,urllib.request
from concurrent.futures import ThreadPoolExecutor
from client import RUNTIME
SOURCE="TeamKoprulu/SCEvoComplete"
COMMIT="773c8e8ae8828121e8f37bb74ab82ceb761a45bc"
BASE="SC Evo Complete/SCEvo_Assets.SC2Mod/Base.SC2Assets/Assets/"
MODELS={
 "marine":"Units/Terran/MarineSCBW/MarineSCBW.m3",
 "scv":"Units/Terran/SCVSCBW_KMC/SCVSCBW_KMC.m3",
 "command-center":"Buildings/Terran/CommandCenterSCBW/CommandCenterSCBW.m3",
 "supply-depot":"Buildings/Terran/SupplyDepotSCBW/SupplyDepotSCBW.m3",
 **{f"mineral-{i}":f"Buildings/Resources/ResourceMineralClassicSCBW/ResourceMineralClassicSCBW_0{i}.m3" for i in range(3)},
 "geyser":"Buildings/Resources/TempleGeyser/TempleGeyser_Updated.m3",
}
def fetch(path):
 url=f"https://raw.githubusercontent.com/{SOURCE}/{COMMIT}/"+urllib.parse.quote(path)
 with urllib.request.urlopen(url,timeout=60) as r:return r.read()
def main():
 root=RUNTIME/"models-source";root.mkdir(parents=True,exist_ok=True)
 tree_file=RUNTIME/"3d-source-tree.json"
 if not tree_file.exists():
  url=f"https://api.github.com/repos/{SOURCE}/git/trees/{COMMIT}?recursive=1"
  with urllib.request.urlopen(url,timeout=60) as response:listing=json.load(response)
  if listing.get("truncated"):raise RuntimeError("Creator asset tree was truncated")
  tree_file.write_text(json.dumps({"commit":COMMIT,**listing}))
 tree_document=json.loads(tree_file.read_text())
 if tree_document.get("commit")!=COMMIT:raise RuntimeError("Creator asset source commit mismatch")
 tree=tree_document["tree"]
 names={pathlib.PurePosixPath(x["path"]).name.lower():x["path"] for x in tree if x["type"]=="blob"}
 records=[];missing=set()
 def save(path,dest):
  data=fetch(path);dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(data)
  records.append({"source":path,"local":str(dest.relative_to(root)),"sha256":hashlib.sha256(data).hexdigest()});return data
 for name,path in MODELS.items():
  data=save(BASE+path,root/(name+".m3"))
  refs={p.decode("ascii").replace("\\","/").split("/")[-1] for p in re.findall(rb"Assets[\\/][ -~]{1,240}?\.(?:dds|tga)",data,re.I)}
  print(name,"textures",sorted(refs),flush=True)
  for ref in refs:
   target=root/"textures"/ref
   if ref.lower() in names:
    if target.exists():records.append({"source":names[ref.lower()],"local":str(target.relative_to(root)),"sha256":hashlib.sha256(target.read_bytes()).hexdigest()})
    else:save(names[ref.lower()],target)
   else:missing.add(ref)
 for file in ["LICENSE","README.md"]:save(file,root/file)
 (root/"provenance.json").write_text(json.dumps({"repository":SOURCE,"commit":COMMIT,"files":records,"missingTextures":sorted(missing)},indent=2))
 print("Missing textures:",sorted(missing),flush=True)
if __name__=="__main__":main()
