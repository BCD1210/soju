import Cocoa
import SceneKit
import Metal
import simd

// Render installed model placements without per-view registration or resizing.
func modelFidelityProof(device:MTLDevice)throws {
    guard let input=argument("--fidelity-job") else {throw SceneError.invalid("Missing fidelity job")}
    let job=try loadObject(URL(fileURLWithPath:input)),library=try Models3D()
    guard let rows=job["models"] as? [Obj],let output=job["output"] as? String else {throw SceneError.invalid("Invalid fidelity job")}
    let folder=URL(fileURLWithPath:output)
    try FileManager.default.createDirectory(at:folder,withIntermediateDirectories:true)
    var results=[Obj]()
    for row in rows {
        let kind=Int(number(row,"type"))
        do {
            let model=try library.make(kind),scene=SCNScene(),camera=SCNNode()
            scene.background.contents=NSColor.clear;Models3D.lights(scene)
            scene.rootNode.addChildNode(model.root);model.teamColor([255,255,255])
            model.playAction("idle",force:true,blendDuration:0)
            let width=number(row,"width"),height=number(row,"height"),scale=number(row,"scale")*32
            let elevation=32.0*Double.pi/180
            camera.camera=SCNCamera();camera.camera!.usesOrthographicProjection=true
            camera.camera!.orthographicScale=height/scale/2;camera.camera!.zFar=200
            Models3D.exposure(camera.camera!)
            // Native GRP center is the game origin; ANIM canvas dimensions can differ.
            let centerX=(width-number(row,"grpWidth")*number(row,"scale"))/scale/2
            let centerUp = -(height-number(row,"grpHeight")*number(row,"scale"))/scale/2
            camera.position=SCNVector3(centerX,40*sin(elevation)+centerUp*cos(elevation),40*cos(elevation)-centerUp*sin(elevation))
            camera.eulerAngles=SCNVector3(-elevation,0,0);scene.rootNode.addChildNode(camera)
            let renderer=SCNRenderer(device:device,options:nil);renderer.scene=scene;renderer.pointOfView=camera
            renderer.isPlaying=true
            let time=CACurrentMediaTime()
            _=renderer.snapshot(atTime:time,with:CGSize(width:32,height:32),antialiasingMode:.none)
            var files=[String]()
            for direction in 0..<Int(number(row,"directions")) {
                model.root.eulerAngles.y=CGFloat(-Double(direction)*Double.pi/16)
                SCNTransaction.flush()
                let image=renderer.snapshot(atTime:time+0.05,with:CGSize(width:width,height:height),antialiasingMode:.multisampling4X)
                guard let tiff=image.tiffRepresentation,let bitmap=NSBitmapImageRep(data:tiff),let png=bitmap.representation(using:.png,properties:[:]) else {throw SceneError.invalid("Fidelity capture encoding")}
                let filename="\(kind)-\(direction).png";try png.write(to:folder.appendingPathComponent(filename));files.append(filename)
            }
            results.append(["type":kind,"files":files,"model":model.definition.file,"revision":model.revision])
        } catch {results.append(["type":kind,"error":String(describing:error)])}
        print("Fidelity render \(kind)");fflush(stdout)
    }
    try JSONSerialization.data(withJSONObject:["models":results],options:.prettyPrinted).write(to:folder.appendingPathComponent("renders.json"))
}

