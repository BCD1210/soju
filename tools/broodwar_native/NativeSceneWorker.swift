import Cocoa
import SceneKit
import Metal
import simd

final class NativeSceneWorker {
    enum RefinementYield:Error {case nativePending}
    struct Update {let state:Obj,fog:[Int:UInt8],creep:Set<Int>;var prepared:PreparedState?=nil;var region:Obj?=nil}
    let mailbox=SceneMailbox()
    private let queue=DispatchQueue(label:"soju.bw.scene-owner",qos:.userInitiated)
    private let viewLock=NSLock()
    private var latestView:SceneView?,viewScheduled=false
    private var pendingNative=0,viewDeferred=false
    private var resourceRevision=0
    private var publishedView:SceneView?,publishedResources = -1
    private var builder:Builder?
    private let device:MTLDevice
    init(device:MTLDevice) {self.device=device}
    // SceneKit's main-run-loop flush also touches lazily materialized sources.
    // Commit construction under its global lock before publishing Metal values.
    private func construct(_ body:()->Void) {
        SCNTransaction.lock();SCNTransaction.begin();SCNTransaction.disableActions=true
        autoreleasepool(invoking:body)
        SCNTransaction.commit();SCNTransaction.unlock()
    }
    // SceneKit retires replaced material contents on its main-thread flush.
    // Release the construction lock for that flush only at a completed upload
    // boundary; render packets remain immutable and the frame owner never waits.
    private static func retireTerrainSources() {
        dispatchPrecondition(condition:.notOnQueue(.main))
        SCNTransaction.commit();SCNTransaction.unlock()
        DispatchQueue.main.sync {autoreleasepool {SCNTransaction.flush()}}
        SCNTransaction.lock();SCNTransaction.begin();SCNTransaction.disableActions=true
    }
    // Admit the next bridge read when this update starts, overlapping it with
    // construction. The bridge holds admission until then, bounding the FIFO
    // to one pending native update without dropping state or event deltas.
    func submit(_ update:Update,readNext:@escaping()->Void={},completion:@escaping()->Void) {
        mailbox.invalidate(update.state["session"] as? String ?? "legacy")
        viewLock.lock();pendingNative+=1;viewLock.unlock()
        queue.async { [self] in
            viewLock.lock();pendingNative-=1;viewLock.unlock()
            DispatchQueue.main.async(execute:readNext)
            construct {
                do {
                    if builder==nil {
                        builder=try Builder(device:device)
                        builder!.batches.retireSourceOwnership=Self.retireTerrainSources
                        builder!.sprites.stream?.onReady={ [weak self] in self?.resourcesReady() }
                    }
                    builder!.workerStartedAt=CACurrentMediaTime()
                    builder!.apply(update)
                    publish(includeTerrain:false)
                } catch {mailbox.fail(String(describing:error));fputs("Native scene worker: \(error)\n",stderr)}
            }
            DispatchQueue.main.async(execute:completion)
            if builder?.terrainDirty == true {scheduleView()}
        }
    }
    func view(_ view:SceneView) {
        viewLock.lock()
        let changed=latestView.map{!$0.equivalent(to:view)} ?? true
        latestView=view;viewLock.unlock()
        if changed {scheduleView()}
    }
    private func resourcesReady() {
        viewLock.lock();resourceRevision &+= 1;viewLock.unlock()
        scheduleView()
    }
    private func scheduleView() {
        viewLock.lock()
        if viewScheduled {viewLock.unlock();return}
        viewScheduled=true;viewLock.unlock()
        queue.async { [self] in
            viewLock.lock();viewScheduled=false
            // One queued native update may replace a view-only publication.
            // Never defer twice, so sustained input cannot starve terrain work.
            let deferView=pendingNative>0 && !viewDeferred
            viewDeferred=deferView;viewLock.unlock()
            if !deferView {construct {publish()}}
        }
    }
    private func publish(includeTerrain:Bool=true) {
        viewLock.lock();let view=latestView,resources=resourceRevision;viewLock.unlock()
        guard let builder=builder,let view=view,view.session==builder.session else {return}
        if includeTerrain,publishedView?.equivalent(to:view)==true,publishedResources==resources,!builder.needsTerrain(view) {return}
        do {
            mailbox.publish(try builder.packet(view,includeTerrain:includeTerrain || builder.terrain.session != builder.session,yieldToNative:{ [self] in
                guard includeTerrain else {return false}
                viewLock.lock();defer{viewLock.unlock()}
                return pendingNative>0
            }))
            publishedView=view;publishedResources=resources
        }
        catch RefinementYield.nativePending {return}
        catch {mailbox.fail(String(describing:error));fputs("Native scene packet: \(error)\n",stderr)}
    }
    final class Builder {
        let terrain:TerrainWorker.Builder,sprites=WorldSprites(streaming:true),effects:CombatEffects,batches:MetalBatches
        private let overlayCache=SceneOverlayCache()
        let modelLibrary:Models3D
        var rigAssets=[Int:MetalRigAsset](),rigs=[String:MetalRigInstance](),retiringRigs=[String:MetalRigInstance]()
        var rigErrors=[Int:String](),approvedRigKinds=Set<Int>()
        var units=[String:Obj](),billboards=[String:UnitBillboard](),bars=[String:UnitBars](),rings=[String:SCNNode]()
        var state:Obj=[:],fog=[Int:UInt8](),creep=Set<Int>(),session="",eventSequence=0.0,generation=0
        private var diagnosticFog=[String:Int](),diagnosticCreep=[Int]()
        private struct Region:Equatable {
            let x:Int,y:Int,width:Int,fog:String,creep:String
            init(_ value:Obj) {
                x=Int(number(value,"x"));y=Int(number(value,"y"));width=Int(number(value,"width"))
                fog=value["fog"] as? String ?? "";creep=value["creep"] as? String ?? ""
            }
        }
        private var lastRegion:Region?
        var terrainRevision=0,fogRevision=0
        var workerStartedAt=0.0
        var terrainDirty=true
        private var lastCenter:SIMD2<Int>?,lastRadius=0
        private struct PickGeometry {
            let source:SCNGeometrySource,vertices:[SIMD3<Float>],indices:[UInt32],lower:SIMD3<Float>,upper:SIMD3<Float>
        }
        private let pickGeometry=ResidentCache<PickGeometry>(limit:64*1024*1024)
        private var terrainPicks=[ScenePickMesh](),pickTerrainRevision = -1
        init(device:MTLDevice)throws {
            terrain=try TerrainWorker.Builder();effects=try CombatEffects(authoredModels:false);batches=try MetalBatches(device:device)
            modelLibrary=try Models3D(folder:argument("--model-dir").map{URL(fileURLWithPath:$0)} ?? runtime.appendingPathComponent("models-3d"),catalogURL:argument("--catalog").map{URL(fileURLWithPath:$0)})
            approvedRigKinds=Set(modelLibrary.fidelity.keys.filter{modelLibrary.approvedForLive($0)})
            if let review=argument("--model-review-types") {approvedRigKinds.formUnion(review.split(separator:",").compactMap{Int($0)})}
        }
        func apply(_ update:Update) {
            let next=update.state["session"] as? String ?? "legacy",reset=next != session
            let prepared=update.prepared ?? PreparedState(update.state,previous:state)
            if reset {rigs.removeAll();retiringRigs.removeAll();billboards.removeAll();bars.removeAll();rings.removeAll();effects.clear();sprites.clear();eventSequence=0}
            let events=reset ? []:(update.state["events"] as? [Obj] ?? []).filter{number($0,"seq")>eventSequence}
            var incomingFog=update.fog,incomingCreep=update.creep
            if let region=update.region {
                incomingFog=reset ? [:]:fog;incomingCreep=reset ? []:creep
                let region=Region(region)
                if region.width>0 && (reset || region != lastRegion) {
                    for (offset,value) in region.fog.utf8.enumerated() {
                        incomingFog[(region.y+offset/region.width)*terrain.mapWidth+region.x+offset%region.width]=value
                    }
                    for (offset,value) in region.creep.utf8.enumerated() {
                        let index=(region.y+offset/region.width)*terrain.mapWidth+region.x+offset%region.width
                        if value==49 {incomingCreep.insert(index)}else {incomingCreep.remove(index)}
                    }
                }
                lastRegion=region
            } else {lastRegion=nil}
            terrainDirty = terrainDirty || reset || fog != incomingFog || creep != incomingCreep
            if reset || fog != incomingFog {diagnosticFog=Dictionary(uniqueKeysWithValues:incomingFog.map{(String($0.key),Int($0.value))})}
            if reset || creep != incomingCreep {diagnosticCreep=Array(incomingCreep).sorted()}
            session=next;state=update.state;fog=incomingFog;creep=incomingCreep
            eventSequence=number(state,"eventSeq");units=prepared.units
            for id in billboards.keys where units[id]==nil {billboards.removeValue(forKey:id);bars.removeValue(forKey:id);rings.removeValue(forKey:id)}
            let colors=state["playerColors"] as? [[Int]] ?? []
            for event in events where event["kind"] as? String == "death" {
                if let id=event["unit"] as? String,units[id]==nil,let rig=rigs.removeValue(forKey:id),rig.model.clip(for:"death") != nil {
                    rig.model.beginDeath(frame:number(state,"frame"));retiringRigs[id]=rig
                }
            }
            for id in rigs.keys where units[id]==nil {rigs.removeValue(forKey:id)}
            for (id,rig) in retiringRigs where rig.model.updateDeath(frame:number(state,"frame"),now:CACurrentMediaTime()) {retiringRigs.removeValue(forKey:id)}
            for (id,unit) in units {
                let kind=Int(number(unit,"type"))
                guard approvedRigKinds.contains(kind) else {continue}
                do {
                    let rig:MetalRigInstance
                    if let existing=rigs[id],existing.model.kind==kind {rig=existing}
                    else {
                        let model=try modelLibrary.make(kind);model.root.name=id
                        let asset:MetalRigAsset
                        if let cached=rigAssets[kind] {asset=cached}
                        else {asset=try MetalRigAsset(url:modelLibrary.catalog.url(model.definition),definition:model.definition,batches:batches);rigAssets[kind]=asset}
                        rig=MetalRigInstance(model:model,asset:asset);rigs[id]=rig
                    }
                    for event in events where event["kind"] as? String == "attack" && event["unit"] as? String == id {rig.model.attackUntilFrame=number(state,"frame")+5}
                    rig.model.apply(unit,height:terrain.ground.buildingElevation(unit),frame:number(state,"frame"))
                    let turret=unit["turret"] as? Obj ?? [:]
                    rig.turretYaw=Float(-(number(turret,"facing")-number(unit,"facing"))*2*Double.pi/256)
                    rig.model.root.simdPosition=rig.model.targetPosition;rig.model.root.eulerAngles.y=CGFloat(rig.model.targetYaw)
                    rig.model.teamColor(WorldSprites.playerColor(colors,owner:Int(number(unit,"owner"))));rigErrors.removeValue(forKey:kind)
                } catch {rigErrors[kind]=String(describing:error);rigs.removeValue(forKey:id)}
            }
            for (id,unit) in units where prepared.dirty.contains(id) || billboards[id]==nil {
                let billboard=billboards[id] ?? UnitBillboard(id:id)
                billboard.apply(unit,ground:terrain.ground,art:sprites,color:WorldSprites.playerColor(colors,owner:Int(number(unit,"owner"))))
                billboards[id]=billboard
            }
            effects.apply(bullets:state["bullets"] as? [Obj] ?? [],events:events,units:units,frame:number(state,"frame"),ground:terrain.ground)
            sprites.apply(state,ground:terrain.ground,authored:Set(retiringRigs.keys))
        }
        private func trimTextureCaches() {
            // Drop only cache ownership before a large reveal competes with live
            // terrain, animation and wrist textures for the shared budget.
            if number(TextureResidency.shared.report(),"residentBytes")>Double(TextureResidency.shared.limit)*0.75 {
                let retired=terrain.art.textures.trim(to:16*1024*1024)+batches.textures.trim(to:8*1024*1024)
                sprites.stream?.trim(to:96*1024*1024)
                if retired>0 {batches.retireSourceOwnership?()}
            }
        }
        func needsTerrain(_ view:SceneView)->Bool {
            terrainDirty || lastCenter != view.center || lastRadius != view.radius || terrain.session != session
        }
        func packet(_ view:SceneView,includeTerrain:Bool=true,yieldToNative:()->Bool={false})throws->ScenePacket {
            trimTextureCaches()
            if includeTerrain && needsTerrain(view) {
                let terrainResult=terrain.apply(TerrainWorker.Request(session:session,center:view.center,radius:view.radius,fog:fog,creep:creep))
                guard terrainResult.error.isEmpty else {terrainDirty=true;throw SceneError.invalid(terrainResult.error)}
                if !terrainResult.terrain.isEmpty || !terrainResult.removedTerrain.isEmpty {terrainRevision &+= 1;fogRevision &+= 1}
                terrainDirty=false;lastCenter=view.center;lastRadius=view.radius
            }
            trimTextureCaches()
            if yieldToNative() {throw RefinementYield.nativePending}
            let column=view.camera.transform.columns.3,eye=SIMD3<Float>(column.x,column.y,column.z)
            for billboard in billboards.values {billboard.updateView(eye:eye,orientation:view.camera.orientation,perspective:!view.camera.orthographic,art:sprites)}
            let hiddenBodies=PossessionPresentation.hiddenBodies(Array(billboards.values),eye:eye,possessed:view.hiddenUnit)
            var rigDraws=[MetalRigDraw](),rigPicks=[ScenePickMesh]()
            for (id,rig) in rigs.merging(retiringRigs,uniquingKeysWith:{$1}) {
                let pose=rig.pose(frame:number(state,"frame"),batches:batches,hidden:hiddenBodies.contains(id))
                rigDraws+=pose.0;rigPicks+=pose.1
            }
            var overlays=Array(terrain.creepNodes.values)+Array(sprites.nodes.values)
            func collect(_ node:SCNNode,hidden:Bool=false) {
                if hidden || node.isHidden {return}
                if node.geometry != nil {overlays.append(node)}
                for child in node.childNodes {collect(child)}
            }
            collect(effects.root)
            for (id,billboard) in billboards where !hiddenBodies.contains(id) && view.selected.contains(id) && !billboard.root.isHidden {
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
            overlays += overlayCache.nodes(view.overlays)
            overlays.sort{$0.renderingOrder<$1.renderingOrder}
            if yieldToNative() {throw RefinementYield.nativePending}
            let nextGeneration=generation &+ 1
            try batches.prepare(MetalBatches.Input(sceneRevision:nextGeneration,terrainRevision:terrainRevision,terrainFogRevision:fogRevision,mapWidth:terrain.mapWidth,
                terrain:terrain.terrain,billboards:Array(billboards.values),overlays:overlays,objectCount:units.count,retireTerrainSources:true,reuseUnitPoses:true,hiddenBodies:hiddenBodies.union(rigs.keys),rigs:rigDraws),checkpoint:{
                    if yieldToNative() {throw RefinementYield.nativePending}
                })
            var picks=rigPicks,positions=[String:SIMD3<Float>]()
            func pick(_ node:SCNNode,_ target:ScenePickHit.Target,sharedKey:String?=nil,relief:SpriteRelief.Mesh?=nil) {
                guard !node.isHidden,let geometry=node.geometry,let source=geometry.sources(for:.vertex).first else {return}
                let key=sharedKey ?? String(describing:ObjectIdentifier(source)),packed:PickGeometry
                if let cached=pickGeometry.get(key),sharedKey != nil || cached.source === source {packed=cached}
                else {
                    let points:[SIMD3<Float>],indices:[UInt32]
                    if let relief=relief {
                        points=relief.vertices.map{SIMD3<Float>($0)};indices=relief.indices.map{UInt32($0)}
                    } else {
                        let mesh=batches.mesh(geometry)
                        points=mesh.0.map{SIMD3<Float>($0.position.x,$0.position.y,$0.position.z)};indices=mesh.1
                    }
                    guard !indices.isEmpty else {return}
                    let lower=points.reduce(SIMD3<Float>(repeating:.greatestFiniteMagnitude)){simd_min($0,$1)}
                    let upper=points.reduce(SIMD3<Float>(repeating: -.greatestFiniteMagnitude)){simd_max($0,$1)}
                    packed=PickGeometry(source:source,vertices:points,indices:indices,lower:lower,upper:upper)
                    pickGeometry.insert(packed,key:key,bytes:points.count*MemoryLayout<SIMD3<Float>>.stride+indices.count*4)
                }
                picks.append(ScenePickMesh(target:target,transform:node.simdWorldTransform,vertices:packed.vertices,indices:packed.indices,lower:packed.lower,upper:packed.upper))
            }
            for (id,billboard) in billboards where !billboard.root.isHidden {
                if yieldToNative() {throw RefinementYield.nativePending}
                positions[id]=billboard.root.simdWorldPosition
                if rigs[id] != nil {continue}
                for (index,node) in billboard.layers where number(billboard.nativeLayers[index] ?? [:],"drawfunc") != 10 {
                    let shared=billboard.contactMeshes[index]==nil && billboard.sourceMeshes[index] != nil
                    let key=shared ? "unit/\(billboard.shownImages[index] ?? -1)/\(billboard.shown[index] ?? -1)":nil
                    let relief=node.geometry is SCNPlane ? nil:billboard.contactMeshes[index] ?? billboard.sourceMeshes[index]
                    pick(node,.unit(id),sharedKey:key,relief:relief)
                }
            }
            if pickTerrainRevision != terrainRevision {
                let start=picks.count
                for node in terrain.terrain.values {pick(node,.terrain)}
                terrainPicks=Array(picks[start...]);picks.removeSubrange(start...);pickTerrainRevision=terrainRevision
            }
            picks.append(contentsOf:terrainPicks)
            for node in overlays {if let name=node.name,name.hasPrefix("card-"),let index=Int(name.dropFirst(5)) {pick(node,.card(index))}}
            let diagnostics:Obj=["billboardUnits":billboards.mapValues{$0.evidence},"nativeDeathFrames":sprites.deathFrames,"nativeLoneFrames":sprites.loneFrames,
                "frame":number(state,"frame"),"nativeFrame":number(state,"frame"),
                "camera":[column.x,column.y,column.z],"viewHeight":view.camera.orthographic ? 1/view.camera.projection.columns.1.y:0,
                "nativeAt":number(state,"nativeAt"),"bridgeReadAt":number(state,"bridgeReadAt"),"bridgeStartedAt":number(state,"bridgeStartedAt"),"workerStartedAt":workerStartedAt,
                "state":state,"fog":diagnosticFog,"creepTilesSnapshot":diagnosticCreep,
                "missingWorldImages":sprites.missing.sorted(),"spriteFrames":effects.sprites.renderedFrames,"missingEffectImages":effects.sprites.missing.sorted(),
                "combatEvents":effects.counts,"projectiles":effects.projectiles.count,"creepTiles":terrain.creepNodes.count,"terrainTiles":terrain.terrain.count,
                "capability":"native Metal terrain, relief and authored GPU-skinned rigs",
                "riggedUnits":rigs.mapValues{["type":$0.model.kind,"clip":$0.model.playing,"targetHeight":$0.model.definition.targetHeight ?? 0]},
                "rigErrors":Dictionary(uniqueKeysWithValues:rigErrors.map{(String($0.key),$0.value)}),
                "rigDraws":rigDraws.count,"retiringRigs":retiringRigs.count,"textureResidency":TextureResidency.shared.report()]
            let projection=view.camera.projection*simd_inverse(view.camera.transform)
            let bounds=billboards.mapValues{$0.projectedSpriteBounds(projection:projection,viewport:view.viewport,art:sprites)}
            if yieldToNative() {throw RefinementYield.nativePending}
            generation=nextGeneration
            var packet=ScenePacket(generation:generation,session:session,batches:batches.batches,picks:picks,nativeFrame:number(state,"frame"),publishedAt:CACurrentMediaTime(),unitPositions:positions,diagnostics:diagnostics,spriteBounds:bounds)
            packet.timing=SceneTiming(native:number(state,"nativeAt"),bridgeStarted:number(state,"bridgeStartedAt"),bridgeRead:number(state,"bridgeReadAt"),workerStarted:workerStartedAt,published:packet.publishedAt)
            packet.timing.bridgeWireRead=number(state,"bridgeWireReadAt")
            return packet
        }
    }
}
