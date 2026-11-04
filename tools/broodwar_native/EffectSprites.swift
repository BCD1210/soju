import Cocoa
import SceneKit
import MetalKit
import os.signpost

// ANIM frames are selected by the native bullet, including its impact phase.
// No weapon damage or projectile lifetime is simulated by the renderer.
final class EffectSprites {
    let folder=runtime.appendingPathComponent("effect-sprites")
    let images:[String:Obj],weapons:[String:Int]
    var textures=[String:NSImage]()
    private let queue=DispatchQueue(label:"soju.bw.effect-stream",qos:.userInitiated)
    private let lock=NSLock()
    private let resident=ResidentCache<MTLTexture>(limit:32*1024*1024)
    private var pending=Set<String>(),failed=Set<String>()
    private lazy var loader=MTKTextureLoader(device:MTLCreateSystemDefaultDevice()!)
    private let streaming=useMetalBatches && !CommandLine.arguments.contains("--runtime-check") && !CommandLine.arguments.contains("--effect-proof")
    private func streamed(_ key:String)->MTLTexture? {
        lock.lock()
        if let texture=resident.get(key) {lock.unlock();return texture}
        guard !pending.contains(key),!failed.contains(key),pending.count<64 else {lock.unlock();return nil}
        pending.insert(key);lock.unlock()
        queue.async { [self] in
            autoreleasepool {
                os_signpost(.begin,log:XRFrameMetrics.log,name:"Effect decode")
                defer {os_signpost(.end,log:XRFrameMetrics.log,name:"Effect decode")}
                var result:MTLTexture?
                do {
                    guard let image=NSImage(contentsOf:folder.appendingPathComponent(key)),
                        let cg=image.cgImage(forProposedRect:nil,context:nil,hints:nil) else {throw SceneError.invalid("Missing effect frame")}
                    result=try TextureResidency.shared.load(loader,MetalBatches.colorImage(cg),options:[.SRGB:true,.generateMipmaps:true,.origin:MTKTextureLoader.Origin.topLeft])
                } catch {fputs("Effect stream \(key): \(error)\n",stderr)}
                lock.lock();pending.remove(key)
                if let texture=result {resident.insert(texture,key:key,bytes:MetalBatches.textureBytes(texture))}
                else {failed.insert(key)}
                lock.unlock()
            }
        }
        return nil
    }
    var renderedFrames=0,missing=Set<Int>()
    init() {
        let catalog=(try? loadObject(folder.appendingPathComponent("catalog.json"))) ?? [:]
        images=catalog["images"] as? [String:Obj] ?? [:]
        weapons=catalog["weapons"] as? [String:Int] ?? [:]
    }
    func apply(_ bullet:Obj,to node:SCNNode)->Bool {
        let native=bullet["image"] as? Obj ?? [:]
        let id=(native["id"] as? Int) ?? weapons[String(Int(number(bullet,"weapon")))] ?? -1
        guard let entry=images[String(id)] else {missing.insert(id);return false}
        let frame=Int(number(native,"frame"))
        guard frame>=0,frame<Int(number(entry,"frames")) else {return false}
        let key="\(id)/\(frame).png"
        let texture:Any
        if streaming {
            guard let prepared=streamed(key) else {
                if node.name?.hasPrefix("sprite-") != true {node.isHidden=true}
                return true
            }
            texture=prepared
        }
        else if let existing=textures[key] {texture=existing}
        else {
            guard let loaded=NSImage(contentsOf:folder.appendingPathComponent(key)) else {return false}
            texture=loaded
            if textures.count>512 {textures.removeAll()}
            textures[key]=loaded
        }
        if node.name != "sprite-\(id)" {
            let scale=number(entry,"scale")*32
            let plane=SCNPlane(width:number(entry,"width")/scale,height:number(entry,"height")/scale)
            let material=SCNMaterial();material.lightingModel = .constant
            material.isDoubleSided=true;material.blendMode = .alpha
            material.writesToDepthBuffer=false;material.readsFromDepthBuffer=true
            plane.materials=[material];node.geometry=plane
            let billboard=SCNBillboardConstraint();billboard.freeAxes = .all
            node.constraints=[billboard];node.castsShadow=false;node.categoryBitMask=2
            node.name="sprite-\(id)"
        }
        if (node.geometry?.firstMaterial?.diffuse.contents as AnyObject?) !== (texture as AnyObject) {
            node.geometry?.firstMaterial?.diffuse.contents=texture
            node.geometry?.firstMaterial?.emission.contents=texture
            node.geometry?.firstMaterial?.emission.intensity=0.35
        }
        node.isHidden=Int(number(native,"flags"))&0x40 != 0
        renderedFrames+=1
        return true
    }
}

func effectSpriteProof()throws {
    guard let device=MTLCreateSystemDefaultDevice() else {throw SceneError.invalid("Metal unavailable")}
    let sprites=EffectSprites(),world=SCNScene(),camera=SCNNode(),node=SCNNode()
    let folder=runtime.appendingPathComponent("3d-proof/m3/effect-proof")
    try FileManager.default.createDirectory(at:folder,withIntermediateDirectories:true)
    world.rootNode.addChildNode(node);world.rootNode.addChildNode(camera)
    world.background.contents=NSColor(calibratedWhite:0.06,alpha:1)
    camera.camera=SCNCamera();camera.camera!.usesOrthographicProjection=true;camera.camera!.orthographicScale=3
    camera.position=SCNVector3(0,0,4);Models3D.exposure(camera.camera!)
    let renderer=SCNRenderer(device:device,options:nil);renderer.scene=world;renderer.pointOfView=camera
    var frames=[Obj]()
    for id in sprites.images.keys.sorted() {
        guard let entry=sprites.images[id],let imageID=Int(id) else {continue}
        let count=Int(number(entry,"frames"))
        for frame in Set([0,count/3,count*2/3,count-1]).sorted() where frame>=0 {
            let bullet:Obj=["weapon":0,"image":["id":imageID,"frame":frame,"flags":0]]
            guard sprites.apply(bullet,to:node) else {throw SceneError.invalid("Effect frame \(id)/\(frame)")}
            SCNTransaction.flush()
            let image=renderer.snapshot(atTime:0,with:CGSize(width:512,height:512),antialiasingMode:.multisampling4X)
            guard let tiff=image.tiffRepresentation,let bitmap=NSBitmapImageRep(data:tiff),
                let png=bitmap.representation(using:.png,properties:[:]) else {throw SceneError.invalid("Effect capture")}
            let name="\(id)-\(frame).png";try png.write(to:folder.appendingPathComponent(name))
            frames.append(["image":imageID,"frame":frame,"capture":name])
        }
    }
    try JSONSerialization.data(withJSONObject:["source":"owned weapon ANIM frames through desktop renderer; synthetic timing","frames":frames],options:.prettyPrinted).write(to:folder.appendingPathComponent("proof.json"))
    print("Effect sprite captures: \(frames.count)")
}
