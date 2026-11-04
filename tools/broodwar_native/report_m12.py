"""Summarize private M12 evidence without treating missing gates as passes."""
import json
from pathlib import Path

from client import RUNTIME


def report():
    folder = RUNTIME/"3d-proof/m12"
    rows = []
    for name, filename in [("Frozen baseline", "baseline-final-metrics.json"),
                           ("Frozen instrumented", "instrumented-final-metrics.json"),
                           ("Live first map", "live-first/live-final-metrics.json"),
                           ("Live residency failure", "live-residency-failure/live-metrics.json"),
                           ("Live land duel", "live-final-metrics.json")]:
        path = folder/filename
        if not path.exists():
            continue
        data = json.loads(path.read_text())
        cadence = data["frameIntervals"]
        rows.append({"name": name, "seconds": data["elapsedSeconds"],
                     "cadenceP95Ms": cadence["p95Ms"], "missedSlots": cadence["estimatedMissedRefreshSlots"],
                     "cpuP95Ms": data["stereoCPUEncode"]["p95Ms"], "gpuP95Ms": data["stereoGPU"]["p95Ms"]})
    live = next((row for row in rows if row["name"] == "Live land duel"), None)
    samples_path = folder/"live-state-samples.json"
    samples = json.loads(samples_path.read_text()) if samples_path.exists() else []
    advancing = len(samples)>1 and all(b["frame"]>a["frame"] for a,b in zip(samples,samples[1:]))
    complete = live is not None and live["seconds"]>=600 and advancing and samples[-1]["seconds"]>=600
    result = {"timings": rows, "tenMinuteLiveMeasurement": complete,
              "cadencePass": bool(complete and live["cadenceP95Ms"]<=11.1),
              "missedSlotsPass": bool(complete and live["missedSlots"]<live["seconds"]/10),
              "zeroFrameAllocationsVerified": False, "completeGroundingVerified": False,
              "cliffStretchingFixed": False, "totalTextureResidencyVerified": False,
              "rendererTextureBudgetMiB": 1536, "overallAccepted": False}
    grounding_path=folder/"building-views/grounding.json"
    if grounding_path.exists():
        buildings=json.loads(grounding_path.read_text())["buildings"]
        result["buildingDiagnosticTypes"]=len(buildings)
        result["buildingDiagnosticViews"]=sum(len(row["files"]) for row in buildings)
        result["emptyBuildingTypes"]=sorted(row["type"] for row in buildings if row["nativeImages"]==0)
        building_sheets(folder,buildings)
    (folder/"acceptance.json").write_text(json.dumps(result, indent=2)+"\n")
    lines = ["# M12 timing evidence", "", "Frozen and live scenes are different workloads. Instrumented timings are diagnostic.", "",
             "| Run | Seconds | Cadence p95 ms | CPU p95 ms | GPU p95 ms | Missed slots |",
             "| --- | ---: | ---: | ---: | ---: | ---: |"]
    lines += [f"| {r['name']} | {r['seconds']:.3f} | {r['cadenceP95Ms']:.2f} | {r['cpuP95Ms']:.2f} | {r['gpuP95Ms']:.2f} | {r['missedSlots']} |" for r in rows]
    lines += ["", "Missed slots are cadence estimates, not compositor telemetry. CPU and GPU overlap.", "",
              f"Ten-minute live evidence complete: {complete}. Overall acceptance remains false."]
    (folder/"timings.md").write_text("\n".join(lines)+"\n")
    print(json.dumps(result, indent=2))


def building_sheets(folder, buildings):
    from PIL import Image, ImageDraw
    names=json.loads((Path(__file__).resolve().parents[2]/"resources/broodwar-unit-types.json").read_text())["types"]
    rows=sorted(buildings,key=lambda row:row["type"])
    lines=["| Type | Building | Views | Native layers |", "| ---: | --- | ---: | ---: |"]
    for row in rows:
        lines.append(f"| {row['type']} | {names[str(row['type'])]} | {len(row['files'])} | {row['nativeImages']} |")
    lines += ["", "Type 214 is a non-rendered start-location marker. Empty image chains remain visible as gaps in this table.",
              "Four-angle SceneKit geometry diagnostics on flat ground do not certify complete footprint contact, slopes or XR rendering."]
    (folder/"building-coverage.md").write_text("\n".join(lines)+"\n")
    for page in range((len(rows)+19)//20):
        sheet=Image.new("RGB",(1024,1440),(25,25,25));draw=ImageDraw.Draw(sheet)
        for index,row in enumerate(rows[page*20:(page+1)*20]):
            x=index%4*256;y=index//4*288
            draw.text((x+4,y+3),f"{row['type']} {names[str(row['type'])][:29]}",fill="white")
            for angle,name in enumerate(row["files"]):
                with Image.open(folder/"building-views"/name) as image:
                    sheet.paste(image.convert("RGB").resize((128,128)),(x+angle%2*128,y+24+angle//2*128))
        sheet.save(folder/f"buildings-{page+1}.png")


if __name__ == "__main__":
    report()
