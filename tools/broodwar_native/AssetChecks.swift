import Cocoa
import SceneKit
import Metal
import simd
import CryptoKit

func argument(_ name:String)->String? {
    guard let i=CommandLine.arguments.firstIndex(of:name),i+1<CommandLine.arguments.count else { return nil }
    return CommandLine.arguments[i+1]
}
func pose(_ model:ModelInstance)->[simd_float4x4] { model.allNodes.map{$0.presentation.simdTransform} }
func poseDifference(_ a:[simd_float4x4],_ b:[simd_float4x4])->Float {
    zip(a,b).reduce(Float(0)) { result,pair in
        let d=pair.0-pair.1
        return max(result,max(simd_length(d.columns.0),max(simd_length(d.columns.1),max(simd_length(d.columns.2),simd_length(d.columns.3)))))
    }
}
extension Models3D {
    func proof(device:MTLDevice)throws {
        let out=argument("--proof-dir").map{URL(fileURLWithPath:$0)} ?? runtime.appendingPathComponent("3d-proof")
        try FileManager.default.createDirectory(at:out,withIntermediateDirectories:true)
        let kinds=argument("--model-types").map{$0.split(separator:",").compactMap{Int($0)}} ?? catalog.definitions.keys.sorted()
        var report=[Obj](),failures=[String]()
        for kind in kinds {
            do {
                let a=try make(kind),b=try make(kind),world=SCNScene()
                let idsA=Set(a.allNodes.map{ObjectIdentifier($0)}),idsB=Set(b.allNodes.map{ObjectIdentifier($0)})
                guard idsA.isDisjoint(with:idsB) else { throw SceneError.invalid("Shared skeleton for \(kind)") }
                for model in [a,b] {
                    let own=Set(model.allNodes.map{ObjectIdentifier($0)})
                    for node in model.allNodes {
                        if let skinner=node.skinner,skinner.bones.contains(where:{!own.contains(ObjectIdentifier($0))}) {
                            throw SceneError.invalid("Skeleton escapes instance \(kind)")
                        }
                    }
                }
                world.rootNode.addChildNode(a.root);world.rootNode.addChildNode(b.root)
                b.root.position.x=20
                for (node,transform) in b.rest { node.removeAllAnimations();node.simdTransform=transform }
                world.background.contents=NSColor(calibratedRed:0.1,green:0.13,blue:0.18,alpha:1)
                Self.lights(world)
                let camera=SCNNode();camera.camera=SCNCamera();camera.camera!.fieldOfView=45;Self.exposure(camera.camera!)
                world.rootNode.addChildNode(camera)
                let box=a.root.boundingBox,size=max(box.max.x-box.min.x,max(box.max.y-box.min.y,box.max.z-box.min.z))
                let center=SCNVector3((box.min.x+box.max.x)/2,(box.min.y+box.max.y)/2,(box.min.z+box.max.z)/2)
                camera.position=SCNVector3(center.x+size*1.5,center.y+size*0.6,center.z+size*1.5);camera.look(at:center)
                let renderer=SCNRenderer(device:device,options:nil);renderer.scene=world;renderer.pointOfView=camera;renderer.isPlaying=true
                var clock=CACurrentMediaTime()
                func render(_ time:Double)->NSImage {
                    SCNTransaction.flush()
                    return renderer.snapshot(atTime:clock+time,with:CGSize(width:480,height:480),antialiasingMode:.multisampling4X)
                }
                var animations=[Obj]()
                let actions=Set(Array(ModelInstance.aliases.keys)+Array(a.definition.animations.keys)).sorted()
                for action in actions where a.clip(for:action) != nil {
                    a.playAction(action,force:true,blendDuration:0,loop:action != "death")
                    _=render(0.01);let initial=pose(a),other=pose(b)
                    let duration=a.clips[a.playing]?.first?.1.duration ?? 1
                    var changed:Float=0,leak:Float=0
                    var captures=[String]()
                    for (sample,fraction) in [0.0,0.21,0.47,0.73].enumerated() {
                        let image=render(max(0.02,duration*fraction))
                        if CommandLine.arguments.contains("--animation-proof"),
                           let tiff=image.tiffRepresentation,let bitmap=NSBitmapImageRep(data:tiff),
                           let png=bitmap.representation(using:.png,properties:[:]) {
                            let file="\(kind)-\(action)-\(sample).png"
                            try png.write(to:out.appendingPathComponent(file));captures.append(file)
                        }
                        changed=max(changed,poseDifference(initial,pose(a)))
                        leak=max(leak,poseDifference(other,pose(b)))
                    }
                    guard leak<0.00001 else { throw SceneError.invalid("Cross-instance animation leak \(kind)/\(action)") }
                    if (action=="walk" || (CommandLine.arguments.contains("--animation-proof") && ["attack","death","work"].contains(action))) && changed<0.00001 { throw SceneError.invalid("Action bones did not move: \(kind)/\(action)") }
                    animations.append(["action":action,"clip":a.playing,"changed":changed,"otherInstanceChange":leak,"boundNodes":a.activePlayers.count,"captures":captures,"loop":action != "death"])
                    clock += max(duration,1)+1
                }
                var pickSamples=[Obj]()
                if CommandLine.arguments.contains("--model-proof") {
                    // A fresh renderer prevents a prior clip's scene-time epoch
                    // from leaking into the exported idle reference views.
                    a.root.removeFromParentNode();b.root.removeFromParentNode()
                    let preview=try make(kind)
                    preview.playAction("idle",force:true,blendDuration:0)
                    preview.teamColor(Self.playerColors[0])
                    world.rootNode.addChildNode(preview.root)
                    let previewRenderer=SCNRenderer(device:device,options:nil)
                    previewRenderer.scene=world;previewRenderer.pointOfView=camera;previewRenderer.isPlaying=true
                    let previewTime=CACurrentMediaTime()
                    _=previewRenderer.snapshot(atTime:previewTime,with:CGSize(width:64,height:64),antialiasingMode:.none)
                    for (label,angle) in [("north",Double.pi),("east",Double.pi/2),("south",0),("west",-Double.pi/2)] {
                        camera.position=SCNVector3(center.x+CGFloat(sin(angle))*size*1.85,center.y+size*0.32,center.z+CGFloat(cos(angle))*size*1.85)
                        camera.look(at:center)
                        SCNTransaction.flush()
                        let image=previewRenderer.snapshot(atTime:previewTime+0.2,with:CGSize(width:640,height:640),antialiasingMode:.multisampling4X)
                        if let tiff=image.tiffRepresentation,let bitmap=NSBitmapImageRep(data:tiff),let png=bitmap.representation(using:.png,properties:[:]) {
                            try png.write(to:out.appendingPathComponent("\(kind)-\(label).png"))
                        }
                    }
                    for (player,color) in (Self.playerColors+[[255,255,255]]).enumerated() where preview.definition.teamMask != "none" {
                        preview.teamColor(color);SCNTransaction.flush()
                        let image=previewRenderer.snapshot(atTime:previewTime+0.2,with:CGSize(width:640,height:640),antialiasingMode:.multisampling4X)
                        if let tiff=image.tiffRepresentation,let bitmap=NSBitmapImageRep(data:tiff),let png=bitmap.representation(using:.png,properties:[:]) {
                            try png.write(to:out.appendingPathComponent("\(kind)-player-\(player+1).png"))
                        }
                    }
                    for y in stride(from:64,through:576,by:64) {
                        for x in stride(from:64,through:576,by:64) {
                            let hits=previewRenderer.hitTest(CGPoint(x:x,y:y),options:[.ignoreHiddenNodes:true])
                            let owned=hits.contains { hit in
                                var node:SCNNode?=hit.node
                                while let current=node {if current===preview.root {return true};node=current.parent}
                                return false
                            }
                            if owned {pickSamples.append(["x":x,"y":y])}
                        }
                    }
                    guard !pickSamples.isEmpty else {throw SceneError.invalid("Rendered model cannot be picked: \(kind)")}
                }
                report.append(["type":kind,"file":a.definition.file,"modelRevision":a.revision,"modelSha256":SHA256.hash(data:try Data(contentsOf:catalog.url(a.definition))).map{String(format:"%02x",$0)}.joined(),"passed":true,"independentSkeletons":true,"animations":animations,
                    "pickingChecked":CommandLine.arguments.contains("--model-proof"),"pickSamples":pickSamples,
                    "triangles":a.source.geometries.reduce(0){sum,g in sum+g.elements.reduce(0){$0+($1.primitiveType == .triangles ? $1.primitiveCount:0)}}])
            } catch { failures.append("\(kind): \(error)");report.append(["type":kind,"passed":false,"error":String(describing:error)]) }
        }
        let result:Obj=["passed":failures.isEmpty,"models":report,"errors":failures,"physicalHeadsetVerified":false]
        let data=try JSONSerialization.data(withJSONObject:result,options:[.prettyPrinted,.sortedKeys])
        try data.write(to:out.appendingPathComponent("asset-checks.json"),options:.atomic)
        print(String(data:data,encoding:.utf8)!)
        if !failures.isEmpty { throw SceneError.invalid("Asset checks failed: \(failures)") }
    }
}

