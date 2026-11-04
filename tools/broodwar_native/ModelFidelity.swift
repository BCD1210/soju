import Cocoa
import SceneKit
import Metal

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
