"""Validate and atomically install private 3D GLB bundles. Never edits the game."""
import argparse
import datetime
import hashlib
import json
import math
import os
import pathlib
import shutil
import struct
import subprocess
import uuid

ROOT = pathlib.Path(__file__).resolve().parents[2]
RUNTIME = pathlib.Path.home() / ".local/share/soju/broodwar-native"
DEFAULTS = ROOT / "resources/broodwar-models.json"
SIZES = {5120: 1, 5121: 1, 5122: 2, 5123: 2, 5125: 4, 5126: 4}
COMPONENTS = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT2": 4, "MAT3": 9, "MAT4": 16}
FORMATS = {5120: "b", 5121: "B", 5122: "h", 5123: "H", 5125: "I", 5126: "f"}

def require(condition, message):
    if not condition:
        raise ValueError(message)

def contained(root, name):
    require(isinstance(name, str) and name and not pathlib.Path(name).is_absolute(), "Use a relative model path")
    require(".." not in pathlib.Path(name).parts, "Model path traverses parent")
    result = (root / name).resolve()
    require(result.is_relative_to(root.resolve()), "Model path escapes bundle")
    return result

def inspect_glb(path):
    require(path.stat().st_size <= 512*1024*1024, "GLB exceeds 512 MiB")
    data = path.read_bytes()
    require(len(data) >= 20, "Truncated GLB")
    magic, version, length = struct.unpack_from("<III", data)
    require(magic == 0x46546C67 and version == 2 and length == len(data), "Invalid GLB header/length")
    chunks = []
    cursor = 12
    while cursor < length:
        require(cursor+8 <= length, "Truncated GLB chunk")
        size, kind = struct.unpack_from("<II", data, cursor)
        cursor += 8
        require(size % 4 == 0 and cursor+size <= length, "Invalid GLB chunk bounds")
        chunks.append((kind, data[cursor:cursor+size]))
        cursor += size
    require(chunks and chunks[0][0] == 0x4E4F534A, "GLB JSON must be first")
    doc = json.loads(chunks[0][1])
    require(str(doc.get("asset", {}).get("version")) == "2.0", "glTF 2.0 required")
    binary = next((b for k, b in chunks[1:] if k == 0x004E4942), b"")
    buffers = doc.get("buffers", [])
    require(len(buffers) == 1 and "uri" not in buffers[0], "Use one embedded GLB buffer")
    require(0 <= buffers[0]["byteLength"] <= len(binary), "Invalid embedded buffer length")
    views = doc.get("bufferViews", [])
    for view in views:
        offset, size = view.get("byteOffset", 0), view["byteLength"]
        require(view.get("buffer", 0) == 0 and offset >= 0 and size >= 0 and offset+size <= buffers[0]["byteLength"], "Invalid bufferView bounds")
    accessors = doc.get("accessors", [])
    for accessor in accessors:
        require("sparse" not in accessor, "Export dense accessors (sparse not supported by validation)")
        component, shape, count = accessor["componentType"], accessor["type"], accessor["count"]
        require(component in SIZES and shape in COMPONENTS and 0 <= count <= 16_000_000, "Invalid accessor")
        require(isinstance(accessor.get("bufferView"), int) and 0 <= accessor["bufferView"] < len(views), "Accessor has no valid bufferView")
        view = views[accessor["bufferView"]]
        packed = SIZES[component]*COMPONENTS[shape]
        stride = view.get("byteStride", packed)
        offset = accessor.get("byteOffset", 0)
        require(stride >= packed and offset >= 0, "Invalid accessor stride/offset")
        require(offset+max(0,count-1)*stride+(packed if count else 0) <= view["byteLength"], "Accessor exceeds view")
    def values(index):
        require(isinstance(index, int) and 0 <= index < len(accessors), "Invalid accessor index")
        a = accessors[index]; v = views[a["bufferView"]]
        width = COMPONENTS[a["type"]]
        size = SIZES[a["componentType"]]*width
        start = v.get("byteOffset", 0)+a.get("byteOffset", 0)
        stride = v.get("byteStride", size)
        fmt = "<"+FORMATS[a["componentType"]]*width
        return [struct.unpack_from(fmt, binary, start+i*stride) for i in range(a["count"])]
    triangles = 0
    low, high = [math.inf]*3, [-math.inf]*3
    for mesh in doc.get("meshes", []):
        for primitive in mesh.get("primitives", []):
            require(primitive.get("mode", 4) == 4, "Export triangulated geometry")
            require("POSITION" in primitive.get("attributes", {}), "Mesh has no POSITION")
            positions = values(primitive["attributes"]["POSITION"])
            require(positions and len(positions[0]) == 3, "Invalid positions")
            for p in positions:
                require(all(math.isfinite(x) for x in p), "Nonfinite geometry")
                for i, x in enumerate(p):
                    low[i], high[i] = min(low[i], x), max(high[i], x)
            if "indices" in primitive:
                indices = values(primitive["indices"])
                require(all(len(i) == 1 and 0 <= i[0] < len(positions) for i in indices), "Mesh index outside vertex array")
                count = len(indices)
            else:
                count = len(positions)
            require(count % 3 == 0, "Triangle index count is not divisible by 3")
            triangles += count//3
    require(triangles > 0 and all(high[i]-low[i] > 1e-6 for i in range(3)), "Real nonempty volumetric mesh required")
    nodes = doc.get("nodes", [])
    require(nodes, "GLB has no scene nodes")
    for node in nodes:
        require(all(isinstance(i, int) and 0 <= i < len(nodes) for i in node.get("children", [])), "Invalid child node")
    visiting, visited = set(), set()
    def walk(index):
        require(index not in visiting, "Cyclic node hierarchy")
        if index in visited:
            return
        visiting.add(index)
        for child in nodes[index].get("children", []):
            walk(child)
        visiting.remove(index); visited.add(index)
    for index in range(len(nodes)):
        walk(index)
    for skin in doc.get("skins", []):
        require(skin.get("joints") and all(0 <= i < len(nodes) for i in skin["joints"]), "Invalid skin joints")
    clips = []
    for animation in doc.get("animations", []):
        require(animation.get("name"), "Every animation needs a name")
        require(animation["name"] not in clips, "Duplicate animation name")
        for channel in animation.get("channels", []):
            target = channel["target"]
            require(0 <= target.get("node", -1) < len(nodes) and target.get("path") in ("translation", "rotation", "scale", "weights"), "Invalid animation target")
            sampler = animation["samplers"][channel["sampler"]]
            times = [x[0] for x in values(sampler["input"])]
            require(times and all(math.isfinite(x) for x in times) and all(a < b for a, b in zip(times, times[1:])), "Invalid animation key times")
            require(all(math.isfinite(x) for row in values(sampler["output"]) for x in row), "Nonfinite animation output")
        clips.append(animation["name"])
    for image in doc.get("images", []):
        require("uri" not in image and image.get("mimeType") in ("image/png", "image/jpeg"), "Embed standard PNG/JPEG textures")
        require(0 <= image.get("bufferView", -1) < len(views), "Invalid embedded texture")
    required = doc.get("extensionsRequired", [])
    require(not required, "Export uncompressed standard glTF; required extensions: "+str(required))
    return {"file": path.name, "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data),
            "triangles": triangles, "bounds": [low, high], "skins": len(doc.get("skins", [])),
            "textures": len(doc.get("images", [])), "animations": clips}

