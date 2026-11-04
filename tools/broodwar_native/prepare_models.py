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

# Original units.dat IDs; source paths are pinned, including explicitly labelled skins.
UNIT_MODELS={0:"marine",7:"scv",106:"command-center",109:"supply-depot",176:"mineral-0",177:"mineral-1",178:"mineral-2",188:"geyser"}
MODELS.update({
    'ghost': 'Units/Terran/GhostSCBW/GhostSCBW.m3',
    'vulture': 'Units/Terran/VultureSCBW/VultureSCBW.m3',
    'goliath': 'Units/Terran/GoliathSCBW/GoliathSCBW.m3',
    'siege-tank': 'Units/Terran/SiegeTankSCBW/SiegeTankSCBW.m3',
    'wraith': 'Units/Terran/WraithSCBW/WraithSCBW.m3',
    'science-vessel': 'Units/Terran/ScienceVesselSCBW/ScienceVesselSCBW.m3',
    'dropship': 'Units/Terran/DropshipSCBW/DropshipSCBW.m3',
    'spider-mine': 'Units/Terran/SpiderMineSCBW/SpiderMineSCBW.m3',
    'siege-tank-sieged': 'Units/Terran/SiegeTankSCBW/SiegeTankSCBW@Sieged.m3',
    'firebat': 'Units/Terran/FirebatSCBW/FirebatSCBW.m3',
    'larva': 'Units/Zerg/LarvaSCBW/LarvaSCBW.m3',
    'egg': 'Units/Zerg/ZergEggSCBW/ZergEggSCBW.m3',
    'zergling': 'Units/Zerg/ZerglingSCBW/ZerglingSCBW.m3',
    'hydralisk-webby': 'Units/Zerg/HydraliskSCBW_Webby/HydraliskSCBW_Webby.m3',
    'ultralisk': 'Units/Zerg/UltraliskSCBW/UltraliskSCBW.m3',
    'drone-undead': 'Units/Zerg/DroneSCBW_Undead/DroneSCBW_Undead.m3',
    'overlord': 'Units/Zerg/OverlordSCBW/OverlordSCBW.m3',
    'mutalisk': 'Units/Zerg/MutaliskSCBW/MutaliskSCBW.m3',
    'guardian': 'Units/Zerg/GuardianSCBW/GuardianSCBW.m3',
    'queen': 'Units/Zerg/QueenSCBW/QueenSCBW.m3',
    'defiler': 'Units/Zerg/DefilerSCBW/DefilerSCBW.m3',
    'scourge': 'Units/Zerg/ScourgeSCBW/ScourgeSCBW.m3',
    'valkyrie': 'Units/Terran/ValkyrieSCBW/ValkyrieSCBW.m3',
    'cocoon': 'Units/Zerg/GuardianSCBWEgg/GuardianSCBWEgg.m3',
    'corsair': 'Units/Protoss/CorsairSCBW/CorsairSCBW.m3',
    'devourer': 'Units/Zerg/DevourerSCBW/DevourerSCBW.m3',
    'dark-archon': 'Units/Protoss/DarkArchonSCBW/DarkArchonSCBW.m3',
    'probe-dark': 'Units/Protoss/ProbeSCBW_Dark/ProbeSCBW_Dark.m3',
    'zealot': 'Units/Protoss/ZealotSCBW/ZealotSCBW.m3',
    'dragoon': 'Units/Protoss/DragoonSCBW/DragoonSCBW.m3',
    'high-templar': 'Units/Protoss/HighTemplarSCBW/HighTemplarSCBW.m3',
    'archon': 'Units/Protoss/ArchonSCBW/ArchonSCBW.m3',
    'shuttle': 'Units/Protoss/ShuttleSCBW/ShuttleSCBW.m3',
    'scout': 'Units/Protoss/ScoutSCBW/ScoutSCBW.m3',
    'arbiter': 'Units/Protoss/ArbiterSCBW/ArbiterSCBW.m3',
    'carrier': 'Units/Protoss/CarrierSCBW/CarrierSCBW.m3',
    'interceptor': 'Units/Protoss/Interceptor_Remastered/Interceptor_Remastered_fix.m3',
    'reaver': 'Units/Protoss/ReaverSCBW/ReaverSCBW.m3',
    'lurker': 'Units/Zerg/LurkerSCBW/LurkerSCBW.m3',
    'comsat': 'Buildings/Terran/ComsatStationSCBW/ComsatStationSCBW.m3',
    'nuclear-silo': 'Buildings/Terran/NuclearSiloSCBW/NuclearSiloSCBW.m3',
    'refinery': 'Buildings/Terran/RefinerySCBW/RefinerySCBW.m3',
    'barracks': 'Buildings/Terran/BarracksSCBW/BarracksSCBW.m3',
    'academy': 'Buildings/Terran/AcademySCBW/AcademySCBW.m3',
    'factory': 'Buildings/Terran/FactorySCBW/FactorySCBW.m3',
    'control-tower': 'Buildings/Terran/ControlTowerSCBW/ControlTowerSCBW.m3',
    'science-facility': 'Buildings/Terran/ScienceFacilitySCBW/ScienceFacilitySCBW.m3',
    'covert-ops': 'Buildings/Terran/CovertOpsSCBW/CovertOpsSCBW.m3',
    'machine-shop': 'Buildings/Terran/MachineShopSCBW/MachineShopSCBWFix.m3',
    'engineering-bay': 'Buildings/Terran/EngineeringBaySCBW/EngineeringBaySCBW.m3',
    'missile-turret': 'Buildings/Terran/MissileTurretSCBW/MissileTurretSCBW.m3',
    'bunker': 'Buildings/Terran/BunkerSCBW/BunkerSCBW.m3',
    'infested-command-center': 'Buildings/Zerg/InfestedCommandCenterSCBW/InfestedCommandCenterSCBW.m3',
    'hatchery': 'Buildings/Zerg/HatcherySCBW/HatcherySCBW.m3',
    'defiler-mound': 'Buildings/Zerg/DefilerMoundSCBW/DefilerMoundSCBW.m3',
    'greater-spire': 'Buildings/Zerg/GreaterSpireSCBW/GreaterSpireSCBW.m3',
    'queens-nest': 'Buildings/Zerg/QueensNestSCBW/QueensNestSCBW.m3',
    'evolution-chamber': 'Buildings/Zerg/EvolutionChamberSCBW/EvolutionChamberSCBW.m3',
    'spire': 'Buildings/Zerg/SpireSCBW/SpireSCBW.m3',
    'spawning-pool': 'Buildings/Zerg/SpawningPoolSCBW/SpawningPoolSCBW.m3',
    'creep-colony': 'Buildings/Zerg/CreepColonySCBW/CreepColonySCBW.m3',
    'spore-colony': 'Buildings/Zerg/SporeColonySCBW/SporeColonySCBW.m3',
    'sunken-colony': 'Buildings/Zerg/SunkenColonySCBW/SunkenColonySCBW.m3',
    'extractor': 'Buildings/Zerg/ExtractorSCBW/ExtractorSCBW.m3',
    'nexus': 'Buildings/Protoss/NexusSCBW/NexusSCBW.m3',
    'robotics-facility': 'Buildings/Protoss/RoboticsFacilitySCBW/RoboticsFacilitySCBW.m3',
    'pylon': 'Buildings/Protoss/PylonSCBW/PylonSCBW.m3',
    'assimilator': 'Buildings/Protoss/AssimilatorSCBW/AssimilatorSCBW.m3',
    'observatory': 'Buildings/Protoss/ObservatorySCBW/ObservatorySCBW.m3',
    'gateway': 'Buildings/Protoss/GatewaySCBW/GatewaySCBW.m3',
    'photon-cannon': 'Buildings/Protoss/PhotonCannonSCBW/PhotonCannonSCBW.m3',
    'citadel-of-adun': 'Buildings/Protoss/CitadelOfAdunSCBW/CitadelofAdunSCBW.m3',
    'cybernetics-core': 'Buildings/Protoss/CyberneticsCoreSCBW/CyberneticsCoreSCBW.m3',
    'templar-archives': 'Buildings/Protoss/TemplarArchivesSCBW/TemplarArchivesSCBW.m3',
    'forge': 'Buildings/Protoss/ForgeSCBW/ForgeSCBW.m3',
    'stargate': 'Buildings/Protoss/StargateSCBW/StargateSCBW.m3',
    'fleet-beacon': 'Buildings/Protoss/FleetBeaconSCBW/FleetBeaconSCBW.m3',
    'arbiter-tribunal': 'Buildings/Protoss/ArbiterTribunalSCBW/ArbiterTribunalSCBW.m3',
    'robotics-support-bay': 'Buildings/Protoss/RoboticsSupportBaySCBW/RoboticsSupportBaySCBW.m3',
    'shield-battery': 'Buildings/Protoss/ShieldBatterySCBW/ShieldBatterySCBW.m3',
})
UNIT_MODELS.update({1: 'ghost', 2: 'vulture', 3: 'goliath', 5: 'siege-tank', 8: 'wraith', 9: 'science-vessel', 11: 'dropship', 13: 'spider-mine', 30: 'siege-tank-sieged', 32: 'firebat', 35: 'larva', 36: 'egg', 37: 'zergling', 38: 'hydralisk-webby', 39: 'ultralisk', 41: 'drone-undead', 42: 'overlord', 43: 'mutalisk', 44: 'guardian', 45: 'queen', 46: 'defiler', 47: 'scourge', 58: 'valkyrie', 59: 'cocoon', 60: 'corsair', 62: 'devourer', 63: 'dark-archon', 64: 'probe-dark', 65: 'zealot', 66: 'dragoon', 67: 'high-templar', 68: 'archon', 69: 'shuttle', 70: 'scout', 71: 'arbiter', 72: 'carrier', 73: 'interceptor', 83: 'reaver', 103: 'lurker', 107: 'comsat', 108: 'nuclear-silo', 110: 'refinery', 111: 'barracks', 112: 'academy', 113: 'factory', 115: 'control-tower', 116: 'science-facility', 117: 'covert-ops', 120: 'machine-shop', 122: 'engineering-bay', 124: 'missile-turret', 125: 'bunker', 130: 'infested-command-center', 131: 'hatchery', 136: 'defiler-mound', 137: 'greater-spire', 138: 'queens-nest', 139: 'evolution-chamber', 141: 'spire', 142: 'spawning-pool', 143: 'creep-colony', 144: 'spore-colony', 146: 'sunken-colony', 149: 'extractor', 154: 'nexus', 155: 'robotics-facility', 156: 'pylon', 157: 'assimilator', 159: 'observatory', 160: 'gateway', 162: 'photon-cannon', 163: 'citadel-of-adun', 164: 'cybernetics-core', 165: 'templar-archives', 166: 'forge', 167: 'stargate', 169: 'fleet-beacon', 170: 'arbiter-tribunal', 171: 'robotics-support-bay', 172: 'shield-battery'})
UNIT_MODELS.update({132:"hatchery",133:"hatchery"})
FORM_CLIPS={132:{"idle":["Stand B_full"],"work":["Stand Work B_full"]},133:{"idle":["Stand C_full"],"work":["Stand Work C_full"]}}
HERO_BASES={10: 32, 16: 1, 17: 3, 19: 2, 20: 0, 21: 8, 22: 9, 23: 5, 25: 30, 48: 39, 49: 45, 52: 46, 53: 38, 54: 37, 55: 43, 56: 44, 57: 42, 76: 68, 77: 65, 78: 66, 79: 67, 80: 70, 81: 83, 82: 72, 86: 71, 87: 67, 88: 70, 98: 60, 99: 1, 100: 1, 104: 1}
UNIT_MODELS.update({kind:UNIT_MODELS[base] for kind,base in HERO_BASES.items()})
SKIN_TYPES={38:"Webby skin; base Hydralisk absent",41:"Undead skin; base Drone absent",64:"Dark skin; base Probe absent",7:"KMC skin"}

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
 previous=json.loads((root/"provenance.json").read_text()) if (root/"provenance.json").exists() else {}
 records=[];missing=set()
 prior={r["local"]:r for r in previous.get("files",[])}
 def save(path,dest):
  record=prior.get(str(dest.relative_to(root)))
  if dest.exists() and record and record["source"]==path and hashlib.sha256(dest.read_bytes()).hexdigest()==record["sha256"]:
   data=dest.read_bytes()
  else:
   data=fetch(path);dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(data)
  records.append({"source":path,"local":str(dest.relative_to(root)),"sha256":hashlib.sha256(data).hexdigest()});return data
 variants={}
 for name,path in MODELS.items():
  stem=path[:-3]
  for entry in tree:
   candidate=entry["path"]
   if candidate.startswith(BASE+stem+"@") and candidate.endswith(".m3") and any(tag in candidate for tag in ("@Death","@Birth")):
    variants[name+"--"+pathlib.PurePosixPath(candidate).stem.split("@")[-1].lower()]=candidate[len(BASE):]
 (root/"variants.json").write_text(json.dumps(variants,indent=2))
 for name,path in {**MODELS,**variants}.items():
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
 (root/"provenance.json").write_text(json.dumps({"repository":SOURCE,"commit":COMMIT,"files":records,"missingTextures":sorted(missing),"unitModels":UNIT_MODELS,"variants":variants},indent=2))
 print("Missing textures:",sorted(missing),flush=True)
if __name__=="__main__":main()
