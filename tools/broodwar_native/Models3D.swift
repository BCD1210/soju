// Original game owns simulation. Each visible unit owns an isolated rig.
import Cocoa
import SceneKit
import GLTFKit2
import simd
import CryptoKit

final class ModelInstance {
    let root=SCNNode(),placement=SCNNode()
    let rig:SCNNode,source:GLTFSCNSceneSource,definition:ModelDefinition,revision:String
    var kind:Int { definition.kind };var modelName:String { definition.name }
    var playing="",action="idle",animationError=""
    var dying=false,deathFrame=0.0,deathDuration=0.0,attackUntilFrame=0.0
    var lastPosition:SIMD2<Double>?,lastFrame:Double = -1,lastFrameAt=0.0
    var targetPosition=SIMD3<Float>.zero,targetYaw:Float=0,positioned=false
    var activePlayers=[SCNAnimationPlayer]()
    var appliedTeamColor=[Int]()
    var rest=[(SCNNode,simd_float4x4)]()
    var allNodes=[SCNNode]()
    var clips=[String:[(SCNNode,CAAnimationGroup)]]()
    let selection=SCNNode(),bars=UnitBars()
    static let aliases:[String:[String]]=[
        "idle":["idle","stand","Stand_full","Stand A_full","Cover_full"],
        "walk":["walk","run","Walk_full","Fly Walk_full"],
        "attack":["attack","Attack_full"],"work":["work","mine","mining","build","Work_full"],
        "carryIdle":["carry_idle","idle_carry","Stand Gather_full"],
        "carryWalk":["carry","carry_walk","walk_carry","Walk Gather_full"],
        "construction":["construction","Build A Stand_full"],"death":["death","die","Death_full"],
        "fly":["fly","flight","Fly Walk_full"],"flyIdle":["fly_idle","hover"],
        "burrow":["burrow"],"burrowed":["burrowed"],"unburrow":["unburrow"],
        "siege":["siege","deploy"],"unsiege":["unsiege","undeploy"],
        "morph":["morph","transform"],"lift":["lift","takeoff"],"land":["land","landing"],
        "cast":["cast","spell"],"stunned":["stunned","disabled"],"repair":["repair","work"],
        "produce":["produce","production"],"depleted":["depleted"]]
    init(asset:GLTFAsset,definition:ModelDefinition,revision:String,folder:URL=runtime.appendingPathComponent("models-3d"))throws {
        self.definition=definition;self.revision=revision
        source=GLTFSCNSceneSource(asset:asset)
        guard let scene=source.defaultScene else { throw SceneError.invalid("No 3D scene: "+definition.file) }
        rig=scene.rootNode
        placement.addChildNode(rig);root.addChildNode(placement)
        let box=rig.boundingBox,height=Double(box.max.y-box.min.y)
        guard height.isFinite,height>0.00001,source.geometries.contains(where:{$0.elements.contains(where:{$0.primitiveType == .triangles && $0.primitiveCount>0})}) else {
            throw SceneError.invalid("Model needs nonempty 3D triangle geometry: "+definition.file)
        }
        let scale=definition.targetHeight.map{$0/height} ?? definition.scale
        placement.scale=SCNVector3(scale,scale,scale)
        placement.eulerAngles.y=CGFloat(definition.yawDegrees*Double.pi/180)
        if definition.footPivot { rig.position.y -= box.min.y }
        // Preserve authored PBR textures, roughness, metallic and two-sidedness.
        for material in source.materials {
            material.lightingModel = .physicallyBased
            let names=definition.raw["teamMaterials"] as? [String] ?? []
            if definition.teamMask != "none" && (names.isEmpty || names.contains(material.name ?? "")) {
                let mask=definition.teamMask=="alpha" ? "1.0 - texture2D(sourceTeamMask, _surface.diffuseTexcoord).a":"texture2D(teamMaskTexture, _surface.diffuseTexcoord).r"
                let textureUniform=definition.teamMask=="texture" ? "uniform sampler2D teamMaskTexture;":"uniform sampler2D sourceTeamMask;"
                let paint:String
                switch definition.teamColorBlend {
                case .multiply: paint="_surface.diffuse.rgb * teamColor"
                case .replace: paint="teamColor"
                }
                if definition.teamMask=="alpha", let contents=material.diffuse.contents,
                   CFGetTypeID(contents as CFTypeRef)==CGImage.typeID {
                    let original=contents as! CGImage
                    // PNG stores straight RGB. Remove alpha before SceneKit uploads the
                    // colour texture so zero-mask paint survives premultiplication.
                    let alpha:CGImageAlphaInfo = original.alphaInfo == .first ? .noneSkipFirst : .noneSkipLast
                    let info=CGBitmapInfo(rawValue:(original.bitmapInfo.rawValue & ~CGBitmapInfo.alphaInfoMask.rawValue) | alpha.rawValue)
                    guard let opaque=CGImage(width:original.width,height:original.height,bitsPerComponent:original.bitsPerComponent,
                        bitsPerPixel:original.bitsPerPixel,bytesPerRow:original.bytesPerRow,space:original.colorSpace!,
                        bitmapInfo:info,provider:original.dataProvider!,decode:nil,shouldInterpolate:true,intent:.defaultIntent) else {
                        throw SceneError.invalid("Cannot separate team paint from diffuse alpha")
                    }
                    material.diffuse.contents=opaque
                    material.setValue(SCNMaterialProperty(contents:original),forKey:"sourceTeamMask")
                }
                material.shaderModifiers=[.surface: """
                uniform vec3 teamColor;
                \(textureUniform)
                #pragma body
                float teamMask = clamp(\(mask), 0.0, 1.0);
                _surface.diffuse.rgb = mix(_surface.diffuse.rgb, \(paint), teamMask);
                _surface.diffuse.a = 1.0;
                """]
                material.setValue(NSValue(scnVector3:SCNVector3(1,1,1)),forKey:"teamColor")
                if let path=definition.raw["teamMaskTexture"] as? String,definition.teamMask=="texture" {
                    let root=folder.resolvingSymlinksInPath(),url=folder.appendingPathComponent(path).resolvingSymlinksInPath()
                    guard url.path.hasPrefix(root.path+"/"),let image=NSImage(contentsOf:url) else {throw SceneError.invalid("Missing team mask texture")}
                    material.setValue(SCNMaterialProperty(contents:image),forKey:"teamMaskTexture")
                }
            }
        }
        rig.enumerateChildNodes { node,_ in node.castsShadow=true }
        rig.enumerateChildNodes { node,_ in self.allNodes.append(node) }
        for name in definition.raw["turretNodes"] as? [String] ?? [] {
            guard allNodes.filter({$0.name==name}).count==1 else {throw SceneError.invalid("Turret node missing or ambiguous: \(name)")}
        }
        for name in definition.raw["teamMaterials"] as? [String] ?? [] {
            guard source.materials.contains(where:{$0.name==name}) else {throw SceneError.invalid("Team material missing: \(name)")}
        }
        rest=allNodes.map{($0,$0.simdTransform)}
        try bindAnimations()
        for required in definition.requiredAnimations where clip(for:required)==nil {
            throw SceneError.invalid("Missing required animation \(required) in \(definition.file); supply its catalog mapping")
        }
        let radius=max(0.22,CGFloat(max(box.max.x-box.min.x,box.max.z-box.min.z))*CGFloat(scale)*0.55)
        let ring=SCNTorus(ringRadius:radius,pipeRadius:0.018)
        let material=SCNMaterial();material.lightingModel = .constant;material.diffuse.contents=NSColor.green
        ring.materials=[material];selection.geometry=ring;selection.position.y=0.035;selection.isHidden=true
        selection.name="selection";selection.categoryBitMask=2;root.addChildNode(selection)
        bars.root.position.y=CGFloat(height*scale)+0.2;root.addChildNode(bars.root)
        bars.root.isHidden=true
        playAction("idle")
    }
    // GLTFKit2 uses absolute /Bone.property paths. Those become ambiguous once
    // two model roots share a scene. Bind each channel to its own instance node.
    func bindAnimations()throws {
        let named=Dictionary(grouping:allNodes,by:{$0.name ?? ""})
        for animation in source.animations {
            let original=CAAnimation(scnAnimation:animation.animationPlayer.animation)
            var channels=[ObjectIdentifier:(SCNNode,[CAAnimation])]()
            func visit(_ sourceAnimation:CAAnimation,offset:Double)throws {
                if let group=sourceAnimation as? CAAnimationGroup {
                    for child in group.animations ?? [] { try visit(child,offset:offset+group.beginTime) }
                    return
                }
                guard let channel=sourceAnimation.copy() as? CAPropertyAnimation,
                    let path=channel.keyPath,path.hasPrefix("/"),
                    let dot=path.range(of:".morpher.weights[",options:.backwards)?.lowerBound ?? path.lastIndex(of:".") else {
                    throw SceneError.invalid("Unsupported glTF animation channel in \(animation.name)")
                }
                let name=String(path[path.index(after:path.startIndex)..<dot])
                guard let matches=named[name],matches.count==1,let target=matches.first else {
                    throw SceneError.invalid("Animation target missing/ambiguous: \(name)")
                }
                channel.keyPath=String(path[path.index(after:dot)...]);channel.beginTime += offset
                channel.repeatDuration=0;channel.repeatCount=0;channel.fillMode = .both;channel.isRemovedOnCompletion=false
                let key=ObjectIdentifier(target)
                if channels[key]==nil { channels[key]=(target,[]) }
                channels[key]!.1.append(channel)
            }
            try visit(original,offset:0)
            clips[animation.name]=channels.values.map { node,parts in
                let group=CAAnimationGroup();group.animations=parts;group.duration=max(0.001,original.duration)
                group.repeatCount = .greatestFiniteMagnitude;group.isRemovedOnCompletion=false
                return (node,group)
            }
        }
    }
    static func normalize(_ text:String)->String { text.lowercased().filter{$0.isLetter || $0.isNumber} }
    func clip(for requested:String)->String? {
        let names=(definition.animations[requested] ?? [])+(Self.aliases[requested] ?? [])
        for name in names { if clips[name] != nil { return name } }
        for name in names {
            let normalized=Self.normalize(name)
            if let key=clips.keys.sorted().first(where:{Self.normalize($0)==normalized}) { return key }
        }
        return nil
    }
    @discardableResult func playAction(_ requested:String,sceneTime:Bool=false,force:Bool=false,blendDuration:Double=0.10,loop:Bool=true)->Bool {
        action=requested
        guard let name=clip(for:requested) ?? (requested=="carryWalk" ? clip(for:"walk"):nil) ?? clip(for:"idle") else {
            animationError=clips.isEmpty ? "":"Missing \(requested) clip";return false
        }
        animationError=clip(for:requested)==nil && requested != "idle" ? "Missing \(requested); fallback \(name)":""
        if playing==name && !force { return true }
        for (node,transform) in rest { node.removeAnimation(forKey:"native-action",blendOutDuration:0);node.simdTransform=transform }
        activePlayers.removeAll()
        for (node,group) in clips[name] ?? [] {
            let sequence=group.copy() as! CAAnimationGroup
            sequence.repeatCount=loop ? .greatestFiniteMagnitude:0;sequence.fillMode = .forwards
            let animation=SCNAnimation(caAnimation:sequence)
            animation.usesSceneTimeBase=sceneTime;animation.blendInDuration=blendDuration
            animation.blendOutDuration=0.10;animation.isRemovedOnCompletion=false
            let player=SCNAnimationPlayer(animation:animation)
            node.addAnimationPlayer(player,forKey:"native-action");player.play();activePlayers.append(player)
        }
        playing=name;return true
    }
    func beginDeath(frame:Double) {
        guard !dying else { return };dying=true;deathFrame=frame;selection.isHidden=true
        let duration=clip(for:"death").flatMap{clips[$0]?.first?.1.duration} ?? 0
        deathDuration=max(0.25,min(20,duration))+0.35
        if duration>0 { playAction("death",force:true,blendDuration:0,loop:false) }
        else { action="death";for player in activePlayers {player.stop()} }
    }
    func teamColor(_ rgba:[Int]) {
        guard rgba.count>=3,rgba != appliedTeamColor else {return};appliedTeamColor=rgba
        let names=Set(definition.raw["teamMaterials"] as? [String] ?? [])
        let color=NSColor(calibratedRed:CGFloat(rgba[0])/255,green:CGFloat(rgba[1])/255,blue:CGFloat(rgba[2])/255,alpha:1)
        func linear(_ value:CGFloat)->CGFloat { value<=0.04045 ? value/12.92 : pow((value+0.055)/1.055,2.4) }
        for material in source.materials {
            if definition.teamMask != "none" && (names.isEmpty || names.contains(material.name ?? "")) {
                material.setValue(NSValue(scnVector3:SCNVector3(linear(color.redComponent),linear(color.greenComponent),linear(color.blueComponent))),forKey:"teamColor")
            } else if names.contains(material.name ?? "") {material.multiply.contents=color}
        }
    }
    func updateDeath(frame:Double,now:Double) -> Bool {
        guard dying else {return false}
        if lastFrame != frame {lastFrame=frame;lastFrameAt=now}
        let elapsed=max(0,(frame-deathFrame)/24)
        root.opacity=CGFloat(max(0,min(1,(deathDuration-elapsed)/0.35)))
        return elapsed>=deathDuration
    }
    static func stateAction(_ u:Obj,moving:Bool,attacking:Bool)->String {
        let order=Int(number(u,"order")),status=u["status"] as? Obj ?? [:]
        if ["lockdown","stasis","maelstrom"].contains(where:{number(status,$0)>0}) {return "stunned"}
        if order==116 {return "burrow"};if order==118 {return "unburrow"}
        if order==98 {return "siege"};if order==99 {return "unsiege"}
        if [42,43,104,105,106].contains(order) {return "morph"}
        if [72,74].contains(order) {return "lift"};if order==71 {return "land"}
        if u["completed"] as? Bool == false && u["building"] as? Bool == true {return "construction"}
        let image=u["image"] as? Obj ?? [:],anim=Int(number(image,"animation"))
        // Moving attackers (turret units) retain their locomotion clip; the turret
        // heading and native weapon effects are independent of the body rig.
        if attacking && !moving {return "attack"}
        if u["burrowed"] as? Bool == true {return "burrowed"}
        if u["airborne"] as? Bool == true {return moving ? "fly":"flyIdle"}
        if moving {return number(u,"carrying") != 0 ? "carryWalk":"walk"}
        if order==34 {return "repair"}
        if anim==19 || [83,87,33].contains(order) {return "work"}
        if [2,3,5,6].contains(anim) {return "attack"}
        if anim==7 {return "cast"}
        if u["resource"] as? Bool == true && number(u,"resources")==0 {return "depleted"}
        if number(u,"secondaryOrder")==38 {return "produce"}
        return number(u,"carrying") != 0 ? "carryIdle":"idle"
    }
    func apply(_ unit:Obj,height:Double,frame:Double,now:Double=CACurrentMediaTime()) {
        guard !dying else {return}
        bars.apply(unit)
        let x=number(unit,"x")/32,z=number(unit,"y")/32,position=SIMD2<Double>(x,z)
        let moving=lastPosition.map{simd_length(position-$0)>0.006} ?? false
        if frame != lastFrame {
            lastPosition=position;lastFrame=frame;lastFrameAt=now
            playAction(Self.stateAction(unit,moving:moving,attacking:frame<attackUntilFrame))
        }
        let flight=unit["airborne"] as? Bool == true ? definition.flightHeight:0
        let placed=Terrain3D.worldPosition(x,height+definition.elevation+flight,z)
        targetPosition=SIMD3<Float>(Float(placed.x),Float(placed.y),Float(placed.z))
        targetYaw=definition.faceHeading ? Float(-number(unit,"facing")*2*Double.pi/256):0
        if !positioned || simd_distance(root.simdPosition,targetPosition)>12 {
            root.simdPosition=targetPosition;root.eulerAngles.y=CGFloat(targetYaw);positioned=true
        }
        root.isHidden=unit["renderHidden"] as? Bool == true || Int(number(unit["image"] as? Obj ?? [:],"flags"))&0x40 != 0
        root.opacity=unit["cloaked"] as? Bool == true ? 0.45:1
        let turret=unit["turret"] as? Obj ?? [:]
        for name in definition.raw["turretNodes"] as? [String] ?? [] {
            if let node=allNodes.first(where:{$0.name==name}) {
                node.eulerAngles.y=CGFloat(-(number(turret,"facing")-number(unit,"facing"))*2*Double.pi/256)
            }
        }
    }
    func advance(dt:Double,now:Double,selected:Bool) {
        let amount=Float(1-exp(-max(0,min(dt,0.1))*24))
        root.simdPosition += (targetPosition-root.simdPosition)*amount
        let current=Float(root.eulerAngles.y),delta=atan2(sin(targetYaw-current),cos(targetYaw-current))
        root.eulerAngles.y=CGFloat(current+delta*amount);selection.isHidden = !selected || dying
        bars.root.isHidden = !selected || dying
        for player in activePlayers { player.speed=now-lastFrameAt>0.4 ? 0:1 }
    }
}

