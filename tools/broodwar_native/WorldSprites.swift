import Cocoa
import SceneKit
import CoreImage
import simd

// Native animation groups are retained while the observer selects a view direction.
final class UnitBillboard {
    let root=SCNNode()
    var layers=[Int:SCNNode](),evidence=[Obj]()
    var nativeLayers=[Int:Obj](),unit:Obj=[:],team=[255,255,255]
    var shown=[Int:Int](),shownImages=[Int:Int]()
    var shownColors=[Int:[Int]](),contacts=[Int:Float]()
    var layerOrder=[Int]()
    var lastNativeChain=[Obj]()
    var geometryRevision=0
    var lastEye:SIMD3<Float>?,lastRotation:SIMD4<Float>?,lastPerspective:Bool?
    init(id:String) {root.name=id}
    func apply(_ unit:Obj,ground:Terrain3D,art:WorldSprites,color:[Int]=[255,255,255]) {
        lastEye=nil
        let sameTeam=team==color
        self.unit=unit;team=color
        let x=number(unit,"x")/32,z=number(unit,"y")/32
        root.position=Terrain3D.worldPosition(x,ground.elevation(x,z)+(unit["airborne"] as? Bool == true ? 1.5:0),z)
        root.isHidden=unit["renderHidden"] as? Bool == true || unit["transported"] as? Bool == true
        root.opacity=unit["cloaked"] as? Bool == true ? 0.45:1
        let main=unit["image"] as? Obj ?? [:]
        let chain=(unit["visual"] as? Obj)?["images"] as? [Obj] ?? [main]
        if sameTeam,!layers.isEmpty,NSArray(array:chain).isEqual(to:lastNativeChain) {return}
        lastNativeChain=chain;geometryRevision &+= 1
        var keep=Set<Int>();evidence=[]
        for (index,native) in chain.enumerated() {
            guard Int(number(native,"flags"))&0x40==0,let id=native["id"] as? Int,
                let entry=art.images[String(id)],let records=art.frameRecords(id) else {continue}
            let frame=Int(number(native,"frame"))
            guard records.indices.contains(frame) else {continue}
            let prepared=art.stream?.get(id,frame:frame,color:color)
            if let stream=art.stream,prepared==nil {
                stream.request(id,frame:frame,color:color);keep.insert(index);nativeLayers[index]=native
                if layers[index]==nil {
                    let node=SCNNode(geometry:SCNPlane(width:number(entry,"grpWidth")/32,height:number(entry,"grpHeight")/32))
                    let material=SCNMaterial();material.lightingModel = .constant;material.isDoubleSided=true
                    node.geometry?.materials=[material];node.isHidden=true;layers[index]=node;root.addChildNode(node)
                }
                continue
            }
            guard let image:Any=prepared?.image ?? art.texture(id,frame:frame,color:color) as Any? else {continue}
            let scale=number(entry,"scale")*32
            guard scale>0 else {continue}
            keep.insert(index);nativeLayers[index]=native;shown[index]=frame;shownImages[index]=id;shownColors[index]=color
            if let prepared=prepared {contacts[index]=prepared.contact}
            let shadow=Int(number(native,"drawfunc"))==10,node=layers[index] ?? SCNNode()
            let plane=SCNPlane(width:number(entry,"grpWidth")/32,height:number(entry,"grpHeight")/32)
            let material=SCNMaterial();material.lightingModel = .constant;material.isDoubleSided=true
            material.diffuse.contents=image;material.diffuse.wrapS = .clamp;material.diffuse.wrapT = .clamp
            material.readsFromDepthBuffer=false;material.writesToDepthBuffer=false;plane.materials=[material];node.geometry=plane
            if !shadow,let mesh=(prepared?.geometry?.copy() as? SCNGeometry) ?? (art.stream==nil ? art.relief.geometry(id,frame:frame,art:art):nil) {
                mesh.materials=[(prepared?.material.copy() as? SCNMaterial) ?? art.relief.material(id,frame:frame,color:color,art:art)];node.geometry=mesh
            }
            node.isHidden=false;node.castsShadow=false;node.renderingOrder=node.geometry is SCNPlane ? 1000+Int(number(unit,"y"))*32+chain.count-index:0
            if shadow {
                node.constraints=nil;node.eulerAngles.x = -.pi/2;material.multiply.contents=NSColor.black;material.readsFromDepthBuffer=true;node.opacity=0.5
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
        nativeLayers=nativeLayers.filter{keep.contains($0.key)};shown=shown.filter{keep.contains($0.key)}
        shownImages=shownImages.filter{keep.contains($0.key)};shownColors=shownColors.filter{keep.contains($0.key)};contacts=contacts.filter{keep.contains($0.key)}
        layerOrder=nativeLayers.keys.sorted()
        for (index,node) in layers where !keep.contains(index) {node.removeFromParentNode();layers.removeValue(forKey:index)}
    }
    static func viewFrame(_ native:Obj,azimuth:Double,count:Int)->(Int,Bool) {
        let flags=Int(number(native,"flags")),original=Int(number(native,"frame"))
        guard flags&8 != 0 else {return (original,flags&2 != 0)}
        let direction=Int(number(native,"direction"))
        let heading=flags&2 != 0 ? 32-direction:direction
        let turn=Int((Double(heading)+azimuth*16/Double.pi).rounded())
        let wrapped=(turn%32+32)%32,mirrored=wrapped>16
        let frame=Int(number(native,"frameset"))+(mirrored ? 32-wrapped:wrapped)
        return (0..<count).contains(frame) ? (frame,mirrored):(original,flags&2 != 0)
    }
    func updateView(eye:SIMD3<Float>,orientation:simd_quatf,perspective:Bool,art:WorldSprites,recordEvidence:Bool=true) {
        if lastEye==eye && lastRotation==orientation.vector && lastPerspective==perspective {return}
        lastEye=eye;lastRotation=orientation.vector;lastPerspective=perspective
        let delta=eye-root.simdWorldPosition,azimuth=atan2(Double(delta.x),Double(delta.z))
        let rotation=perspective ? simd_quatf(angle:Float(azimuth),axis:SIMD3<Float>(0,1,0))*simd_quatf(angle:-Float(Terrain3D.canonicalElevation),axis:SIMD3<Float>(1,0,0)):orientation
        var contact=0.0
        let main=unit["image"] as? Obj ?? [:]
        if perspective,let entry=art.images[String(Int(number(main,"id")))],let records=art.frameRecords(Int(number(main,"id"))) {
            let (frame,_)=Self.viewFrame(main,azimuth:azimuth,count:records.count)
            if records.indices.contains(frame) {
                contact=max(0,(number(records[frame],"oy")+number(records[frame],"h"))/number(entry,"scale")-number(entry,"grpHeight")/2+number(main,"offsetY"))/32
            }
        }
        var buildingLift:Float=0
        if perspective,unit["building"] as? Bool == true,unit["airborne"] as? Bool != true,
           let entry=art.images[String(Int(number(main,"id")))],let records=art.frameRecords(Int(number(main,"id"))) {
            let (frame,_)=Self.viewFrame(main,azimuth:azimuth,count:records.count)
            let id=Int(number(main,"id"))
            let key="\(id)/\(frame)"
            if art.stream==nil && art.relief.contactHeights[key]==nil {_ = art.relief.geometry(id,frame:frame,art:art)}
            if let minimum=art.stream?.get(id,frame:frame,color:team)?.contact ?? (shownImages[0]==id && shown[0]==frame ? contacts[0]:nil) ?? art.relief.contactHeights[key] {
                let oy=(number(main,"offsetY")+Double(Int(number(entry,"grpHeight"))%2)/2)/32
                buildingLift = -minimum+Float(oy*cos(Terrain3D.canonicalElevation))
            }
        }
        if recordEvidence {evidence.removeAll(keepingCapacity:true)}
        for index in layerOrder {
            guard let native=nativeLayers[index] else {continue}
            guard let node=layers[index],let entry=art.images[String(Int(number(native,"id")))],
                let records=art.frameRecords(Int(number(native,"id"))) else {continue}
            let shadow=Int(number(native,"drawfunc"))==10
            let (frame,mirrored)=perspective && !shadow ? Self.viewFrame(native,azimuth:azimuth,count:records.count):(Int(number(native,"frame")),Int(number(native,"flags"))&2 != 0)
            let id=Int(number(native,"id"))
            if shown[index] != frame || shownImages[index] != id || shownColors[index] != team {
                let prepared=art.stream?.get(id,frame:frame,color:team)
                if let stream=art.stream,prepared==nil {
                    stream.request(id,frame:frame,color:team);lastEye=nil;continue
                }
                geometryRevision &+= 1
                if !shadow,let mesh=(prepared?.geometry?.copy() as? SCNGeometry) ?? (art.stream==nil ? art.relief.geometry(id,frame:frame,art:art):nil) {
                    mesh.materials=[(prepared?.material.copy() as? SCNMaterial) ?? art.relief.material(id,frame:frame,color:team,art:art)];node.geometry=mesh;node.renderingOrder=0
                } else {node.geometry?.firstMaterial?.diffuse.contents=prepared?.image ?? art.texture(id,frame:frame,color:team) as Any?}
                shown[index]=frame;shownImages[index]=id;shownColors[index]=team;node.isHidden=false
                if let prepared=prepared {contacts[index]=prepared.contact}
            }
            let ox=(number(native,"offsetX")+Double(Int(number(entry,"grpWidth"))%2)/2)/32
            let oy=(number(native,"offsetY")+Double(Int(number(entry,"grpHeight"))%2)/2)/32
            if let stream=art.stream,Int(number(native,"flags"))&8 != 0,!shadow {
                let base=Int(number(native,"frameset")),id=Int(number(native,"id"))
                for neighbor in [frame-1,frame+1] where neighbor>=base && neighbor<=base+16 && records.indices.contains(neighbor) {
                    stream.request(id,frame:neighbor,color:team)
                }
            }
            node.opacity=shadow ? 0.5:1
            node.constraints=nil
            node.scale=SCNVector3(1,1,1)
            if shadow {
                // Owned shadow-image alpha retains the original soft coverage.
                // Undo the source projection before laying that layer on ground.
                (node.geometry as? SCNPlane)?.height=number(entry,"grpHeight")/32/sin(Terrain3D.canonicalElevation)
                node.eulerAngles=SCNVector3(-Double.pi/2,0,0)
                let flight=unit["airborne"] as? Bool == true ? 1.5:0
                node.position=SCNVector3(ox,0.002-flight,oy/sin(Terrain3D.canonicalElevation)-flight*Terrain3D.reliefDepth)
                node.renderingOrder=1
            } else {
                node.simdOrientation=rotation
                let offset=rotation.act(SIMD3<Float>(Float(ox),Float(-oy),0))
                node.simdPosition=offset+SIMD3<Float>(0,node.geometry is SCNPlane ? Float(contact):buildingLift,0)
            }
            node.scale.x=mirrored ? -1:1
            if !useMetalBatches,!(node.geometry is SCNPlane),let material=node.geometry?.firstMaterial {
                var light=rotation.inverse.act(SpriteRelief.sun)
                var view=rotation.inverse.act(simd_normalize(delta))
                if mirrored {light.x = -light.x;view.x = -view.x}
                if !perspective {light=SpriteRelief.reference;view=SIMD3<Float>(0,0,1)}
                material.setValue(NSValue(scnVector3:SCNVector3(light)),forKey:"reliefLight")
                material.setValue(NSValue(scnVector3:SCNVector3(view)),forKey:"reliefEye")
            }
            if recordEvidence {evidence.append(["image":number(native,"id"),"frame":frame,"nativeFrame":number(native,"frame"),
                "frameset":number(native,"frameset"),"mirrored":mirrored,"observerAzimuth":azimuth,
                "shadow":shadow,"relief":!(node.geometry is SCNPlane),"contactLift":contact,"buildingLift":buildingLift,"position":[node.position.x,node.position.y,node.position.z]])}
        }
    }
    func projectedLayers(view:SCNView,art:WorldSprites)->[Obj] {
        nativeLayers.sorted(by:{$0.key<$1.key}).compactMap { index,native in
            guard let node=layers[index],
                let entry=art.images[String(Int(number(native,"id")))],let records=art.frameRecords(Int(number(native,"id"))),
                let frame=shown[index],records.indices.contains(frame) else {return nil}
            let record=records[frame],scale=number(entry,"scale")
            let left=number(record,"ox")/(number(entry,"grpWidth")*scale)-0.5
            let top=0.5-number(record,"oy")/(number(entry,"grpHeight")*scale)
            let right=left+number(record,"w")/(number(entry,"grpWidth")*scale)
            let bottom=top-number(record,"h")/(number(entry,"grpHeight")*scale)
            let points=[(left,top),(right,top),(right,bottom),(left,bottom)].map { x,y in
                view.projectPoint(node.convertPosition(SCNVector3(x*number(entry,"grpWidth")/32,y*number(entry,"grpHeight")/32,0),to:nil))
            }
            return ["image":number(native,"id"),"frame":frame,"shadow":number(native,"drawfunc")==10,
                "left":points.map{$0.x}.min()!,"top":view.bounds.height-points.map{$0.y}.max()!,
                "right":points.map{$0.x}.max()!,"bottom":view.bounds.height-points.map{$0.y}.min()!]
        }
    }

}

// Native image chains drive both independent map sprites and fallback death frames.
final class WorldSprites {
    let stream:FrameStream?
    let relief=SpriteRelief()
    let root=SCNNode(),folder=runtime.appendingPathComponent("world-sprites")
    let images:[String:Obj],creep:[String:Obj]
    let creepEdges:[String:[Int]]
    private var frameCatalog=[Int:[Obj]]()
    let colorContext=CIContext()
    var nodes=[String:SCNNode]()
    let frames=ResidentCache<NSImage>(limit:512*1024*1024)
    let atlases=ResidentCache<NSImage>(limit:256*1024*1024)
    var missing=Set<Int>(),deathFrames=0,loneFrames=0
    var creepMaterials=[String:SCNMaterial]()
    init(streaming:Bool?=nil) {
        stream=(streaming ?? (useMetalBatches && !CommandLine.arguments.contains("--runtime-check"))) ? FrameStream():nil
        let catalog=(try? loadObject(folder.appendingPathComponent("catalog.json"))) ?? [:]
        images=catalog["images"] as? [String:Obj] ?? [:];creep=catalog["creep"] as? [String:Obj] ?? [:]
        creepEdges=creep.mapValues{$0["edgeLookup"] as? [Int] ?? []}
        root.name="native-world-sprites"
    }
    func frameRecords(_ id:Int)->[Obj]? {
        if let cached=frameCatalog[id] {return cached}
        guard let records=images[String(id)]?["frames"] as? [Obj] else {return nil}
        frameCatalog[id]=records;return records
    }
    static func playerColor(_ colors:[[Int]],owner:Int)->[Int] {
        if colors.indices.contains(owner),colors[owner].count>=3,colors[owner].prefix(3).contains(where:{$0>0}) {return Array(colors[owner].prefix(3))}
        return Models3D.playerColors.indices.contains(owner) ? Models3D.playerColors[owner]:[255,255,255]
    }
    func clear() {
        for node in nodes.values {node.removeFromParentNode()}
        nodes.removeAll();deathFrames=0;loneFrames=0;missing.removeAll()
    }
    func texture(_ id:Int,frame:Int,color:[Int]=[255,255,255],layer:String="diffuse")->NSImage? {
        let tint=(layer=="diffuse" || layer=="bright") ? color:[]
        let key="\(id)/\(frame)/\(tint)/\(layer)"
        if let image=frames.get(key) {return image}
        guard let entry=images[String(id)],let records=frameRecords(id),records.indices.contains(frame),let file=(layer=="diffuse" ? entry["file"] as? String:((entry["relief"] as? Obj)?["files"] as? [String:String])?[layer]) else {if layer=="diffuse" {missing.insert(id)};return nil}
        let atlas:NSImage
        if let cached=atlases.get(file) {atlas=cached}
        else {guard let loaded=NSImage(contentsOf:folder.appendingPathComponent(file)) else {return nil};atlas=loaded;atlases.insert(loaded,key:file,bytes:Int(loaded.size.width*loaded.size.height)*4)}
        let f=records[frame],width=number(entry,"grpWidth")*number(entry,"scale"),height=number(entry,"grpHeight")*number(entry,"scale")
        let image=NSImage(size:NSSize(width:width,height:height));image.lockFocus()
        atlas.draw(in:NSRect(x:number(f,"ox"),y:height-number(f,"oy")-number(f,"h"),width:number(f,"w"),height:number(f,"h")),
                   from:NSRect(x:number(f,"x"),y:number(entry,"atlasHeight")-number(f,"y")-number(f,"h"),width:number(f,"w"),height:number(f,"h")),operation:.copy,fraction:1)
        image.unlockFocus()
        if (layer=="diffuse" || layer=="bright"),let teamFile=entry["teamFile"] as? String,let maskAtlas=NSImage(contentsOf:folder.appendingPathComponent(teamFile)),color.count>=3 {
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
                    frames.insert(result,key:key,bytes:Int(width*height)*4);return result
                }
            }
        }
        frames.insert(image,key:key,bytes:Int(width*height)*4);return image
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
        guard let lookup=creepEdges[tileset],lookup.indices.contains(neighbors) else {return 0}
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
        var orientation=SCNMatrix4MakeScale(1,-1,1);orientation.m42=1
        material.diffuse.contentsTransform=orientation
        material.isDoubleSided=true;material.writesToDepthBuffer=false
        creepMaterials[file]=material;return material
    }
}