def bundle(source, domain="units"):
    require(domain in ("units", "effects"), "Unknown asset domain")
    defaults = json.loads(DEFAULTS.read_text())["models"] if domain == "units" else {}
    manifest = source/"catalog.json"
    if manifest.is_file():
        doc = json.loads(manifest.read_text())
        require(doc.get("schemaVersion") == 1 and isinstance(doc.get("models"), dict), "Invalid catalog.json")
        models = doc["models"]
    else:
        known = {v["file"]: k for k, v in defaults.items()}
        models = {}
        for path in source.glob("*.glb"):
            kind = known.get(path.name)
            if path.stem.startswith("unit-") and path.stem[5:].isdigit():
                kind = path.stem[5:]
            require(kind is not None, f"Unknown {path.name}: supply catalog.json with native unit ID")
            definition = dict(defaults.get(kind, {}))
            sidecar = path.with_suffix(".json")
            if sidecar.exists():
                definition.update(json.loads(sidecar.read_text()))
            definition["file"] = path.name
            models[kind] = definition
    require(models, "No GLB models found")
    result = {}
    for key, definition in models.items():
        require(key.isdigit() and 0 <= int(key) <= 65535, "Invalid unit ID")
        require(isinstance(definition, dict), "Model entry must be an object")
        path = contained(source, definition["file"])
        require(path.suffix.lower() == ".glb", "Model file must end in .glb")
        for name in ("scale", "targetHeight", "yawDegrees", "elevation", "flightHeight"):
            if name in definition:
                value = definition[name]
                require(isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value), "Invalid "+name)
                if name in ("scale", "targetHeight"):
                    require(0 < value <= 1000, "Invalid "+name)
        if "flightHeight" in definition:
            require(0 <= definition["flightHeight"] < 1000, "Invalid flightHeight")
        require(definition.get("teamMask", "none") in ("alpha", "texture", "none"), "Invalid teamMask")
        if definition.get("teamMask") == "texture":
            mask = contained(source, definition.get("teamMaskTexture", ""))
            require(mask.is_file() and mask.suffix.lower() in (".png", ".jpg", ".jpeg"), "Invalid teamMaskTexture")
        for field in ("turretNodes", "teamMaterials"):
            if field in definition:
                require(isinstance(definition[field], list) and all(isinstance(v, str) and v for v in definition[field]), "Invalid "+field)
        result[key] = (definition, path, inspect_glb(path))
    return result

