import Cocoa
import SceneKit

// ANIM frames are selected by the native bullet, including its impact phase.
// No weapon damage or projectile lifetime is simulated by the renderer.
final class EffectSprites {
    let folder=runtime.appendingPathComponent("effect-sprites")
    let images:[String:Obj],weapons:[String:Int]
    var textures=[String:NSImage]()
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
        let texture:NSImage
        if let existing=textures[key] {texture=existing}
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
        node.geometry?.firstMaterial?.diffuse.contents=texture
        node.geometry?.firstMaterial?.emission.contents=texture
        node.geometry?.firstMaterial?.emission.intensity=0.35
        node.isHidden=Int(number(native,"flags"))&0x40 != 0
        renderedFrames+=1
        return true
    }
}
