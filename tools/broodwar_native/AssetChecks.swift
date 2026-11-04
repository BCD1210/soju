import Cocoa
import SceneKit
import Metal
import simd

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
                let camera=SCNNode();camera.camera=SCNCamera();camera.camera!.fieldOfView=45
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
                    a.playAction(action,force:true,blendDuration:0)
                    _=render(0.01);let initial=pose(a),other=pose(b)
                    let duration=a.clips[a.playing]?.first?.1.duration ?? 1
                    var changed:Float=0,leak:Float=0
                    for fraction in [0.21,0.47,0.73] {
                        _=render(max(0.02,duration*fraction));changed=max(changed,poseDifference(initial,pose(a)))
                        leak=max(leak,poseDifference(other,pose(b)))
                    }
                    guard leak<0.00001 else { throw SceneError.invalid("Cross-instance animation leak \(kind)/\(action)") }
                    if action=="walk" && changed<0.00001 { throw SceneError.invalid("Walk bones did not move: \(kind)") }
                    animations.append(["action":action,"clip":a.playing,"changed":changed,"otherInstanceChange":leak,"boundNodes":a.activePlayers.count])
                    clock += max(duration,1)+1
                }
                if CommandLine.arguments.contains("--model-proof") {
                    // A fresh renderer prevents a prior clip's scene-time epoch
                    // from leaking into the exported idle reference views.
                    a.root.removeFromParentNode();b.root.removeFromParentNode()
                    let preview=try make(kind)
                    preview.playAction("idle",force:true,blendDuration:0)
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
                }
                report.append(["type":kind,"file":a.definition.file,"passed":true,"independentSkeletons":true,"animations":animations,
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
