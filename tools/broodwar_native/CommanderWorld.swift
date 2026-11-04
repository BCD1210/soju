import os.signpost
import Cocoa
import SceneKit
import Metal
import QuartzCore
import CoreImage
import Darwin
import simd
struct PreparedState {
    let list:[Obj],units:[String:Obj],dirty:Set<String>
    init(_ state:Obj,previous:Obj) {
        os_signpost(.begin,log:XRFrameMetrics.log,name:"State diff")
        defer {os_signpost(.end,log:XRFrameMetrics.log,name:"State diff")}
        list=state["units"] as? [Obj] ?? []
        units=Dictionary(uniqueKeysWithValues:list.compactMap {unit in (unit["id"] as? String).map{($0,unit)}})
        let old=Dictionary(uniqueKeysWithValues:(previous["units"] as? [Obj] ?? []).compactMap {unit in (unit["id"] as? String).map{($0,unit)}})
        let reset=state["session"] as? String != previous["session"] as? String ||
            !NSArray(array:state["playerColors"] as? [[Int]] ?? []).isEqual(to:previous["playerColors"] as? [[Int]] ?? [])
        dirty=Set(units.compactMap {id,unit in
            reset || old[id]==nil || !NSDictionary(dictionary:unit).isEqual(to:old[id]!) ? id:nil
        })
    }
}
extension Commander {
    func apply(_ s:Obj,_ f:Obj,prepared preparedState:PreparedState?=nil,capturedTerrain:([Int:UInt8],Set<Int>)?=nil) {
        os_signpost(.begin,log:XRFrameMetrics.log,name:"State apply")
        defer {os_signpost(.end,log:XRFrameMetrics.log,name:"State apply")}
        let liveMap=s["map"] as? Obj ?? [:]
        guard Int(number(liveMap,"width"))==mapWidth*32,Int(number(liveMap,"height"))==mapHeight*32 else {
            restartForMap();return
        }
        let prepared=art.manifest["map"] as? Obj ?? [:]
        if let expected=prepared["nativePath"] as? String,!expected.isEmpty,
            let actual=liveMap["path"] as? String,expected.lowercased() != actual.lowercased() {
            restartForMap();return
        }
        if sceneWorker==nil {modelLibrary.catalog.poll()}
        let newSession=s["session"] as? String ?? "legacy"
        let changed=newSession != session
        if changed {
            lastTerrainUpdate=nil
            for n in nodes.values {n.removeFromParentNode()}
            for m in retiring.values {m.root.removeFromParentNode()}
            nodes.removeAll();models.removeAll();billboards.removeAll();retiring.removeAll();effects.clear();worldSprites.clear();selected=[];controlGroups.removeAll()
            for node in creepNodes.values {node.removeFromParentNode()};creepNodes.removeAll();creepTiles.removeAll()
            for node in terrain.values {node.removeFromParentNode()}
            terrainRevision &+= 1
            terrain.removeAll();fog.removeAll();fogPresentation.cache.removeAll()
            wrists.pending=nil;wrists.dragStart=nil;wrists.grab=nil;wrists.grabDistance=nil;wrists.lastUnit="";wrists.lastTap=0
            initialized=false;session=newSession;eventSequence=0;deathCount=0;lostEvents=0
        }
        let events=changed ? []:(s["events"] as? [Obj] ?? []).filter{number($0,"seq")>eventSequence}
        eventSequence=number(s,"eventSeq")
        if s["eventsLost"] as? Bool == true {lostEvents+=1}
        sceneRevision &+= 1
        state=s
        let list=preparedState?.list ?? (s["units"] as? [Obj] ?? [])
        units=preparedState?.units ?? Dictionary(uniqueKeysWithValues:list.compactMap { u in (u["id"] as? String).map { ($0,u) } })
        if !initialized,let base=list.first(where:{($0["townHall"] as? Bool == true || number($0,"type")==106) && number($0,"owner")==number(s,"player")}) {
            camera.position=SCNVector3(number(base,"x")/32-3,1.65,number(base,"y")/32+5)
            if let value=argument("--camera"),let a=Double(value.split(separator:",").first ?? ""),let b=Double(value.split(separator:",").last ?? "") {camera.position.x=CGFloat(a);camera.position.z=CGFloat(b)}
            if CommandLine.arguments.contains("--tabletop") {tabletop=true;pitch = -1}
            if let value=argument("--yaw"),let angle=Double(value) {yaw=CGFloat(angle)}
            if let value=argument("--select-type"),let kind=Int(value),let unit=list.first(where:{number($0,"type")==Double(kind) && number($0,"owner")==number(s,"player")}),let id=unit["id"] as? String {selected=[id]}
            if !CommandLine.arguments.contains("--top-down") {
                let x=Double(camera.position.x),z=Double(camera.position.z)
                camera.position=Terrain3D.worldPosition(x,ground.elevation(x,z),z)
                camera.position.y += CGFloat(commanderHeight);placedEyeHeight=commanderHeight
            }
            initialized=true
        }
        let x=Int(number(f,"x")),y=Int(number(f,"y")),w=Int(number(f,"width"))
        let radius=tabletop || camera.camera?.usesOrthographicProjection == true ? 44:20
        let packet=TerrainUpdateKey(region:SIMD4<Int32>(Int32(x),Int32(y),Int32(w),Int32(number(f,"height"))),
            center:SIMD3<Int32>(Int32(terrainCenter.x),Int32(terrainCenter.z),Int32(radius)),
            fog:f["fog"] as? String ?? "",creep:f["creep"] as? String ?? "")
        if packet != lastTerrainUpdate {
            var dirty=Set<Int>()
            func changed(_ index:Int) {
                let tx=index%mapWidth,ty=index/mapWidth
                for dy in -1...1 {for dx in -1...1 {
                    let nx=tx+dx,ny=ty+dy
                    if nx>=0 && nx<mapWidth && ny>=0 && ny<mapHeight {dirty.insert(ny*mapWidth+nx)}
                }}
            }
            if w>0 {
                for (i,c) in packet.fog.utf8.enumerated() {
                    let index=(y+i/w)*mapWidth+x+i%w
                    if fog[index] != c {fog[index]=c;changed(index)}
                }
                for (i,c) in packet.creep.utf8.enumerated() {
                    let index=(y+i/w)*mapWidth+x+i%w
                    if creepTiles.contains(index) != (c==49) {
                        if c==49 {creepTiles.insert(index)}else {creepTiles.remove(index)}
                        changed(index)
                    }
                }
            }
            let sameRegion=lastTerrainUpdate?.region==packet.region && lastTerrainUpdate?.center==packet.center
            updateTerrain(dirty:sameRegion ? dirty:nil);lastTerrainUpdate=packet
        }
        selected=selected.filter { units[$0] != nil }
        if let captured=capturedTerrain {fog=captured.0;creepTiles=captured.1;updateTerrain()}
        if let worker=sceneWorker {
            notice="Native HD relief and sprite chains · authored rigs: SOJU_BW_RENDERER=scenekit"
            worker.submit(NativeSceneWorker.Update(state:s,fog:fog,creep:creepTiles)) { [weak self] in self?.busy=false }
            sendSceneView()
            return
        }
        for e in events where e["kind"] as? String == "death" {
            guard let id=e["unit"] as? String,units[id]==nil,retiring[id]==nil,
                let dead=e["state"] as? Obj,dead["renderHidden"] as? Bool != true else {continue}
            if let model=models[id],model.clip(for:"death") != nil {
                model.apply(dead,height:ground.elevation(number(dead,"x")/32,number(dead,"y")/32),frame:number(s,"frame"))
                model.beginDeath(frame:number(s,"frame"));retiring[id]=model;deathCount+=1
                models.removeValue(forKey:id);nodes.removeValue(forKey:id)
            }
        }
        for (id,n) in nodes where units[id]==nil {
            n.removeFromParentNode();nodes.removeValue(forKey:id);models.removeValue(forKey:id);billboards.removeValue(forKey:id)
        }
        var unsupported=Set<Int>()
        for (id,u) in units {
            let kind=Int(number(u,"type"))
            if preparedState?.dirty.contains(id)==false,models[id]==nil,billboards[id] != nil,!modelLibrary.approvedForLive(kind) {
                unsupported.insert(kind);continue
            }
            do {
                if requireFidelity && !modelLibrary.approvedForLive(kind) {
                    models.removeValue(forKey:id)?.root.removeFromParentNode()
                    throw SceneError.invalid("Model has no passing HD fidelity measurement")
                }
                let instance:ModelInstance
                if let old=models[id],old.kind==kind,old.revision==modelLibrary.catalog.signatures[kind] { instance=old }
                else {
                    instance=try modelLibrary.make(kind);instance.root.name=id
                    // Commit a replacement only after import and rig binding succeed.
                    nodes[id]?.removeFromParentNode()
                    billboards.removeValue(forKey:id)
                    models[id]=instance;nodes[id]=instance.root;world.rootNode.addChildNode(instance.root)
                }
                for e in events where e["kind"] as? String == "attack" && e["unit"] as? String == id {instance.attackUntilFrame=number(s,"frame")+5}
                instance.apply(u,height:ground.buildingElevation(u),frame:number(s,"frame"))
                let colors=s["playerColors"] as? [[Int]] ?? [],owner=Int(number(u,"owner"))
                if colors.indices.contains(owner),colors[owner].prefix(3).contains(where:{$0>0}) {instance.teamColor(colors[owner])}
                else if Models3D.playerColors.indices.contains(owner) {instance.teamColor(Models3D.playerColors[owner])}
            } catch {
                if modelLibrary.errors[kind]==nil { modelLibrary.errors[kind]=String(describing:error) }
                if let old=models[id],old.kind==kind {
                    old.apply(u,height:ground.buildingElevation(u),frame:number(s,"frame"))
                } else {
                    unsupported.insert(kind)
                    let billboard=billboards[id] ?? UnitBillboard(id:id)
                    if billboards[id]==nil {
                        nodes[id]?.removeFromParentNode();models.removeValue(forKey:id)
                        billboards[id]=billboard;nodes[id]=billboard.root;world.rootNode.addChildNode(billboard.root)
                    }
                    let colors=s["playerColors"] as? [[Int]] ?? [],owner=Int(number(u,"owner"))
                    billboard.apply(u,ground:ground,art:worldSprites,color:WorldSprites.playerColor(colors,owner:owner))
                }
            }
        }
        for (id,model) in retiring where model.updateDeath(frame:number(s,"frame"),now:CACurrentMediaTime()) {
            model.root.removeFromParentNode();retiring.removeValue(forKey:id)
        }
        effects.apply(bullets:s["bullets"] as? [Obj] ?? [],events:events,units:units,frame:number(s,"frame"),ground:ground)
        worldSprites.apply(s,ground:ground,authored:Set(retiring.keys))
        if !unsupported.isEmpty { notice="HD sprite fallback: \(unsupported.sorted())" }
        else if !modelLibrary.catalog.lastError.isEmpty { notice=modelLibrary.catalog.lastError }
        else if !modelLibrary.errors.isEmpty { notice="Asset update rejected; previous models retained: \(modelLibrary.errors.keys.sorted())" }
        else if ["Connecting","Connection:","3D models unavailable","Asset update rejected","Model catalog:","Reloading 3D assets"].contains(where:{notice.hasPrefix($0)}) { notice="Original engine connected · rigged 3D models" }
    }
    var terrainCenter:SCNVector3 {
        if CommandLine.arguments.contains("--top-down"),let values=argument("--camera")?.split(separator:","),values.count==2,
           let x=Double(values[0]),let z=Double(values[1]) {return SCNVector3(x,0,z)}
        if tabletop {let p=cameraMapPosition;return SCNVector3(p.x-sin(yaw)*CGFloat(wrists.zoom),0,p.z-cos(yaw)*CGFloat(wrists.zoom))}
        #if SOJU_OPENXR
        if xrRequested,let h=headCenter { return Terrain3D.mapPosition(SCNVector3(Double(h.x),Double(h.y)-commanderHeight,Double(h.z))) }
        #endif
        return cameraMapPosition
    }
    func updateTerrain(dirty:Set<Int>?=nil) {
        if sceneWorker != nil {return}
        if let worker=terrainWorker {
            let center=terrainCenter
            worker.submit(TerrainWorker.Request(session:session,center:SIMD2<Int>(Int(center.x),Int(center.z)),
                radius:tabletop || camera.camera?.usesOrthographicProjection == true ? 44:20,fog:fog,creep:creepTiles)) { [weak self] result in
                guard let self=self,result.session==self.session else {return}
                self.installTerrain(result)
            }
            return
        }
        let tiles=art.tileCatalog
        let cx=Int(terrainCenter.x),cy=Int(terrainCenter.z),radius=tabletop || camera.camera?.usesOrthographicProjection == true ? 44:20
        var keep=dirty==nil ? Set<Int>():Set(terrain.keys)
        let candidates:Set<Int>
        if let dirty=dirty {candidates=dirty}
        else {
            var region=Set<Int>()
            for y in max(0,cy-radius)..<max(0,min(mapHeight,cy+radius+1)) {
                for x in max(0,cx-radius)..<max(0,min(mapWidth,cx+radius+1)) {region.insert(y*mapWidth+x)}
            }
            candidates=region
        }
        for index in candidates {
                let x=index%mapWidth,y=index/mapWidth
                guard abs(x-cx)<=radius,abs(y-cy)<=radius,let visibility=fog[index],visibility != 48 else {keep.remove(index);continue}
                keep.insert(index)
                if let n=terrain[index] {
                    let mask=fogPresentation.texture(x:x,y:y,width:mapWidth,fog:fog)
                    if (n.geometry?.firstMaterial?.multiply.contents as? NSImage) !== mask {terrainFogRevision &+= 1;n.geometry?.firstMaterial?.multiply.contents=mask}
                    continue
                }
                guard let a=tiles[String(mapTiles[index])],let file=a["file"] as? String else { continue }
                do {
                    // HD megatiles already contain the original terrain lighting.
                    let neighbors=(-1...1).flatMap { dy in (-1...1).map { dx -> String in
                        let nx=max(0,min(mapWidth-1,x+dx)),ny=max(0,min(mapHeight-1,y+dy))
                        return tiles[String(mapTiles[ny*mapWidth+nx])]?["file"] as? String ?? file
                    }}
                    let material=try art.terrainMaterial(neighbors)
                    material.multiply.wrapS = .clamp;material.multiply.wrapT = .clamp
                    material.multiply.minificationFilter = .linear;material.multiply.magnificationFilter = .linear
                    let p=ground.geometry(tileX:x,tileY:y,material:material)
                    let n=SCNNode(geometry:p);n.name="terrain"
                    n.castsShadow=false
                    n.position=SCNVector3(CGFloat(x),0,CGFloat(y))
                    p.firstMaterial?.multiply.contents=fogPresentation.texture(x:x,y:y,width:mapWidth,fog:fog)
                    world.rootNode.addChildNode(n);terrain[index]=n;terrainRevision &+= 1
                } catch { notice="Terrain: \(error)" }
        }
        for (i,n) in terrain where !keep.contains(i) { n.removeFromParentNode();terrain.removeValue(forKey:i);terrainRevision &+= 1 }
        let tileset=(art.manifest["map"] as? Obj)?["tileset"] as? String ?? "badlands"
        let visibleCreep=creepTiles.filter{fog[$0]==50}
        var creepKeep=dirty==nil ? Set<Int>():Set(creepNodes.keys)
        for index in candidates {
            guard keep.contains(index),fog[index]==50 else {creepKeep.remove(index);continue}
            let x=index%mapWidth,y=index/mapWidth
            let interior=visibleCreep.contains(index)
            var neighbors=0
            for (bit,offset) in [(1,1),(0,1),(-1,1),(1,0),(-1,0),(1,-1),(0,-1),(-1,-1)].enumerated() {
                let nx=x+offset.0,ny=y+offset.1
                if nx>=0 && nx<mapWidth && ny>=0 && ny<mapHeight && visibleCreep.contains(ny*mapWidth+nx) {neighbors |= 1<<bit}
            }
            let edge=interior ? 0:worldSprites.creepEdgeFrame(tileset:tileset,neighbors:neighbors)
            if !interior && edge==0 {creepKeep.remove(index);continue}
            creepKeep.insert(index)
            let signature=interior ? "creep":"creep-edge-\(edge)"
            if creepNodes[index]?.name==signature {continue}
            creepNodes.removeValue(forKey:index)?.removeFromParentNode()
            let material=interior ? worldSprites.creepMaterial(tileset:tileset,index:x*7+y*13):worldSprites.creepEdgeMaterial(tileset:tileset,frame:edge)
            if let material=material {
                let node=SCNNode(geometry:ground.geometry(tileX:x,tileY:y,material:material))
                node.position=SCNVector3(Double(x),0,Double(y));node.name=signature;node.castsShadow=false
                node.renderingOrder=1
                world.rootNode.addChildNode(node);creepNodes[index]=node
            }
        }
        for (index,node) in creepNodes where !creepKeep.contains(index) {node.removeFromParentNode();creepNodes.removeValue(forKey:index)}
    }
    func installTerrain(_ result:TerrainWorker.Result) {
        for index in result.removedTerrain {terrain.removeValue(forKey:index)?.removeFromParentNode();terrainRevision &+= 1}
        for index in result.removedCreep {creepNodes.removeValue(forKey:index)?.removeFromParentNode()}
        for tile in result.terrain {
            if let node=terrain[tile.index] {node.geometry=tile.geometry;terrainFogRevision &+= 1}
            else {
                let node=SCNNode(geometry:tile.geometry);node.name=tile.name;node.castsShadow=false
                node.position=SCNVector3(CGFloat(tile.index%mapWidth),0,CGFloat(tile.index/mapWidth))
                world.rootNode.addChildNode(node);terrain[tile.index]=node;terrainRevision &+= 1
            }
        }
        for tile in result.creep {
            creepNodes.removeValue(forKey:tile.index)?.removeFromParentNode()
            let node=SCNNode(geometry:tile.geometry);node.name=tile.name;node.castsShadow=false;node.renderingOrder=1
            node.position=SCNVector3(CGFloat(tile.index%mapWidth),0,CGFloat(tile.index/mapWidth))
            world.rootNode.addChildNode(node);creepNodes[tile.index]=node
        }
        if !result.terrain.isEmpty || !result.creep.isEmpty || !result.removedTerrain.isEmpty || !result.removedCreep.isEmpty {sceneRevision &+= 1}
        if !result.error.isEmpty {notice=result.error}
    }
    var cameraMapPosition:SCNVector3 {
        Terrain3D.mapPosition(SCNVector3(camera.position.x,camera.position.y-CGFloat(placedEyeHeight),camera.position.z))
    }
    func walk(_ dx:CGFloat,_ dz:CGFloat) {
        var native=cameraMapPosition
        for axis in 0..<2 {
            let x=native.x+(axis==0 ? dx:0),z=native.z+(axis==1 ? dz:0)
            let before=ground.elevation(Double(native.x),Double(native.z))
            guard ground.walkable(Double(x),Double(z)),abs(ground.elevation(Double(x),Double(z))-before)<0.4 else {continue}
            native.x=x;native.z=z
        }
        native.x=max(0.1,min(CGFloat(mapWidth)-0.1,native.x));native.z=max(0.1,min(CGFloat(mapHeight)-0.1,native.z))
        camera.position=Terrain3D.worldPosition(Double(native.x),ground.elevation(Double(native.x),Double(native.z)),Double(native.z))
        camera.position.y += CGFloat(commanderHeight);placedEyeHeight=commanderHeight
    }
    var commanderHeight:Double {tabletop ? Double(wrists.zoom)*0.9:1.65}
    func desktopCaptureMetadata()->Obj {
        if let packet=sceneWorker?.mailbox.presented() {
            var metadata=packet.diagnostics
            metadata["snapshotSource"]=argument("--snapshot") ?? "live";metadata["creepTiles"]=metadata["creepTilesSnapshot"]
            metadata["elevationDegrees"]=Double(argument("--elevation") ?? "90") ?? 90
            metadata["terrainProjection"]="HD oblique relief at 32 degrees"
            return metadata
        }
        return ["frame":number(state,"frame"),
                "spriteBounds":billboards.mapValues{$0.projectedLayers(view:view,art:worldSprites)},
                "camera":[camera.position.x,camera.position.y,camera.position.z],
                "snapshotSource":argument("--snapshot") ?? "live", "terrainProjection":"HD oblique relief at 32 degrees", "state":state,"fog":Dictionary(uniqueKeysWithValues:fog.map{(String($0.key),Int($0.value))}),"creepTiles":Array(creepTiles).sorted(),"elevationDegrees":Double(argument("--elevation") ?? "90") ?? 90,"viewHeight":camera.camera?.orthographicScale ?? 0,
                "map":state["map"] ?? [:],"terrainTiles":terrain.count,"topDown":camera.camera?.usesOrthographicProjection ?? false]
    }
    func step() {
        os_signpost(.begin,log:XRFrameMetrics.log,name:"World tick")
        defer {os_signpost(.end,log:XRFrameMetrics.log,name:"World tick")}
        let now=CACurrentMediaTime(),dt=CGFloat(min(0.1,now-lastTick));lastTick=now
        #if SOJU_OPENXR
        if let loop=metalXRLoop {
            let feedback=loop.latest();xrOrigin=feedback.origin;xrOriginYaw=feedback.originYaw;headCenter=feedback.center
            if let packet=feedback.camera {xrCamera.simdTransform=packet.transform;xrCamera.camera?.projectionTransform=SCNMatrix4(packet.projection)}
            if let image=feedback.image,lastPacketMirror !== image {lastPacketMirror=image;mirror?.image=NSImage(cgImage:image,size:.zero)}
            while let input=loop.takeInput() {controls(input)}
            let error=loop.error();if !error.isEmpty {notice=error;label?.stringValue=error;label?.isHidden=false}
        }
        #endif
        if sceneWorker==nil {
        SCNTransaction.begin();SCNTransaction.disableActions=true
        for (id,model) in models { model.advance(dt:Double(dt),now:now,selected:selected.contains(id)) }
        for model in retiring.values {model.advance(dt:Double(dt),now:now,selected:false)}
        SCNTransaction.commit()
        }
        var forward:CGFloat=0,side:CGFloat=0
        if view.keys.contains(13) { forward+=1 };if view.keys.contains(1) { forward-=1 }
        if view.keys.contains(0) { side-=1 };if view.keys.contains(2) { side+=1 }
        let speed:CGFloat=4
        if !CommandLine.arguments.contains("--top-down") {
            walk((-sin(yaw)*forward+cos(yaw)*side)*speed*dt,(-cos(yaw)*forward-sin(yaw)*side)*speed*dt)
        }
        camera.eulerAngles=SCNVector3(pitch,yaw,0)
        if CommandLine.arguments.contains("--top-down") {
            let elevation=(Double(argument("--elevation") ?? "90") ?? 90)*Double.pi/180
            camera.position.y=40;camera.eulerAngles=SCNVector3(-elevation,0,0)
            if let center=argument("--camera")?.split(separator:",").last,let z=Double(center) {camera.position.z=CGFloat(z+40/tan(elevation))}
        }
        if !xrRequested {
            if sceneWorker==nil {for billboard in billboards.values {billboard.updateView(eye:camera.simdWorldPosition,orientation:camera.simdOrientation,perspective:camera.camera?.usesOrthographicProjection != true,art:worldSprites,recordEvidence:captureFrames.folder != nil || now-lastSceneDiagnostic>1)}}
            updateDesktopCard()
        }
        sendSceneView()
        if sceneWorker==nil {Models3D.followLight(world,position:camera.position)}
        if !xrRequested,!useMetalBatches,initialized,let index=captureFrames.reserve(),
            let tiff=view.snapshot().tiffRepresentation,let bitmap=NSBitmapImageRep(data:tiff),let image=bitmap.cgImage {
            captureFrames.save(image,index:index,metadata:desktopCaptureMetadata())
        }
        if now-lastSceneDiagnostic>1 {
            lastSceneDiagnostic=now
            let rendered:[Obj]=models.map { id,m in
                ["id":id,"type":m.kind,"model":m.modelName,"animation":m.playing,
                 "position":[m.root.position.x,m.root.position.y,m.root.position.z],"yaw":m.root.eulerAngles.y,
                 "hidden":m.root.isHidden,"geometry":"rigged triangles","action":m.action,
                 "animationWarning":m.animationError,"animationPlayers":m.activePlayers.count,
                 "file":m.definition.file]
            }
            var diagnostics:Obj=["time":Date().timeIntervalSince1970,"frame":number(state,"frame"),
                "selected":selected,"cardPage":wrists.page,"nativeCard":wrists.card.nativeResponse,"commandCard":wrists.choices.map{choice -> Obj in
                    ["title":choice.title,"hotkey":choice.hotkey,"payload":choice.payload,"position":choice.originalPosition ?? 0,
                     "unavailable":choice.unavailable(state,selected.first.flatMap{units[$0]} ?? [:]) ?? ""]},
                "units":rendered,"camera":[camera.position.x,camera.position.y,camera.position.z],"xr":xrRequested,
                "protocol":number(state,"protocol"),"eventSequence":eventSequence,"eventsLost":lostEvents,
                "creepTiles":creepNodes.count,"creepEdgeTiles":creepNodes.values.filter{$0.name?.hasPrefix("creep-edge-")==true}.count,
                "nativeDeathFrames":worldSprites.deathFrames,"nativeLoneFrames":worldSprites.loneFrames,"missingWorldImages":worldSprites.missing.sorted(),
                "spriteFrames":effects.sprites.renderedFrames,"missingEffectImages":effects.sprites.missing.sorted(),"combatEvents":effects.counts,"deathAnimations":deathCount,"retiringModels":retiring.count,"projectiles":effects.projectiles.count,
                "effectErrors":Dictionary(uniqueKeysWithValues:effects.library.errors.map{(String($0.key),$0.value)}),
                "catalogError":modelLibrary.catalog.lastError,"assetErrors":Dictionary(uniqueKeysWithValues:modelLibrary.errors.map{(String($0.key),$0.value)}),
                "billboardUnits":billboards.mapValues{$0.evidence},"devOverlay":devOverlay,
                "missingTypes":Array(Set(units.values.compactMap { u -> Int? in let id=u["id"] as? String ?? "";return models[id]==nil ? Int(number(u,"type")):nil })).sorted()]
            if let packet=sceneWorker?.mailbox.latest() {
                diagnostics.merge(packet.diagnostics){_,new in new}
                diagnostics["packetGeneration"]=packet.generation;diagnostics["packetNativeFrame"]=packet.nativeFrame
                diagnostics["packetAgeMs"]=(CACurrentMediaTime()-packet.publishedAt)*1000
            }
            if let error=sceneWorker?.mailbox.error(),!error.isEmpty {notice="Native scene: "+error;diagnostics["sceneWorkerError"]=error}
            record("3d-scene-diagnostics.json",diagnostics)
            record(xrRequested ? "3d-xr-diagnostics.json":"3d-desktop-diagnostics.json",diagnostics)
        }
        if devOverlay {label.stringValue="LIVE BROOD WAR · RIGGED 3D · \(xrRequested ? "OpenXR commander" : "Desktop commander")\nWASD walk · Drag select · Option-drag look · Ctrl+0–9 groups · V scale · F12 capture · G mine · F follow · T train worker · B structure · Q attack · H hold · X stop · R recenter · Right-click command\nFrame \(Int(number(state,"frame")))  Minerals \(Int(number(state,"minerals")))  Selected \(selected.count)  |  \(notice)"}
    }
}