final class Models3D {
    let catalog:ModelCatalog
    let fidelity:[Int:Obj]
    var assets=[Int:(String,GLTFAsset)](),errors=[Int:String]()
    var failedRevisions=[Int:String]()
    init(folder:URL=runtime.appendingPathComponent("models-3d"),catalogURL:URL?=nil,includeDefaults:Bool=true)throws {
        catalog=try ModelCatalog(folder:folder,catalog:catalogURL,includeDefaults:includeDefaults)
        let report=(try? loadObject(runtime.appendingPathComponent("model-fidelity.json"))) ?? [:]
        fidelity=Dictionary((report["models"] as? [Obj] ?? []).map{(Int(number($0,"type")),$0)},uniquingKeysWith:{_,new in new})
    }
    func approvedForLive(_ kind:Int)->Bool {
        guard let row=fidelity[kind],
              Self.passesReviewCriteria(row),row["modelRevision"] as? String == catalog.signatures[kind],
              let definition=catalog.definitions[kind],let path=try? catalog.url(definition),
              let bytes=try? Data(contentsOf:path) else {return false}
        let hash=SHA256.hash(data:bytes).map{String(format:"%02x",$0)}.joined()
        return row["modelSha256"] as? String == hash
    }
    static func passesReviewCriteria(_ row:Obj)->Bool {
        guard row["passed"] as? Bool == true else {return false}
        if row["method"] as? String == "visual-native" {
            guard row["visualReviewed"] as? Bool == true,let checks=row["nativeChecks"] as? [String:Bool] else {return false}
            return ["scale","picking","teamColors","clips","textureBudget"].allSatisfy{checks[$0]==true}
        }
        return number(row,"minimumIoU")>=0.95 && number(row,"minimumSSIM")>=0.85
    }
    func make(_ kind:Int)throws->ModelInstance {
        guard let definition=catalog.definitions[kind],let revision=catalog.signatures[kind] else {
            throw SceneError.invalid("Install unit-\(kind).glb or add type \(kind) to catalog.json")
        }
        if failedRevisions[kind]==revision { throw SceneError.invalid(errors[kind] ?? "Rejected model revision") }
        let asset:GLTFAsset
        do {
            // Incomplete imports must not replace the owned, animated image chain.
            if definition.raw["nativeImages"] as? Bool == true {
                throw SceneError.invalid("Catalog requests native image rendering")
            }
            if let cached=assets[kind],cached.0==revision { asset=cached.1 }
            else { asset=try GLTFAsset(url:catalog.url(definition),options:[:]) }
            let instance=try ModelInstance(asset:asset,definition:definition,revision:revision,folder:catalog.folder)
            assets[kind]=(revision,asset);errors.removeValue(forKey:kind);failedRevisions.removeValue(forKey:kind);return instance
        } catch {
            errors[kind]=String(describing:error);failedRevisions[kind]=revision;throw error
        }
    }
    static let playerColors=[[244,4,4],[12,72,204],[44,180,148],[136,64,156],[248,140,20],[112,48,20],[204,224,208],[252,252,56]]
    static func exposure(_ camera:SCNCamera) {
        // Match desktop proof and eye passes; avoid per-eye adaptive exposure.
        camera.wantsHDR=false;camera.wantsExposureAdaptation=false;camera.exposureOffset=0
    }
    static func lights(_ world:SCNScene,tileset:String="badlands") {
        let presets:[String:(Double,Double,NSColor)]=[
            "badlands":(-0.8,-0.6,NSColor(calibratedRed:1,green:0.94,blue:0.84,alpha:1)),
            "platform":(-0.65,0.4,NSColor(calibratedRed:0.88,green:0.93,blue:1,alpha:1)),
            "install":(-0.9,0.2,.white),"ashworld":(-0.5,-1,NSColor(calibratedRed:1,green:0.76,blue:0.58,alpha:1)),
            "jungle":(-0.9,0.8,NSColor(calibratedRed:1,green:0.96,blue:0.86,alpha:1)),
            "desert":(-0.7,-0.4,NSColor(calibratedRed:1,green:0.92,blue:0.77,alpha:1)),
            "ice":(-0.6,0.6,NSColor(calibratedRed:0.86,green:0.94,blue:1,alpha:1)),
            "twilight":(-0.4,-0.8,NSColor(calibratedRed:0.94,green:0.86,blue:1,alpha:1))]
        let preset=presets[tileset] ?? presets["badlands"]!
        // Linear floating-point radiance, generated locally; no external environment art.
        let width=256,height=128
        var pixels=[Float]();pixels.reserveCapacity(width*height*4)
        for y in 0..<height { for x in 0..<width {
            let latitude=Double(y)/Double(height-1)*Double.pi
            let longitude=Double(x)/Double(width-1)*2*Double.pi
            let sky=max(0,cos(latitude)),softbox=pow(max(0,sin(latitude)*cos(longitude-0.8)),24)
            let radiance=Float(0.12+0.45*sky+1.8*softbox)
            pixels += [radiance,radiance,radiance,1]
        }}
        let data=pixels.withUnsafeBytes{Data($0)}
        let environment=CIImage(bitmapData:data,bytesPerRow:width*16,size:CGSize(width:width,height:height),format:.RGBAf,colorSpace:CGColorSpace(name:CGColorSpace.linearSRGB)!)
        let context=CIContext(options:[.workingColorSpace:CGColorSpace(name:CGColorSpace.linearSRGB)!])
        world.lightingEnvironment.contents=context.createCGImage(environment,from:environment.extent,format:.RGBAh,colorSpace:CGColorSpace(name:CGColorSpace.linearSRGB)!)
        world.lightingEnvironment.intensity=1.2
        let fill=SCNNode();fill.light=SCNLight();fill.light!.type = .ambient;fill.light!.intensity=450
        fill.light!.color=NSColor.white;world.rootNode.addChildNode(fill)
        let sun=SCNNode();sun.name="commander-sun";sun.light=SCNLight();sun.light!.type = .directional;sun.light!.intensity=1050
        sun.light!.color=preset.2;sun.light!.castsShadow=true;sun.light!.shadowMapSize=CGSize(width:2048,height:2048)
        sun.light!.shadowMode = .deferred;sun.light!.shadowSampleCount=8
        sun.light!.automaticallyAdjustsShadowProjection=false;sun.light!.orthographicScale=30
        sun.light!.zNear=0.1;sun.light!.zFar=100;sun.light!.shadowColor=NSColor.black.withAlphaComponent(0.45)
        sun.eulerAngles=SCNVector3(preset.0,preset.1,0);world.rootNode.addChildNode(sun)
    }
    static func followLight(_ world:SCNScene,position:SCNVector3) {
        guard let sun=world.rootNode.childNode(withName:"commander-sun",recursively:false) else {return}
        let forward=sun.simdOrientation.act(SIMD3<Float>(0,0,-1))
        sun.simdPosition=SIMD3<Float>(Float(position.x),Float(position.y),Float(position.z))-forward*40
    }
}
