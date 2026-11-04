import Cocoa
import SceneKit
import simd

// All trajectories and hits originate in native state. These nodes never
// simulate damage, choose targets or invent a projectile for a hitscan weapon.
final class CombatEffects {
    let root=SCNNode(),library:Models3D
    let sprites=EffectSprites()
    var projectiles=[String:SCNNode](),meshes=[String:ModelInstance]()
    var transients=[(SCNNode,Double,Double)]()
    var counts=[String:Int]()
    init()throws {
        let folder=runtime.appendingPathComponent("effects-3d")
        try FileManager.default.createDirectory(at:folder,withIntermediateDirectories:true)
        library=try Models3D(folder:folder,includeDefaults:false)
        root.name="combat-effects";root.categoryBitMask=2
    }
    func clear() {
        for n in root.childNodes {n.removeFromParentNode()}
        projectiles.removeAll();meshes.removeAll();transients.removeAll();counts.removeAll()
        sprites.renderedFrames=0;sprites.missing.removeAll()
    }
    func glow(_ color:NSColor,radius:CGFloat)->SCNNode {
        let geometry=SCNSphere(radius:radius);geometry.segmentCount=8
        let material=SCNMaterial();material.lightingModel = .constant
        material.diffuse.contents=color;material.emission.contents=color
        geometry.materials=[material]
        let node=SCNNode(geometry:geometry);node.categoryBitMask=2;return node
    }
    func pulse(position:SCNVector3,shield:Bool,frame:Double) {
        let node=glow(shield ? .cyan:.orange,radius:shield ? 0.42:0.10)
        node.position=position;root.addChildNode(node)
        transients.append((node,frame,shield ? 6:4))
        while transients.count>256 {transients.removeFirst().0.removeFromParentNode()}
    }
    func apply(bullets:[Obj],events:[Obj],units:[String:Obj],frame:Double,ground:Terrain3D) {
        library.catalog.poll()
        var live=Set<String>()
        for b in bullets {
            let id=String(Int(number(b,"id"))),kind=Int(number(b,"weapon"));live.insert(id)
            let source=units[b["source"] as? String ?? ""] ?? [:]
            let flight=source["airborne"] as? Bool == true ? 1.5:0.35
            let x=number(b,"x")/32,z=number(b,"y")/32
            let position=SCNVector3(x,ground.elevation(x,z)+flight,z)
            if let old=meshes[id],old.kind != kind || old.revision != library.catalog.signatures[kind] {
                projectiles.removeValue(forKey:id)?.removeFromParentNode();meshes.removeValue(forKey:id)
            }
            if meshes[id]==nil,library.catalog.definitions[kind] != nil,let model=try? library.make(kind) {
                projectiles.removeValue(forKey:id)?.removeFromParentNode()
                model.selection.isHidden=true;projectiles[id]=model.root;meshes[id]=model;root.addChildNode(model.root)
            }
            if projectiles[id]==nil {let node=glow(.yellow,radius:0.065);projectiles[id]=node;root.addChildNode(node)}
            if meshes[id]==nil,let node=projectiles[id],!sprites.apply(b,to:node) {
                node.geometry=glow(.yellow,radius:0.065).geometry
                node.constraints=nil;node.name=nil;node.isHidden=false
            }
            projectiles[id]?.position=position
            projectiles[id]?.eulerAngles.y=CGFloat(-number(b,"facing")*2*Double.pi/256)
        }
        for (id,n) in projectiles where !live.contains(id) {n.removeFromParentNode();projectiles.removeValue(forKey:id);meshes.removeValue(forKey:id)}
        for e in events {
            let kind=e["kind"] as? String ?? "";counts[kind,default:0]+=1
            let x=number(e,"x")/32,z=number(e,"y")/32
            let y=ground.elevation(x,z)+(e["airborne"] as? Bool == true ? 1.5:0.35)
            if kind=="damage" {pulse(position:SCNVector3(x,y,z),shield:number(e,"shieldLoss")>0,frame:frame)}
            if kind=="attack" {
                let n=glow(.yellow,radius:0.08);n.position=SCNVector3(x,y,z);root.addChildNode(n)
                transients.append((n,frame,2))
            }
        }
        while transients.count>256 {transients.removeFirst().0.removeFromParentNode()}
        advance(frame:frame)
    }
    func advance(frame:Double) {
        transients=transients.filter { node,start,duration in
            let progress=max(0,(frame-start)/duration)
            if progress>=1 {node.removeFromParentNode();return false}
            node.opacity=CGFloat(1-progress);node.scale=SCNVector3(1+progress,1+progress,1+progress);return true
        }
    }
}