func buildingGroundingProof()throws {
    guard let input=argument("--grounding-job") else {throw SceneError.invalid("Missing grounding job")}
    let job=try loadObject(URL(fileURLWithPath:input))
    guard let state=job["state"] as? Obj,let units=state["units"] as? [Obj],let output=job["output"] as? String else {throw SceneError.invalid("Invalid grounding job")}
    let folder=URL(fileURLWithPath:output),art=try Art(),ground=try Terrain3D(url:assets.appendingPathComponent("terrain3d.json"))
    try FileManager.default.createDirectory(at:folder,withIntermediateDirectories:true)
    let manifest=art.manifest["map"] as! Obj,tiles=manifest["tiles"] as! [Int],width=Int(number(manifest,"width")),height=Int(number(manifest,"height"))
    let tileCatalog=art.manifest["tiles"] as? [String:Obj] ?? [:]
    var results=[Obj]()
    for unit in units where unit["building"] as? Bool == true {
        try autoreleasepool {
            let kind=Int(number(unit,"type")),x=number(unit,"x")/32,z=number(unit,"y")/32
            let sprites=WorldSprites(streaming:false),billboard=UnitBillboard(id:"grounding-\(kind)")
            let scene=SCNScene(),camera=SCNNode(),renderer=SCNRenderer(device:art.device,options:nil)
            scene.background.contents=NSColor(calibratedWhite:0.08,alpha:1);Models3D.lights(scene)
            billboard.apply(unit,ground:ground,art:sprites);scene.rootNode.addChildNode(billboard.root)
            let entry=sprites.images[String(Int(number(unit["image"] as? Obj ?? [:],"id")))] ?? [:]
            let radius=min(24,max(5,Int(ceil(max(number(entry,"grpWidth"),number(entry,"grpHeight"))/32))+3))
            for y in max(0,Int(z)-radius)..<min(height,Int(z)+radius+1) {for tx in max(0,Int(x)-radius)..<min(width,Int(x)+radius+1) {
                guard let file=tileCatalog[String(tiles[y*width+tx])]?["file"] as? String else {continue}
                let material=try art.terrainMaterial([String](repeating:file,count:9))
                let node=SCNNode(geometry:ground.geometry(tileX:tx,tileY:y,material:material))
                node.position=SCNVector3(tx,0,y);scene.rootNode.addChildNode(node)
            }}
            camera.camera=SCNCamera();camera.camera!.fieldOfView=55;camera.camera!.zNear=0.03;camera.camera!.zFar=100
            Models3D.exposure(camera.camera!);scene.rootNode.addChildNode(camera)
            renderer.scene=scene;renderer.pointOfView=camera
            var captures=[String]()
            let base=billboard.root.simdPosition
            for angle in 0..<4 {
                let yaw=Float(angle)*Float.pi/2
                camera.simdPosition=base+SIMD3<Float>(sin(yaw)*8,5,cos(yaw)*8)
                camera.look(at:SCNVector3(base+SIMD3<Float>(0,0.7,0)))
                billboard.lastEye=nil
                billboard.updateView(eye:camera.simdPosition,orientation:camera.simdOrientation,perspective:true,art:sprites)
                let bounds=billboard.root.boundingBox
                let lo=SIMD3<Float>(Float(bounds.min.x),Float(bounds.min.y),Float(bounds.min.z))
                let hi=SIMD3<Float>(Float(bounds.max.x),Float(bounds.max.y),Float(bounds.max.z))
                let target=base+(lo+hi)*0.5,distance=max(8,simd_length(hi-lo)*1.6)
                camera.simdPosition=target+SIMD3<Float>(sin(yaw)*distance,distance*0.625,cos(yaw)*distance)
                camera.look(at:SCNVector3(target))
                billboard.lastEye=nil
                billboard.updateView(eye:camera.simdPosition,orientation:camera.simdOrientation,perspective:true,art:sprites)
                SCNTransaction.flush()
                let image=renderer.snapshot(atTime:CACurrentMediaTime(),with:CGSize(width:512,height:512),antialiasingMode:.multisampling4X)
                guard let tiff=image.tiffRepresentation,let bitmap=NSBitmapImageRep(data:tiff),let png=bitmap.representation(using:.png,properties:[:]) else {throw SceneError.invalid("Grounding PNG encoding")}
                let name="\(kind)-\(angle).png";try png.write(to:folder.appendingPathComponent(name));captures.append(name)
            }
            results.append(["type":kind,"nativeFrame":number(state,"frame"),"files":captures,"nativeImages":billboard.nativeLayers.count,
                            "contactConvention":"Lowest opaque relief vertex; complete footprint contact is not certified"])
            print("Grounding views \(kind)");fflush(stdout)
        }
    }
    let report:Obj=["buildings":results,"viewsPerBuilding":4,"renderer":"SceneKit geometry diagnostic using native completed building image chains; not XR performance evidence",
                    "completeGroundingVerified":false]
    try JSONSerialization.data(withJSONObject:report,options:.prettyPrinted).write(to:folder.appendingPathComponent("grounding.json"))
}
