import Cocoa
import SceneKit
import Metal
import simd

// These gates inspect Metal readbacks. Contact probes are attached to the actual
// opaque base vertices; the reference probes sit on the native terrain samples.
private final class VisualReadback {
    let batches:MetalBatches,texture:MTLTexture
    var revision=0
    init(device:MTLDevice)throws {
        batches=try MetalBatches(device:device)
        let d=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.bgra8Unorm_srgb,width:512,height:512,mipmapped:false)
        d.storageMode = .shared;d.usage=[.renderTarget,.shaderRead]
        guard let t=device.makeTexture(descriptor:d) else {throw SceneError.invalid("Visual gate target")};texture=t
    }
    func draw(terrain:[Int:SCNNode],objects:[UnitBillboard],overlays:[SCNNode]=[],camera:SCNNode,width:Int)throws->[UInt8] {
        revision+=1
        try batches.prepare(MetalBatches.Input(sceneRevision:revision,terrainRevision:revision,terrainFogRevision:revision,mapWidth:width,terrain:terrain,billboards:objects,overlays:overlays,objectCount:objects.count))
        guard let command=batches.queue.makeCommandBuffer() else {throw SceneError.invalid("Visual gate command")}
        var right=camera.simdWorldTransform;right.columns.3.x+=0.064
        try batches.render(command:command,output:texture,camera:camera,eye:camera.simdWorldPosition,rightTransform:right,rightProjection:simd_float4x4(camera.camera!.projectionTransform))
        batches.copyEye(command:command,to:texture,eye:0);command.commit();command.waitUntilCompleted()
        guard command.status == .completed else {throw SceneError.invalid("Visual gate GPU completion")}
        var bytes=[UInt8](repeating:0,count:512*512*4)
        texture.getBytes(&bytes,bytesPerRow:2048,from:MTLRegionMake2D(0,0,512,512),mipmapLevel:0)
        return bytes
    }
    func save(_ bytes:[UInt8],_ url:URL)throws {
        let bitmap=NSBitmapImageRep(bitmapDataPlanes:nil,pixelsWide:512,pixelsHigh:512,bitsPerSample:8,samplesPerPixel:4,hasAlpha:true,isPlanar:false,colorSpaceName:.deviceRGB,bytesPerRow:2048,bitsPerPixel:32)!
        for i in stride(from:0,to:bytes.count,by:4) {
            bitmap.bitmapData![i]=bytes[i+2];bitmap.bitmapData![i+1]=bytes[i+1];bitmap.bitmapData![i+2]=bytes[i];bitmap.bitmapData![i+3]=bytes[i+3]
        }
        guard let png=bitmap.representation(using:.png,properties:[:]) else {throw SceneError.invalid("Visual gate PNG")}
        try png.write(to:url)
    }
}
private func probeMask(_ bytes:[UInt8])->Set<Int> {
    Set((0..<512*512).filter{bytes[$0*4]>160 && bytes[$0*4+2]>160 && bytes[$0*4+1]<100})
}
private func terrainMask(_ bytes:[UInt8])->Set<Int> {
    Set((0..<512*512).filter{min(bytes[$0*4],bytes[$0*4+1],bytes[$0*4+2])>160})
}
private func overlap(_ a:Set<Int>,_ b:Set<Int>,radius:Int)->Double {
    guard !a.isEmpty && !b.isEmpty else {return 0}
    func matched(_ source:Set<Int>,_ target:Set<Int>)->Int {
        source.filter {p in
            for y in max(0,p/512-radius)...min(511,p/512+radius) {
                for x in max(0,p%512-radius)...min(511,p%512+radius) where target.contains(y*512+x) {return true}
            }
            return false
        }.count
    }
    return Double(matched(a,b)+matched(b,a))/Double(a.count+b.count)
}
private func marker(_ point:SIMD3<Float>)->SCNNode {
    let node=SCNNode(geometry:SCNSphere(radius:0.04)),material=SCNMaterial()
    material.lightingModel = .constant;material.diffuse.contents=NSColor(red:1,green:0,blue:1,alpha:1)
    material.readsFromDepthBuffer=true;material.writesToDepthBuffer=true
    node.geometry!.materials=[material];node.simdPosition=point+SIMD3<Float>(0,0.03,0)
    return node
}
private func contactPoints(_ object:UnitBillboard)->[SIMD3<Float>] {
    var points=[SIMD3<Float>]()
    for (index,mesh) in object.contactMeshes {
        guard let node=object.layers[index],!node.isHidden,number(object.nativeLayers[index] ?? [:],"drawfunc") != 10 else {continue}
        for i in mesh.baseSamples {
            let p=node.simdWorldTransform*SIMD4<Float>(SIMD3<Float>(mesh.vertices[i]),1)
            points.append(SIMD3<Float>(p.x,p.y,p.z))
        }
    }
    return points
}