def install(source, runtime=RUNTIME, check=True, domain="units"):
    source = source.expanduser().resolve()
    models = bundle(source, domain)  # Validate the complete bundle before any activation.
    folder = runtime/("effects-3d" if domain == "effects" else "models-3d")
    folder.mkdir(parents=True, exist_ok=True)
    release = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")+"-"+uuid.uuid4().hex[:8]
    stage = folder/"releases"/release
    stage.mkdir(parents=True)
    previous = folder/"catalog.json"
    prior_bytes = previous.read_bytes() if previous.exists() else None
    current = json.loads(prior_bytes) if prior_bytes else {"schemaVersion": 1, "models": {}}
    require(current.get("schemaVersion") == 1, "Invalid installed catalog")
    entries = dict(current["models"])
    for kind, (definition, path, info) in models.items():
        target = stage/f"unit-{kind}.glb"
        shutil.copy2(path, target)
        entries[kind] = {**definition, "file": str(target.relative_to(folder))}
        if definition.get("teamMask") == "texture":
            mask = contained(source, definition["teamMaskTexture"])
            copied = stage/f"mask-{kind}{mask.suffix}"
            shutil.copy2(mask, copied)
            entries[kind]["teamMaskTexture"] = str(copied.relative_to(folder))
    proposed = {"schemaVersion": 1, "release": release, "models": entries}
    candidate = stage/"catalog.json"
    candidate.write_text(json.dumps(proposed, indent=2)+"\n")
    report = {"release": release, "models": {k: v[2] for k, v in models.items()}, "rendererChecked": check}
    if check:
        exe = runtime/"soju-bw-scene"
        require(exe.is_file(), "Build the scene before installing assets")
        subprocess.run([str(exe), "--model-check", "--model-dir", str(folder), "--catalog", str(candidate),
                        "--model-types", ",".join(models), "--proof-dir", str(stage/"proof")]+(["--no-default-catalog"] if domain == "effects" else []), check=True)
    # Detect concurrent installs instead of silently clobbering another catalog.
    require((previous.read_bytes() if previous.exists() else None) == prior_bytes, "Catalog changed during validation; retry")
    (stage/"previous-catalog.json").write_text(json.dumps(current, indent=2)+"\n")
    (stage/"validation.json").write_text(json.dumps(report, indent=2)+"\n")
    temporary = folder/("catalog."+release+".tmp")
    shutil.copy2(candidate, temporary)
    os.replace(temporary, previous)
    return report

def rollback(runtime=RUNTIME, domain="units"):
    require(domain in ("units", "effects"), "Unknown asset domain")
    folder = runtime/("effects-3d" if domain == "effects" else "models-3d")
    current = json.loads((folder/"catalog.json").read_text())
    release = current.get("release", "")
    require(release and "/" not in release and ".." not in release, "No installed release to roll back")
    saved = folder/"releases"/release/"previous-catalog.json"
    previous = json.loads(saved.read_text())
    require(previous.get("schemaVersion") == 1, "Invalid previous catalog")
    temporary = folder/"catalog.rollback.tmp"
    temporary.write_text(json.dumps(previous, indent=2)+"\n")
    os.replace(temporary, folder/"catalog.json")
    return {"rolledBack": release, "activeRelease": previous.get("release", "baseline")}

def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("action", choices=["check", "install", "rollback"])
    p.add_argument("source", nargs="?", type=pathlib.Path)
    p.add_argument("--runtime", type=pathlib.Path, default=RUNTIME)
    p.add_argument("--domain", choices=["units", "effects"], default="units")
    a = p.parse_args(argv)
    if a.action == "rollback":
        result = rollback(a.runtime, a.domain)
    else:
        p.error("source folder is required") if a.source is None else None
        result = install(a.source, a.runtime, domain=a.domain) if a.action == "install" else {k: v[2] for k, v in bundle(a.source, a.domain).items()}
    print(json.dumps(result, indent=2))
    return 0

if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, KeyError, IndexError, OSError, struct.error, subprocess.CalledProcessError) as error:
        raise SystemExit(f"3D assets: {error}")
