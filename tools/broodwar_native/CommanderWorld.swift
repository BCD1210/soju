import Cocoa
import SceneKit
import Metal
import QuartzCore
import CoreImage
import Darwin
import simd
extension Commander {
    func apply(_ s:Obj,_ f:Obj) {
        let liveMap=s["map"] as? Obj ?? [:]
        guard Int(number(liveMap,"width"))==mapWidth*32,Int(number(liveMap,"height"))==mapHeight*32 else {
            restartForMap();return
        }
        let prepared=art.manifest["map"] as? Obj ?? [:]
        if let expected=prepared["nativePath"] as? String,!expected.isEmpty,
            let actual=liveMap["path"] as? String,expected.lowercased() != actual.lowercased() {
            restartForMap();return
        }
        modelLibrary.catalog.poll()
        let newSession=s["session"] as? String ?? "legacy"
        let changed=newSession != session
        if changed {
            for n in nodes.values {n.removeFromParentNode()}
            for m in retiring.values {m.root.removeFromParentNode()}
            nodes.removeAll();models.removeAll();billboards.removeAll();retiring.removeAll();effects.clear();worldSprites.clear();selected=[];controlGroups.removeAll()
            for node in creepNodes.values {node.removeFromParentNode()};creepNodes.removeAll();creepTiles.removeAll()
            for node in terrain.values {node.removeFromParentNode()}
            terrain.removeAll();fog.removeAll();fogPresentation.cache.removeAll()
            wrists.pending=nil;wrists.dragStart=nil;wrists.grab=nil;wrists.grabDistance=nil;wrists.lastUnit="";wrists.lastTap=0
            initialized=false;session=newSession;eventSequence=0;deathCount=0;lostEvents=0
        }
        let events=changed ? []:(s["events"] as? [Obj] ?? []).filter{number($0,"seq")>eventSequence}
        eventSequence=number(s,"eventSeq")
        if s["eventsLost"] as? Bool == true {lostEvents+=1}
        state=s
        let list=s["units"] as? [Obj] ?? []
        units=Dictionary(uniqueKeysWithValues:list.compactMap { u in (u["id"] as? String).map { ($0,u) } })
        if !initialized,let base=list.first(where:{($0["townHall"] as? Bool == true || number($0,"type")==106) && number($0,"owner")==number(s,"player")}) {
            camera.position=SCNVector3(number(base,"x")/32-3,1.65,number(base,"y")/32+5)
            if let value=argument("--camera"),let a=Double(value.split(separator:",").first ?? ""),let b=Double(value.split(separator:",").last ?? "") {camera.position.x=CGFloat(a);camera.position.z=CGFloat(b)}
            if CommandLine.arguments.contains("--tabletop") {tabletop=true;pitch = -1}
            if let value=argument("--yaw"),let angle=Double(value) {yaw=CGFloat(angle)}
            if let value=argument("--select-type"),let kind=Int(value),let unit=list.first(where:{number($0,"type")==Double(kind) && number($0,"owner")==number(s,"player")}),let id=unit["id"] as? String {selected=[id]}
            initialized=true
        }
        let x=Int(number(f,"x")),y=Int(number(f,"y")),w=Int(number(f,"width"))
        for (i,c) in (f["fog"] as? String ?? "").utf8.enumerated() { fog[(y+i/w)*mapWidth+x+i%w]=c }
        for (i,c) in (f["creep"] as? String ?? "").utf8.enumerated() {
            let index=(y+i/w)*mapWidth+x+i%w
            if c==49 {creepTiles.insert(index)}else {creepTiles.remove(index)}
        }
        updateTerrain()
        selected=selected.filter { units[$0] != nil }
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
            do {
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
                instance.apply(u,height:ground.elevation(number(u,"x")/32,number(u,"y")/32),frame:number(s,"frame"))
                let colors=s["playerColors"] as? [[Int]] ?? [],owner=Int(number(u,"owner"))
                if colors.indices.contains(owner),colors[owner].prefix(3).contains(where:{$0>0}) {instance.teamColor(colors[owner])}
                else if Models3D.playerColors.indices.contains(owner) {instance.teamColor(Models3D.playerColors[owner])}
            } catch {
                if modelLibrary.errors[kind]==nil { modelLibrary.errors[kind]=String(describing:error) }
                if let old=models[id],old.kind==kind {
                    old.apply(u,height:ground.elevation(number(u,"x")/32,number(u,"y")/32),frame:number(s,"frame"))
                } else {
                    unsupported.insert(kind)
                    let billboard=billboards[id] ?? UnitBillboard(id:id)
                    if billboards[id]==nil {
                        nodes[id]?.removeFromParentNode();models.removeValue(forKey:id)
                        billboards[id]=billboard;nodes[id]=billboard.root;world.rootNode.addChildNode(billboard.root)
                    }
                    billboard.apply(u,ground:ground,art:worldSprites)
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
        if tabletop {return SCNVector3(camera.position.x-sin(yaw)*CGFloat(wrists.zoom),0,camera.position.z-cos(yaw)*CGFloat(wrists.zoom))}
        #if SOJU_OPENXR
        if xrRequested,let h=headCenter { return SCNVector3(h.x,h.y,h.z) }
        #endif
        return camera.position
    }
    func updateTerrain() {
        let tiles=art.manifest["tiles"] as? [String:Obj] ?? [:]
        let cx=Int(terrainCenter.x),cy=Int(terrainCenter.z),radius=tabletop ? 44:20
        var keep=Set<Int>()
        for y in max(0,cy-radius)..<max(0,min(mapHeight,cy+radius+1)) {
            for x in max(0,cx-radius)..<max(0,min(mapWidth,cx+radius+1)) {
                let index=y*mapWidth+x
                guard let visibility=fog[index],visibility != 48 else { continue }
                keep.insert(index)
                if let n=terrain[index] { n.geometry?.firstMaterial?.multiply.contents=fogPresentation.texture(x:x,y:y,width:mapWidth,fog:fog);continue }
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
                    world.rootNode.addChildNode(n);terrain[index]=n
                } catch { notice="Terrain: \(error)" }
            }
        }
        for (i,n) in terrain where !keep.contains(i) { n.removeFromParentNode();terrain.removeValue(forKey:i) }
        let tileset=(art.manifest["map"] as? Obj)?["tileset"] as? String ?? "badlands"
        let visibleCreep=creepTiles.filter{fog[$0]==50}
        var creepKeep=Set<Int>()
        for index in keep where fog[index]==50 {
            let x=index%mapWidth,y=index/mapWidth
            let interior=visibleCreep.contains(index)
            var neighbors=0
            for (bit,offset) in [(1,1),(0,1),(-1,1),(1,0),(-1,0),(1,-1),(0,-1),(-1,-1)].enumerated() {
                let nx=x+offset.0,ny=y+offset.1
                if nx>=0 && nx<mapWidth && ny>=0 && ny<mapHeight && visibleCreep.contains(ny*mapWidth+nx) {neighbors |= 1<<bit}
            }
            let edge=interior ? 0:worldSprites.creepEdgeFrame(tileset:tileset,neighbors:neighbors)
            if !interior && edge==0 {continue}
            creepKeep.insert(index)
            let signature=interior ? "creep":"creep-edge-\(edge)"
            if creepNodes[index]?.name==signature {continue}
            creepNodes.removeValue(forKey:index)?.removeFromParentNode()
            let material=interior ? worldSprites.creepMaterial(tileset:tileset,index:(x*7+y*13)%16):worldSprites.creepEdgeMaterial(tileset:tileset,frame:edge)
            if let material=material {
                let node=SCNNode(geometry:ground.geometry(tileX:x,tileY:y,material:material))
                node.position=SCNVector3(Double(x),0.015,Double(y));node.name=signature;node.castsShadow=false
                node.renderingOrder=1
                world.rootNode.addChildNode(node);creepNodes[index]=node
            }
        }
        for (index,node) in creepNodes where !creepKeep.contains(index) {node.removeFromParentNode();creepNodes.removeValue(forKey:index)}
    }
    func walk(_ dx:CGFloat,_ dz:CGFloat) {
        for axis in 0..<2 {
            let x=camera.position.x+(axis==0 ? dx:0),z=camera.position.z+(axis==1 ? dz:0)
            let before=ground.elevation(Double(camera.position.x),Double(camera.position.z))
            guard ground.walkable(Double(x),Double(z)),abs(ground.elevation(Double(x),Double(z))-before)<0.4 else { continue }
            camera.position.x=x;camera.position.z=z
        }
    }
    var commanderHeight:Double {tabletop ? Double(wrists.zoom)*0.9:1.65}
    func step() {
        let now=CACurrentMediaTime(),dt=CGFloat(min(0.1,now-lastTick));lastTick=now
        SCNTransaction.begin();SCNTransaction.disableActions=true
        for (id,model) in models { model.advance(dt:Double(dt),now:now,selected:selected.contains(id)) }
        for model in retiring.values {model.advance(dt:Double(dt),now:now,selected:false)}
        SCNTransaction.commit()
        var forward:CGFloat=0,side:CGFloat=0
        if view.keys.contains(13) { forward+=1 };if view.keys.contains(1) { forward-=1 }
        if view.keys.contains(0) { side-=1 };if view.keys.contains(2) { side+=1 }
        let speed:CGFloat=4
        walk((-sin(yaw)*forward+cos(yaw)*side)*speed*dt,(-cos(yaw)*forward-sin(yaw)*side)*speed*dt)
        camera.position.x=max(0.1,min(CGFloat(mapWidth)-0.1,camera.position.x))
        camera.position.z=max(0.1,min(CGFloat(mapHeight)-0.1,camera.position.z))
        camera.position.y=CGFloat(ground.elevation(Double(camera.position.x),Double(camera.position.z))+commanderHeight)
        camera.eulerAngles=SCNVector3(pitch,yaw,0)
        if CommandLine.arguments.contains("--top-down") {
            camera.position.y=40;camera.eulerAngles=SCNVector3(-Double.pi/2,0,0)
        }
        if !xrRequested {updateDesktopCard()}
        Models3D.followLight(world,position:camera.position)
        if !xrRequested,initialized,let index=captureFrames.reserve(),
            let tiff=view.snapshot().tiffRepresentation,let bitmap=NSBitmapImageRep(data:tiff),let image=bitmap.cgImage {
            captureFrames.save(image,index:index,metadata:["frame":number(state,"frame"),
                "camera":[camera.position.x,camera.position.y,camera.position.z],
                "map":state["map"] ?? [:],"terrainTiles":terrain.count,"topDown":camera.camera?.usesOrthographicProjection ?? false])
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
            let diagnostics:Obj=["time":Date().timeIntervalSince1970,"frame":number(state,"frame"),
                "units":rendered,"camera":[camera.position.x,camera.position.y,camera.position.z],"xr":xrRequested,
                "protocol":number(state,"protocol"),"eventSequence":eventSequence,"eventsLost":lostEvents,
                "creepTiles":creepNodes.count,"creepEdgeTiles":creepNodes.values.filter{$0.name?.hasPrefix("creep-edge-")==true}.count,
                "nativeDeathFrames":worldSprites.deathFrames,"nativeLoneFrames":worldSprites.loneFrames,"missingWorldImages":worldSprites.missing.sorted(),
                "spriteFrames":effects.sprites.renderedFrames,"missingEffectImages":effects.sprites.missing.sorted(),"combatEvents":effects.counts,"deathAnimations":deathCount,"retiringModels":retiring.count,"projectiles":effects.projectiles.count,
                "effectErrors":Dictionary(uniqueKeysWithValues:effects.library.errors.map{(String($0.key),$0.value)}),
                "catalogError":modelLibrary.catalog.lastError,"assetErrors":Dictionary(uniqueKeysWithValues:modelLibrary.errors.map{(String($0.key),$0.value)}),
                "billboardUnits":billboards.mapValues{$0.evidence},"devOverlay":devOverlay,
                "missingTypes":Array(Set(units.values.compactMap { u -> Int? in let id=u["id"] as? String ?? "";return models[id]==nil ? Int(number(u,"type")):nil })).sorted()]
            record("3d-scene-diagnostics.json",diagnostics)
            record(xrRequested ? "3d-xr-diagnostics.json":"3d-desktop-diagnostics.json",diagnostics)
        }
        label.stringValue="LIVE BROOD WAR · RIGGED 3D · \(xrRequested ? "OpenXR commander" : "Desktop commander")\nWASD walk · Drag select · Option-drag look · Ctrl+0–9 groups · V scale · F12 capture · G mine · F follow · T train worker · B structure · Q attack · H hold · X stop · R recenter · Right-click command\nFrame \(Int(number(state,"frame")))  Minerals \(Int(number(state,"minerals")))  Selected \(selected.count)  |  \(notice)"
    }
}