func renderedVisualGates()throws {
    guard let input=argument("--rendered-gates") else {throw SceneError.invalid("Missing visual gate job")}
    let job=try loadObject(URL(fileURLWithPath:input))
    guard let state=job["state"] as? Obj,let units=state["units"] as? [Obj],let output=job["output"] as? String else {throw SceneError.invalid("Invalid visual gate job")}
    let assetFolder=(job["assets"] as? String).map{URL(fileURLWithPath:$0)} ?? assets
    let folder=URL(fileURLWithPath:output),art=try Art(folder:assetFolder)
    let ground=try Terrain3D(url:(job["terrain"] as? String).map{URL(fileURLWithPath:$0)} ?? assetFolder.appendingPathComponent("terrain3d.json"))
    let map=art.manifest["map"] as! Obj,width=Int(number(map,"width")),height=Int(number(map,"height")),tiles=map["tiles"] as! [Int]
    guard ground.width==width*4 && ground.height==height*4 else {throw SceneError.invalid("Visual terrain and texture map dimensions differ")}
    try FileManager.default.createDirectory(at:folder,withIntermediateDirectories:true)
    var results=[Obj]()
    for unit in units where unit["building"] as? Bool == true {
        try autoreleasepool {
            let kind=Int(number(unit,"type")),x=number(unit,"x")/32,z=number(unit,"y")/32
            let sprites=WorldSprites(streaming:false),object=UnitBillboard(id:"visual-\(kind)"),scene=SCNScene(),camera=SCNNode()
            object.apply(unit,ground:ground,art:sprites);scene.rootNode.addChildNode(object.root)
            camera.camera=SCNCamera();camera.camera!.fieldOfView=55;camera.camera!.zNear=0.03;camera.camera!.zFar=150
            scene.rootNode.addChildNode(camera);Models3D.exposure(camera.camera!)
            let radius=8,x0=max(0,Int(x)-radius),x1=min(width,Int(x)+radius+1),z0=max(0,Int(z)-radius),z1=min(height,Int(z)+radius+1)
            var terrain=[Int:SCNNode](),materials=[Int:[SCNMaterial]](),transforms=[Int:simd_float4x4]()
            for tz in z0..<z1 {for tx in x0..<x1 {
                guard let file=art.tileCatalog[String(tiles[tz*width+tx])]?["file"] as? String else {throw SceneError.invalid("Missing visual gate terrain tile")}
                let material=try art.terrainMaterial([String](repeating:file,count:9)),node=SCNNode(geometry:ground.geometry(tileX:tx,tileY:tz,material:material))
                node.position=SCNVector3(tx,0,tz);scene.rootNode.addChildNode(node)
                let id=tz*width+tx;terrain[id]=node;materials[id]=node.geometry!.materials;transforms[id]=node.simdTransform
            }}
            let heights=(z0...z1).flatMap{tz in (x0...x1).map{tx in ground.profile(Double(tx),Double(tz))}}
            guard (heights.max() ?? 0)-(heights.min() ?? 0)>0.5 else {throw SceneError.invalid("Terrain visual gate requires a nonflat cliff or slope fixture")}
            let readback=try VisualReadback(device:art.device),white=SCNMaterial()
            white.lightingModel = .constant
            white.diffuse.contents=try readback.batches.texture(NSColor(red:1,green:1,blue:1,alpha:1),fallback:readback.batches.white)
            white.multiply.contents=NSColor(red:1,green:1,blue:1,alpha:1)
            let top=SCNNode();top.camera=SCNCamera();top.camera!.usesOrthographicProjection=true;top.camera!.orthographicScale=Double(max(x1-x0,z1-z0))+2
            top.camera!.zNear=0.03;top.camera!.zFar=200;scene.rootNode.addChildNode(top)
            let center=SIMD3<Float>(Float(x0+x1)/2,0,Float(z0+z1)/2)
            top.simdPosition=center+SIMD3<Float>(0,80,0);top.look(at:SCNVector3(center),up:SCNVector3(0,0,-1),localFront:SCNVector3(0,0,-1))
            let flat=SCNNode(geometry:SCNPlane(width:CGFloat(x1-x0),height:CGFloat(z1-z0)))
            flat.geometry!.materials=[white];flat.eulerAngles.x = -.pi/2;flat.simdPosition=center
            let referenceTerrain=terrainMask(try readback.draw(terrain:[:],objects:[],overlays:[flat],camera:top,width:width))
            for angle in 0..<4 {
                let base=object.root.simdPosition,yaw=Float(angle)*Float.pi/2
                camera.simdPosition=base+SIMD3<Float>(sin(yaw)*9,6,cos(yaw)*9);camera.look(at:SCNVector3(base+SIMD3<Float>(0,1,0)))
                object.lastEye=nil;object.updateView(eye:camera.simdPosition,orientation:camera.simdOrientation,perspective:true,art:sprites)
                let contacts=contactPoints(object)
                let expected=contacts.map{marker(SIMD3<Float>($0.x,Float(ground.elevation(Double($0.x),Double($0.z))),$0.z))}
                let reference=try readback.draw(terrain:terrain,objects:[],overlays:expected,camera:camera,width:width)
                let referenceMask=probeMask(reference)
                try readback.save(reference,folder.appendingPathComponent("\(kind)-\(angle)-expected-contact.png"))
                for node in terrain.values {node.geometry!.materials=[white]}
                let referenceProfile=terrainMask(try readback.draw(terrain:terrain,objects:[],camera:camera,width:width))
                for (id,node) in terrain {node.geometry!.materials=materials[id]!}
                for variant in ["normal","missing","floating","clipped","folded-terrain","flattened-terrain"] {
                    object.root.simdPosition=base
                    if variant=="floating" {object.root.position.y+=0.75}
                    if variant=="clipped" {object.root.position.y-=0.75}
                    object.poseRevision &+= 1
                    for (id,node) in terrain {
                        node.simdTransform=transforms[id]!
                        if variant=="folded-terrain" {
                            var shear=matrix_identity_float4x4;shear.columns.1.z=Float(Terrain3D.reliefDepth)
                            node.simdTransform=node.simdTransform*shear
                        }
                        if variant=="flattened-terrain" {node.simdScale.y=0}
                    }
                    let objects=variant=="missing" ? []:[object]
                    let points=objects.flatMap{contactPoints($0)},markers=points.map{marker($0)}
                    let actual=try readback.draw(terrain:terrain,objects:[],overlays:markers,camera:camera,width:width)
                    let actualMask=probeMask(actual),contactScore=overlap(actualMask,referenceMask,radius:2)
                    let body=try readback.draw(terrain:[:],objects:objects,camera:camera,width:width)
                    let bodyPixels=(0..<512*512).filter{max(body[$0*4],body[$0*4+1],body[$0*4+2])>60}.count
                    let color=try readback.draw(terrain:terrain,objects:objects,camera:camera,width:width)
                    let stem="\(kind)-\(angle)-\(variant)"
                    try readback.save(color,folder.appendingPathComponent(stem+".png"))
                    try readback.save(actual,folder.appendingPathComponent(stem+"-contact.png"))
                    for node in terrain.values {node.geometry!.materials=[white]}
                    let terrainPixels=try readback.draw(terrain:terrain,objects:[],camera:top,width:width)
                    let coverage=overlap(terrainMask(terrainPixels),referenceTerrain,radius:1)
                    let profile=try readback.draw(terrain:terrain,objects:[],camera:camera,width:width)
                    let profileScore=overlap(terrainMask(profile),referenceProfile,radius:1)
                    try readback.save(terrainPixels,folder.appendingPathComponent(stem+"-terrain.png"))
                    for (id,node) in terrain {node.geometry!.materials=materials[id]!}
                    let passed=bodyPixels>256 && referenceMask.count>=12 && actualMask.count>=12 && contactScore>=0.95 && coverage>=0.995 && profileScore>=0.999
                    results.append(["type":kind,"view":angle,"variant":variant,"accepted":passed,"bodyPixels":bodyPixels,
                        "referenceContactPixels":referenceMask.count,"actualContactPixels":actualMask.count,"contactOverlap":contactScore,"terrainCoverage":coverage,"terrainProfileOverlap":profileScore,"file":stem+".png"])
                    print("Visual gate \(kind)/\(angle)/\(variant): \(passed) contact=\(contactScore) terrain=\(coverage)");fflush(stdout)
                }
                object.root.simdPosition=base
                for (id,node) in terrain {node.simdTransform=transforms[id]!}
            }
        }
    }
    let positives=results.filter{$0["variant"] as? String=="normal"},negatives=results.filter{$0["variant"] as? String != "normal"}
    let passed = !positives.isEmpty && positives.allSatisfy{$0["accepted"] as? Bool==true} && negatives.allSatisfy{$0["accepted"] as? Bool==false}
    let report:Obj=["passed":passed,"renderer":"Metal stereo pipeline readback","positiveViews":positives.count,"negativeControls":negatives.count,
        "criteria":["bodyPixels":256,"minimumContactPixels":12,"minimumContactOverlap":0.95,"minimumTerrainCoverage":0.995,"minimumTerrainProfileOverlap":0.999],"results":results]
    try JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys]).write(to:folder.appendingPathComponent("acceptance.json"))
    if !passed {throw SceneError.invalid("Rendered visual gate rejected a required positive or accepted a negative control")}
}
