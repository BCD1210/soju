import Cocoa
import SceneKit
import CoreImage

// The original image frame already includes the 17-direction lookup and the
// mirrored half of the turn. Keep its animation frameset and image-chain order.
final class UnitBillboard {
    let root=SCNNode()
    var layers=[Int:SCNNode](),evidence=[Obj]()
    init(id:String) {root.name=id}
    func apply(_ unit:Obj,ground:Terrain3D,art:WorldSprites,color:[Int]=[255,255,255]) {
        let x=number(unit,"x")/32,z=number(unit,"y")/32
        root.position=Terrain3D.worldPosition(x,ground.elevation(x,z)+(unit["airborne"] as? Bool == true ? 1.5:0),z)
        root.isHidden=unit["renderHidden"] as? Bool == true || unit["transported"] as? Bool == true
        root.opacity=unit["cloaked"] as? Bool == true ? 0.45:1
        let main=unit["image"] as? Obj ?? [:]
        let chain=(unit["visual"] as? Obj)?["images"] as? [Obj] ?? [main]
        var keep=Set<Int>();evidence=[]
        for (index,native) in chain.enumerated() {
            guard Int(number(native,"flags"))&0x40==0,let id=native["id"] as? Int,
                let entry=art.images[String(id)],let records=entry["frames"] as? [Obj] else {continue}
            let frame=Int(number(native,"frame"))
            guard records.indices.contains(frame),let image=art.texture(id,frame:frame,color:color) else {continue}
            let scale=number(entry,"scale")*32
            guard scale>0 else {continue}
            keep.insert(index)
            let shadow=Int(number(native,"drawfunc"))==10,node=layers[index] ?? SCNNode()
            let plane=SCNPlane(width:number(entry,"grpWidth")/32,height:number(entry,"grpHeight")/32)
            let material=SCNMaterial();material.lightingModel = .constant;material.isDoubleSided=true
            material.diffuse.contents=image;material.diffuse.wrapS = .clamp;material.diffuse.wrapT = .clamp
            material.readsFromDepthBuffer=false;material.writesToDepthBuffer=false;plane.materials=[material];node.geometry=plane
            node.castsShadow=false;node.renderingOrder=1000+Int(number(unit,"y"))*32+chain.count-index
            if shadow {
                node.constraints=nil;node.eulerAngles.x = -.pi/2;material.multiply.contents=NSColor.black;node.opacity=0.5
            } else {
                node.eulerAngles = SCNVector3Zero;node.opacity=1
                let constraint=SCNBillboardConstraint();constraint.freeAxes = .all;node.constraints=[constraint]
            }
            node.position=SCNVector3(number(native,"offsetX")/32,shadow ? 0.025:-number(native,"offsetY")/32,0)
            node.scale.x=Int(number(native,"flags"))&2 != 0 ? -1:1
            if layers[index]==nil {layers[index]=node;root.addChildNode(node)}
            evidence.append(["image":id,"frame":frame,"frameset":number(native,"frameset"),
                "direction":number(native,"direction"),"mirrored":node.scale.x<0])
        }
        for (index,node) in layers where !keep.contains(index) {node.removeFromParentNode();layers.removeValue(forKey:index)}
    }
}