// Sample the original engine and render its actual unit transforms, without issuing orders.
func liveModelProof()throws {
    let commander=try Commander(art:Art()),map=try Bridge().request(["op":"scene"])
    let nativeMap=map["map"] as? Obj ?? [:]
    let mapPath=nativeMap["path"] as? String ?? ""
    guard ["Soju-Acceptance-","Soju-Coverage-","Soju-Combat-Test-","Soju-Commander-Test"].contains(where:{mapPath.contains($0)}) else {throw SceneError.invalid("Live proof requires a private Soju fixture map")}
    let prepared=commander.art.manifest["map"] as! Obj
    commander.mapWidth=Int(number(prepared,"width"));commander.mapHeight=Int(number(prepared,"height"));commander.mapTiles=prepared["tiles"] as! [Int]
    commander.world.rootNode.addChildNode(commander.worldSprites.root)
    commander.apply(map,[:])
    for y in stride(from:0,to:commander.mapHeight,by:64) {
        for x in stride(from:0,to:commander.mapWidth,by:64) {
            let width=min(64,commander.mapWidth-x),height=min(64,commander.mapHeight-y)
            let region=try Bridge().request(["op":"terrain","x":x,"y":y,"width":width,"height":height])
            for (i,value) in (region["creep"] as? String ?? "").utf8.enumerated() where value==49 {commander.creepTiles.insert((y+i/width)*commander.mapWidth+x+i%width)}
            for (i,value) in (region["fog"] as? String ?? "").utf8.enumerated() {
                commander.fog[(y+i/width)*commander.mapWidth+x+i%width]=value
            }
        }
    }
    Models3D.lights(commander.world,tileset:prepared["tileset"] as? String ?? "badlands")
    commander.world.background.contents=NSColor(calibratedRed:0.1,green:0.13,blue:0.18,alpha:1)
    commander.camera.camera=SCNCamera();commander.camera.camera!.fieldOfView=50;commander.camera.camera!.zFar=180;Models3D.exposure(commander.camera.camera!)
    commander.world.rootNode.addChildNode(commander.camera)
    let renderer=SCNRenderer(device:commander.art.device,options:nil);renderer.scene=commander.world;renderer.pointOfView=commander.camera;renderer.isPlaying=true
    let out=argument("--proof-dir").map{URL(fileURLWithPath:$0)} ?? runtime.appendingPathComponent("3d-proof/m2/live")
    try FileManager.default.createDirectory(at:out,withIntermediateDirectories:true)
    var seen=Set<Int>(),records=[Obj]()
    for (_,model) in commander.models.sorted(by:{$0.value.kind<$1.value.kind}) where !CommandLine.arguments.contains("--overview-only") && !seen.contains(model.kind) && !model.root.isHidden {
        seen.insert(model.kind)
        let box=model.root.boundingBox,size=max(1,max(box.max.x-box.min.x,max(box.max.y-box.min.y,box.max.z-box.min.z)))
        let center=SCNVector3(model.root.position.x,model.root.position.y+(box.max.y+box.min.y)/2,model.root.position.z)
        model.advance(dt:0,now:CACurrentMediaTime(),selected:true)
        commander.camera.position=SCNVector3(center.x+size*1.3,center.y+size*0.8,center.z+size*1.3)
        commander.camera.look(at:center);Models3D.followLight(commander.world,position:center);commander.updateTerrain()
        SCNTransaction.flush()
        let image=renderer.snapshot(atTime:CACurrentMediaTime(),with:CGSize(width:960,height:720),antialiasingMode:.multisampling4X)
        let filename="live-\(model.kind).png"
        if let tiff=image.tiffRepresentation,let bitmap=NSBitmapImageRep(data:tiff),let png=bitmap.representation(using:.png,properties:[:]) {try png.write(to:out.appendingPathComponent(filename))}
        records.append(["type":model.kind,"position":[model.root.position.x,model.root.position.y,model.root.position.z],"action":model.action,"file":filename])
    }
    commander.tabletop=true;commander.yaw=0
    commander.camera.position=SCNVector3(30,22,42);commander.camera.look(at:SCNVector3(30,0,22))
    Models3D.followLight(commander.world,position:SCNVector3(30,0,22));commander.updateTerrain();SCNTransaction.flush()
    let overview=renderer.snapshot(atTime:CACurrentMediaTime(),with:CGSize(width:1440,height:1000),antialiasingMode:.multisampling4X)
    if let tiff=overview.tiffRepresentation,let bitmap=NSBitmapImageRep(data:tiff),let png=bitmap.representation(using:.png,properties:[:]) {try png.write(to:out.appendingPathComponent("overview.png"))}
    let report:Obj=["frame":number(map,"frame"),"map":nativeMap,"nativeLoneFrames":commander.worldSprites.loneFrames,"nativeDeathFrames":commander.worldSprites.deathFrames,"creepTiles":commander.creepNodes.count,"rendered":records,"missingTypes":Array(Set(commander.units.values.compactMap {u -> Int? in
        let id=u["id"] as? String ?? "";return commander.models[id]==nil ? Int(number(u,"type")):nil
    })).sorted(),"errors":Dictionary(uniqueKeysWithValues:commander.modelLibrary.errors.map{(String($0.key),$0.value)})]
    try JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys]).write(to:out.appendingPathComponent("live-proof.json"))
    print("Live native frame \(Int(number(map,"frame"))): rendered \(records.count) types")
}
