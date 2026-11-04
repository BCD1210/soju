import Cocoa
import SceneKit
import Metal
import simd

final class NativeSceneWorker {
    struct Update {let state:Obj,fog:[Int:UInt8],creep:Set<Int>}
    let mailbox=SceneMailbox()
    private let queue=DispatchQueue(label:"soju.bw.scene-owner",qos:.userInitiated)
    private let viewLock=NSLock()
    private var latestView:SceneView?,viewScheduled=false
    private var builder:Builder?
    private let device:MTLDevice
    init(device:MTLDevice) {self.device=device}
    // The bridge keeps its single outstanding request busy until this callback.
    // Native updates are FIFO and never replaced by a newer state or view.
    func submit(_ update:Update,completion:@escaping()->Void) {
        mailbox.invalidate(update.state["session"] as? String ?? "legacy")
        queue.async { [self] in
            autoreleasepool {
                do {
                    if builder==nil {
                        builder=try Builder(device:device)
                        builder!.sprites.stream?.onReady={ [weak self] in self?.scheduleView() }
                    }
                    builder!.apply(update)
                    publish()
                } catch {mailbox.fail(String(describing:error));fputs("Native scene worker: \(error)\n",stderr)}
            }
            DispatchQueue.main.async(execute:completion)
        }
    }
    func view(_ view:SceneView) {viewLock.lock();latestView=view;viewLock.unlock();scheduleView()}
    private func scheduleView() {
        viewLock.lock()
        if viewScheduled {viewLock.unlock();return}
        viewScheduled=true;viewLock.unlock()
        queue.async { [self] in
            viewLock.lock();viewScheduled=false;viewLock.unlock()
            autoreleasepool {publish()}
        }
    }
    private func publish() {
        viewLock.lock();let view=latestView;viewLock.unlock()
        guard let builder=builder,let view=view,view.session==builder.session else {return}
        do {mailbox.publish(try builder.packet(view))}
        catch {mailbox.fail(String(describing:error));fputs("Native scene packet: \(error)\n",stderr)}
    }
    final class Builder {
        let terrain:TerrainWorker.Builder,sprites=WorldSprites(streaming:true),effects:CombatEffects,batches:MetalBatches
        var units=[String:Obj](),billboards=[String:UnitBillboard](),bars=[String:UnitBars](),rings=[String:SCNNode]()
        var state:Obj=[:],fog=[Int:UInt8](),creep=Set<Int>(),session="",eventSequence=0.0,generation=0
        var terrainRevision=0,fogRevision=0
        private var terrainDirty=true,lastCenter:SIMD2<Int>?,lastRadius=0
        private struct PickGeometry {
            let source:SCNGeometrySource,vertices:[SIMD3<Float>],indices:[UInt32],lower:SIMD3<Float>,upper:SIMD3<Float>
        }
        private let pickGeometry=ResidentCache<PickGeometry>(limit:64*1024*1024)
        init(device:MTLDevice)throws {terrain=try TerrainWorker.Builder();effects=try CombatEffects(authoredModels:false);batches=try MetalBatches(device:device)}
        func apply(_ update:Update) {
            let next=update.state["session"] as? String ?? "legacy",reset=next != session
            let prepared=PreparedState(update.state,previous:state)
            if reset {billboards.removeAll();bars.removeAll();rings.removeAll();effects.clear();sprites.clear();eventSequence=0}
            let events=reset ? []:(update.state["events"] as? [Obj] ?? []).filter{number($0,"seq")>eventSequence}
            terrainDirty = terrainDirty || reset || fog != update.fog || creep != update.creep
            session=next;state=update.state;fog=update.fog;creep=update.creep
            eventSequence=number(state,"eventSeq");units=prepared.units
            for id in billboards.keys where units[id]==nil {billboards.removeValue(forKey:id);bars.removeValue(forKey:id);rings.removeValue(forKey:id)}
            let colors=state["playerColors"] as? [[Int]] ?? []
            for (id,unit) in units where prepared.dirty.contains(id) || billboards[id]==nil {
                let billboard=billboards[id] ?? UnitBillboard(id:id)
                billboard.apply(unit,ground:terrain.ground,art:sprites,color:WorldSprites.playerColor(colors,owner:Int(number(unit,"owner"))))
                billboards[id]=billboard
            }
            effects.apply(bullets:state["bullets"] as? [Obj] ?? [],events:events,units:units,frame:number(state,"frame"),ground:terrain.ground)
            sprites.apply(state,ground:terrain.ground,authored:[])
        }
        func packet(_ view:SceneView)throws->ScenePacket {
            if terrainDirty || lastCenter != view.center || lastRadius != view.radius {
                let terrainResult=terrain.apply(TerrainWorker.Request(session:session,center:view.center,radius:view.radius,fog:fog,creep:creep))
                guard terrainResult.error.isEmpty else {terrainDirty=true;throw SceneError.invalid(terrainResult.error)}
                if !terrainResult.terrain.isEmpty || !terrainResult.removedTerrain.isEmpty {terrainRevision &+= 1;fogRevision &+= 1}
                terrainDirty=false;lastCenter=view.center;lastRadius=view.radius
            }
            let column=view.camera.transform.columns.3,eye=SIMD3<Float>(column.x,column.y,column.z)
            for billboard in billboards.values {billboard.updateView(eye:eye,orientation:view.camera.orientation,perspective:!view.camera.orthographic,art:sprites)}
            var overlays=Array(terrain.creepNodes.values)+Array(sprites.nodes.values)
            func collect(_ node:SCNNode,hidden:Bool=false) {
                if hidden || node.isHidden {return}
                if node.geometry != nil {overlays.append(node)}
                for child in node.childNodes {collect(child)}
            }
            collect(effects.root)
            for (id,billboard) in billboards where view.selected.contains(id) && !billboard.root.isHidden {
                guard let unit=units[id] else {continue}
                let bar=bars[id] ?? UnitBars();bars[id]=bar;bar.apply(unit)
                bar.root.simdPosition=billboard.root.simdPosition+SIMD3<Float>(0,1.2,0)
                bar.root.simdOrientation=view.camera.orientation;bar.root.constraints=nil
                collect(bar.root)
                let ring:SCNNode
                if let old=rings[id] {ring=old}
                else {
                    let geometry=SCNTorus(ringRadius:0.45,pipeRadius:0.018),material=SCNMaterial()
                    material.lightingModel = .constant;material.diffuse.contents=NSColor.green;geometry.materials=[material]
                    ring=SCNNode(geometry:geometry);rings[id]=ring
                }
                ring.simdPosition=billboard.root.simdPosition+SIMD3<Float>(0,0.035,0);overlays.append(ring)
            }
            overlays += view.overlays.map{$0.node()}
            overlays.sort{$0.renderingOrder<$1.renderingOrder}
            generation &+= 1
            try batches.prepare(MetalBatches.Input(sceneRevision:generation,terrainRevision:terrainRevision,terrainFogRevision:fogRevision,mapWidth:terrain.mapWidth,
                terrain:terrain.terrain,billboards:Array(billboards.values),overlays:overlays,objectCount:units.count))
            var picks=[ScenePickMesh](),positions=[String:SIMD3<Float>]()
            func pick(_ node:SCNNode,_ target:ScenePickHit.Target) {
                guard !node.isHidden,let geometry=node.geometry,let source=geometry.sources(for:.vertex).first else {return}
                let key=String(describing:ObjectIdentifier(source)),packed:PickGeometry
                if let cached=pickGeometry.get(key),cached.source === source {packed=cached}
                else {
                    let (vertices,indices)=batches.mesh(geometry)
                    guard !indices.isEmpty else {return}
                    let points=vertices.map{SIMD3<Float>($0.position.x,$0.position.y,$0.position.z)}
                    let lower=points.reduce(SIMD3<Float>(repeating:.greatestFiniteMagnitude)){simd_min($0,$1)}
                    let upper=points.reduce(SIMD3<Float>(repeating: -.greatestFiniteMagnitude)){simd_max($0,$1)}
                    packed=PickGeometry(source:source,vertices:points,indices:indices,lower:lower,upper:upper)
                    pickGeometry.insert(packed,key:key,bytes:points.count*MemoryLayout<SIMD3<Float>>.stride+indices.count*4)
                }
                picks.append(ScenePickMesh(target:target,transform:node.simdWorldTransform,vertices:packed.vertices,indices:packed.indices,lower:packed.lower,upper:packed.upper))
            }
            for (id,billboard) in billboards where !billboard.root.isHidden {
                positions[id]=billboard.root.simdWorldPosition
                for (index,node) in billboard.layers where number(billboard.nativeLayers[index] ?? [:],"drawfunc") != 10 {pick(node,.unit(id))}
            }
            for node in terrain.terrain.values {pick(node,.terrain)}
            for node in overlays {if let name=node.name,name.hasPrefix("card-"),let index=Int(name.dropFirst(5)) {pick(node,.card(index))}}
            let diagnostics:Obj=["billboardUnits":billboards.mapValues{$0.evidence},"nativeDeathFrames":sprites.deathFrames,"nativeLoneFrames":sprites.loneFrames,
                "spriteBounds":billboards.mapValues{$0.projectedLayers(camera:view.camera,viewport:view.viewport,art:sprites)},"frame":number(state,"frame"),"nativeFrame":number(state,"frame"),
                "camera":[column.x,column.y,column.z],"viewHeight":view.camera.orthographic ? 1/view.camera.projection.columns.1.y:0,
                "state":state,"fog":Dictionary(uniqueKeysWithValues:fog.map{(String($0.key),Int($0.value))}),"creepTilesSnapshot":Array(creep).sorted(),
                "missingWorldImages":sprites.missing.sorted(),"spriteFrames":effects.sprites.renderedFrames,"missingEffectImages":effects.sprites.missing.sorted(),
                "combatEvents":effects.counts,"projectiles":effects.projectiles.count,"creepTiles":terrain.creepNodes.count,"terrainTiles":terrain.terrain.count,
                "capability":"native HD relief and sprite chains; authored rigs require SOJU_BW_RENDERER=scenekit"]
            return ScenePacket(generation:generation,session:session,batches:batches.batches,picks:picks,nativeFrame:number(state,"frame"),publishedAt:CACurrentMediaTime(),unitPositions:positions,diagnostics:diagnostics)
        }
    }
}