// Native image chains drive both independent map sprites and fallback death frames.
final class WorldSprites {
    let root=SCNNode(),folder=runtime.appendingPathComponent("world-sprites")
    let images:[String:Obj],creep:[String:Obj]
    let colorContext=CIContext()
    var nodes=[String:SCNNode](),frames=[String:NSImage](),atlases=[String:NSImage]()
    var missing=Set<Int>(),deathFrames=0,loneFrames=0
    var creepMaterials=[String:SCNMaterial]()
    init() {
        let catalog=(try? loadObject(folder.appendingPathComponent("catalog.json"))) ?? [:]
        images=catalog["images"] as? [String:Obj] ?? [:];creep=catalog["creep"] as? [String:Obj] ?? [:]
        root.name="native-world-sprites"
    }
    func clear() {
        for node in nodes.values {node.removeFromParentNode()}
        nodes.removeAll();deathFrames=0;loneFrames=0;missing.removeAll()
    }
    func texture(_ id:Int,frame:Int,color:[Int]=[255,255,255])->NSImage? {
        let key="\(id)/\(frame)/\(color)"
        if let image=frames[key] {return image}
        guard let entry=images[String(id)],let records=entry["frames"] as? [Obj],records.indices.contains(frame),let file=entry["file"] as? String else {missing.insert(id);return nil}
        let atlas:NSImage
        if let cached=atlases[file] {atlas=cached}
        else {guard let loaded=NSImage(contentsOf:folder.appendingPathComponent(file)) else {return nil};atlas=loaded;if atlases.count>32 {atlases.removeAll()};atlases[file]=loaded}
        let f=records[frame],width=number(entry,"grpWidth")*number(entry,"scale"),height=number(entry,"grpHeight")*number(entry,"scale")
        let image=NSImage(size:NSSize(width:width,height:height));image.lockFocus()
        atlas.draw(in:NSRect(x:number(f,"ox"),y:height-number(f,"oy")-number(f,"h"),width:number(f,"w"),height:number(f,"h")),
                   from:NSRect(x:number(f,"x"),y:number(entry,"atlasHeight")-number(f,"y")-number(f,"h"),width:number(f,"w"),height:number(f,"h")),operation:.copy,fraction:1)
        image.unlockFocus()
        if let teamFile=entry["teamFile"] as? String,let maskAtlas=NSImage(contentsOf:folder.appendingPathComponent(teamFile)),color.count>=3 {
            let mask=NSImage(size:image.size);mask.lockFocus()
            maskAtlas.draw(in:NSRect(x:number(f,"ox"),y:height-number(f,"oy")-number(f,"h"),width:number(f,"w"),height:number(f,"h")),
                from:NSRect(x:number(f,"x"),y:number(entry,"atlasHeight")-number(f,"y")-number(f,"h"),width:number(f,"w"),height:number(f,"h")),operation:.copy,fraction:1)
            mask.unlockFocus()
            if let data=image.tiffRepresentation,let base=CIImage(data:data),
               let maskData=mask.tiffRepresentation,let weights=CIImage(data:maskData) {
                let tinted=base.applyingFilter("CIColorMatrix",parameters:[
                    "inputRVector":CIVector(x:CGFloat(color[0])/255,y:0,z:0,w:0),
                    "inputGVector":CIVector(x:0,y:CGFloat(color[1])/255,z:0,w:0),
                    "inputBVector":CIVector(x:0,y:0,z:CGFloat(color[2])/255,w:0)])
                let mixed=tinted.applyingFilter("CIBlendWithMask",parameters:[kCIInputBackgroundImageKey:base,kCIInputMaskImageKey:weights])
                if let cg=colorContext.createCGImage(mixed,from:base.extent) {
                    let result=NSImage(cgImage:cg,size:image.size)
                    if frames.count>512 {frames.removeAll()};frames[key]=result;return result
                }
            }
        }
        if frames.count>512 {frames.removeAll()};frames[key]=image;return image
    }
    func apply(_ state:Obj,ground:Terrain3D,authored:Set<String>) {
        var live=Set<String>()
        for (prefix,list) in [("death",state["dyingUnits"] as? [Obj] ?? []),("lone",state["loneSprites"] as? [Obj] ?? [])] {
            for sprite in list {
                guard let identity=sprite["id"] as? String,!authored.contains(identity) else {continue}
                let main=sprite["image"] as? Obj ?? [:],chain=(sprite["visual"] as? Obj)?["images"] as? [Obj] ?? [main]
                for (index,native) in chain.enumerated() {
                    let id=Int(number(native,"id")),frame=Int(number(native,"frame")),key="\(prefix)/\(identity)/\(index)/\(id)"
                    guard Int(number(native,"flags"))&0x40==0 else {continue}
                    guard let entry=images[String(id)] else {missing.insert(id);continue}
                    guard let image=texture(id,frame:frame) else {continue}
                    live.insert(key)
                    let node:SCNNode
                    if let existing=nodes[key] {node=existing}
                    else {
                        let geometry=SCNPlane(width:number(entry,"grpWidth")/32,height:number(entry,"grpHeight")/32)
                        let material=SCNMaterial();material.lightingModel = .constant;material.isDoubleSided=true;material.readsFromDepthBuffer=false;material.writesToDepthBuffer=false
                        geometry.materials=[material];node=SCNNode(geometry:geometry);node.castsShadow=false
                        if Int(number(native,"drawfunc"))==10 {
                            node.eulerAngles.x = -.pi/2;material.multiply.contents=NSColor.black;node.opacity=0.5
                        } else {let billboard=SCNBillboardConstraint();billboard.freeAxes = .all;node.constraints=[billboard]}
                        nodes[key]=node;root.addChildNode(node)
                    }
                    node.renderingOrder=1000+Int(number(sprite,"y"))*32+chain.count-index
                    node.geometry?.firstMaterial?.diffuse.contents=image
                    let x=number(sprite,"x")/32,z=number(sprite,"y")/32
                    let shadow=Int(number(native,"drawfunc"))==10
                    node.position=Terrain3D.worldPosition(x+number(native,"offsetX")/32,ground.elevation(x,z),z)
                    node.position.y += CGFloat(shadow ? 0.025:-number(native,"offsetY")/32)
                    node.scale.x=Int(number(native,"flags"))&2 != 0 ? -1:1
                    if prefix=="death" {deathFrames+=1}else {loneFrames+=1}
                }
            }
        }
        for (key,node) in nodes where !live.contains(key) {node.removeFromParentNode();nodes.removeValue(forKey:key)}
    }
    func creepMaterial(tileset:String,index:Int)->SCNMaterial? {
        guard let entries=creep[tileset]?["tiles"] as? [Obj],!entries.isEmpty,
            let file=entries[index%entries.count]["file"] as? String else {return nil}
        return creepImage(file)
    }
    func creepEdgeFrame(tileset:String,neighbors:Int)->Int {
        guard let lookup=creep[tileset]?["edgeLookup"] as? [Int],lookup.indices.contains(neighbors) else {return 0}
        return lookup[neighbors]
    }
    func creepEdgeMaterial(tileset:String,frame:Int)->SCNMaterial? {
        guard let files=creep[tileset]?["edges"] as? [String],files.indices.contains(frame-1) else {return nil}
        return creepImage(files[frame-1])
    }
    func creepImage(_ file:String)->SCNMaterial? {
        if let material=creepMaterials[file] {return material}
        guard let image=NSImage(contentsOf:folder.appendingPathComponent(file)) else {return nil}
        let material=SCNMaterial();material.lightingModel = .constant;material.diffuse.contents=image
        material.isDoubleSided=true;material.writesToDepthBuffer=false
        creepMaterials[file]=material;return material
    }
}
