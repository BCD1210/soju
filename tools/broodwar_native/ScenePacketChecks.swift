import Cocoa
import Metal
import SceneKit
import simd

func scenePacketChecks(device:MTLDevice,require:(Bool,String)throws->Void)throws {
    try require(!ScenePickHit.Target.unit("blocking").accepts(command:.ground) && ScenePickHit.Target.terrain.accepts(command:.ground),"ground targeting ignores intervening unit geometry")
    try require(!ScenePickHit.Target.terrain.accepts(command:.unit) && ScenePickHit.Target.unit("target").accepts(command:.unit),"unit targeting excludes terrain")
    try require(ScenePickHit.Target.card(-3).accepts(command:.ground) && ScenePickHit.Target.unit("rally").accepts(command:.rally),"pending targets retain cancel cards and unit rally targets")
    let raster=WristMapRaster(),fog:[Int:UInt8]=[0:50,1:48,2:49]
    let minimap=raster.image(width:2,height:2,fog:fog)
    try require(minimap === raster.image(width:2,height:2,fog:fog),"unchanged minimap fog retains the raster image")
    let bitmap=minimap.representations[0] as! NSBitmapImageRep
    try require(bitmap.colorAt(x:0,y:0)!.alphaComponent==1 && bitmap.colorAt(x:1,y:0)!.alphaComponent==0 && bitmap.colorAt(x:0,y:1)!.redComponent<bitmap.colorAt(x:0,y:0)!.redComponent,"minimap preserves visible, unseen and explored tile states in native row order")
    let changedMap=raster.image(width:2,height:2,fog:[0:49])
    try require(changedMap !== minimap,"changed minimap fog replaces the immutable raster")
    let imageCache=SceneOverlayImageCache();imageCache.begin()
    let image=imageCache.capture(minimap);imageCache.end();imageCache.begin()
    try require(image != nil && image === imageCache.capture(minimap),"pose-only captures retain converted panel image identity")
    _=imageCache.capture(changedMap);imageCache.end();imageCache.begin();_=imageCache.capture(changedMap);imageCache.end()
    try require(imageCache.count==1,"panel image cache retires unused images")

    let overlayCache=SceneOverlayCache()
    func overlay(_ x:Float,_ color:SIMD4<Float>)->SceneOverlay {
        var transform=matrix_identity_float4x4;transform.columns.3.x=x
        return SceneOverlay(transform:transform,vertices:[SCNVector3(0,0,0),SCNVector3(1,0,0),SCNVector3(0,1,0)],uv:[],indices:[0,1,2],image:nil,color:color,opacity:1,depth:false,order:1,line:false,name:"card-0")
    }
    let originalOverlay=overlayCache.nodes([overlay(0,SIMD4<Float>(1,1,1,1))])[0]
    let movedOverlay=overlayCache.nodes([overlay(2,SIMD4<Float>(1,1,1,1))])[0]
    try require(originalOverlay === movedOverlay && movedOverlay.simdPosition.x==2,"tracked overlay pose reuses worker geometry and material")
    let changedOverlay=overlayCache.nodes([overlay(2,SIMD4<Float>(1,0,0,1))])[0]
    try require(changedOverlay !== originalOverlay,"overlay content change replaces worker material")
    try require(overlayCache.nodes([]).isEmpty,"removed overlays release worker cache entries")
    let overlayBatches=try MetalBatches(device:device)
    func prepareOverlay(_ node:SCNNode)throws->MetalBatches.PacketBatch {
        try overlayBatches.prepare(MetalBatches.Input(sceneRevision:1,terrainRevision:0,terrainFogRevision:0,mapWidth:1,terrain:[:],billboards:[],overlays:[node],objectCount:0))
        return overlayBatches.batches[0]
    }
    let overlayFirst=try prepareOverlay(movedOverlay)
    movedOverlay.simdPosition.x=4;movedOverlay.opacity=0.5
    let overlayNext=try prepareOverlay(movedOverlay)
    try require(overlayFirst.vertices === overlayNext.vertices && overlayFirst.instances[0] === overlayNext.instances[0] && overlayNext.drawNodes[0].transform.columns.3.x==4 && overlayNext.drawNodes[0].opacity==0.5,"tracked overlay motion reuses GPU geometry while publishing fresh transforms and opacity")
    try require(overlayFirst.drawNodes[0].transform.columns.3.x==2 && overlayFirst.drawNodes[0].opacity==1,"overlay GPU reuse preserves the prior packet's pose")
    movedOverlay.geometry!.firstMaterial!.diffuse.contentsTransform=SCNMatrix4MakeScale(2,2,1)
    let changedUV=try prepareOverlay(movedOverlay)
    try require(changedUV.vertices !== overlayNext.vertices,"overlay UV changes invalidate uploaded geometry")
    try require(SceneOverlay.tint(NSImage(size:NSSize(width:2,height:2))) == SIMD4<Float>(repeating:1),"textured wrist overlays use white RGB tint without querying grayscale components")
    try require(SceneOverlay.tint(NSColor(calibratedWhite:0.5,alpha:0.25)).w == 0.25,"overlay grayscale colors convert to RGB while preserving alpha")
    if case .color(let fallback)=FrameStream.Material.Map(nil) {
        try require(fallback == SIMD4<Float>(0,0,0,1),"missing material maps use explicit RGB black")
    } else {try require(false,"missing material maps use a color")}
    let vertices=[SIMD3<Float>(-1,-1,0),SIMD3<Float>(1,-1,0),SIMD3<Float>(0,1,0)]
    func mesh(_ target:ScenePickHit.Target,_ z:Float)->ScenePickMesh {
        var transform=matrix_identity_float4x4;transform.columns.3.z=z
        return ScenePickMesh(target:target,transform:transform,vertices:vertices,indices:[0,1,2],lower:SIMD3<Float>(-1,-1,0),upper:SIMD3<Float>(1,1,0))
    }
    func packet(_ generation:Int,_ session:String,_ picks:[ScenePickMesh])->ScenePacket {
        ScenePacket(generation:generation,session:session,batches:[],picks:picks,nativeFrame:Double(generation),publishedAt:0,unitPositions:[:],diagnostics:[:])
    }
    let pickPacket=packet(1,"pick",[mesh(.card(2),1),mesh(.unit("far"),-1),mesh(.unit("near"),0)])
    let projected=SceneSpriteBounds(image:239,frame:68,shadow:false,left:1,top:2,right:3,bottom:4)
    let diagnosticPacket=ScenePacket(generation:1,session:"diagnostic",batches:[],picks:pickPacket.picks,nativeFrame:5,publishedAt:0,unitPositions:[:],diagnostics:["frame":5],spriteBounds:["near":[projected]])
    let decoded=try JSONSerialization.jsonObject(with:JSONSerialization.data(withJSONObject:diagnosticPacket.diagnostics)) as! Obj
    try require(number(decoded,"frame")==5 && ((decoded["spriteBounds"] as? [String:[Obj]])?["near"]?.first).map{number($0,"frame")==68 && number($0,"left")==1}==true,"deferred diagnostics serialize immutable native frame and projected sprite values")
    try require((decoded["unitPickBounds"] as? [Obj])?.count==2 && (decoded["cardPickBounds"] as? [Obj])?.count==1,"deferred diagnostics retain unit and card picking bounds")
    try require(WorldSprites.colorKey([1,23,4]) != WorldSprites.colorKey([12,3,4]) && WorldSprites.colorKey([]) != WorldSprites.colorKey([0,0,0]),"texture keys preserve channel boundaries and untinted identity")
    let filtered=pickPacket.hit(origin:SIMD3<Float>(0,0,3),direction:SIMD3<Float>(0,0,-1),accept:{if case .unit=$0 {return true};return false})
    try require(filtered?.unitID=="near" && filtered?.distance==3,"pure picking filters eligibility before choosing nearest intersection")
    try require(pickPacket.hit(origin:SIMD3<Float>(3,0,3),direction:SIMD3<Float>(0,0,-1),accept:{_ in true})==nil,"pure picking rejects a ray outside triangle bounds")
    let empty=ScenePickMesh(target:.terrain,transform:matrix_identity_float4x4,vertices:[],indices:[],lower:.zero,upper:.zero)
    try require(empty.hit(origin:.zero,direction:SIMD3<Float>(0,0,-1),limit:10)==nil,"empty pick geometry has no intersection")
    let world=Terrain3D.worldPosition(8,2,10),position=SIMD3<Float>(Float(world.x),Float(world.y),Float(world.z))
    var elevated=matrix_identity_float4x4;elevated.columns.3=SIMD4<Float>(position,1)
    let ground=ScenePickMesh(target:.terrain,transform:elevated,vertices:vertices,indices:[0,1,2],lower:SIMD3<Float>(-1,-1,0),upper:SIMD3<Float>(1,1,0))
    let groundHit=ground.hit(origin:position+SIMD3<Float>(0,0,3),direction:SIMD3<Float>(0,0,-1),limit:10)!
    let native=Terrain3D.mapPosition(groundHit.worldCoordinates)
    try require(abs(native.x-8)<0.0001 && abs(native.z-10)<0.0001,"pure hit coordinates preserve oblique world to map elevation conversion")
    let mailbox=SceneMailbox();mailbox.invalidate("old");mailbox.publish(packet(1,"old",[]))
    let retained=mailbox.latest()!
    mailbox.invalidate("new");mailbox.publish(packet(2,"old",[]))
    try require(mailbox.latest()==nil && retained.session=="old","mailbox rejects late old-session publication without mutating retained packet")
    mailbox.publish(packet(3,"new",[]));mailbox.publish(packet(4,"new",[]))
    try require(mailbox.latest()?.generation==4,"mailbox exposes only the latest complete publication")
    final class ReleaseObservation {var released=false,onMain=false}
    final class PacketLease {
        let observation:ReleaseObservation
        init(_ observation:ReleaseObservation) {self.observation=observation}
        deinit {observation.released=true;observation.onMain=Thread.isMainThread}
    }
    let lifetime=SceneMailbox(),observation=ReleaseObservation(),consumer=DispatchQueue(label:"packet-check-consumer")
    lifetime.invalidate("first")
    lifetime.publish(ScenePacket(generation:1,session:"first",batches:[],picks:[],nativeFrame:1,publishedAt:0,unitPositions:[:],diagnostics:["lease":PacketLease(observation)]))
    var consumed:ScenePacket?
    consumer.sync{lifetime.acquire(into:&consumed)}
    lifetime.invalidate("second");consumer.sync{lifetime.acquire(into:&consumed)}
    try require(!observation.released,"invalidating the first displayed packet retains its resources past render-owner release")
    lifetime.publish(packet(2,"second",[]))
    try require(observation.released && observation.onMain,"publication owner retires invalidated packet resources away from the consumer queue")

    let worker=NativeSceneWorker(device:device)
    var transform=matrix_identity_float4x4;transform.columns.3=SIMD4<Float>(3,4,5,1)
    let camera=MetalBatches.CameraPacket(transform:transform,projection:matrix_identity_float4x4,orientation:simd_quatf(),orthographic:true)
    let line=SceneOverlay(transform:matrix_identity_float4x4,vertices:[SCNVector3(-1,0.8,0),SCNVector3(1,0.8,0)],uv:[],indices:[0,1],image:nil,color:SIMD4<Float>(0,1,0,1),opacity:1,depth:false,order:100,line:true,name:"target-ray")
    let card=SceneOverlay(transform:matrix_identity_float4x4,vertices:vertices.map{SCNVector3($0)},uv:[],indices:[0,1,2],image:nil,color:SIMD4<Float>(1,0,0,1),opacity:1,depth:false,order:100,line:false,name:"card-7")
    func view(_ session:String)->SceneView {SceneView(session:session,viewport:SIMD2<Float>(32,32),camera:camera,center:SIMD2<Int>(1,1),radius:20,selected:["native"],overlays:[line,card])}
    try require(view("same").equivalent(to:view("same")) && !view("same").equivalent(to:view("other")) && !line.equivalent(to:card),"view coalescing preserves session and overlay changes")
    let unit:Obj=["id":"native","x":100,"y":100,"type":7,"owner":0,"hp":40,"maxHP":40,"image":["id":239,"frame":68,"flags":0]]
    func state(_ frame:Int,_ events:[Obj],_ session:String="native-check")->Obj {
        ["session":session,"frame":frame,"units":[unit],"events":events,"eventSeq":events.map{number($0,"seq")}.max() ?? 0,
         "bullets":[["id":1,"weapon":0,"x":100,"y":100,"facing":0]]]
    }
    func wait(_ predicate:()->Bool)->Bool {
        let deadline=Date().addingTimeInterval(30)
        while !predicate() && Date()<deadline {RunLoop.current.run(until:Date().addingTimeInterval(0.01))}
        return predicate()
    }
    worker.view(view("native-check"))
    var completed=false
    worker.submit(NativeSceneWorker.Update(state:state(1,[]),fog:[:],creep:[])){completed=true}
    try require(wait{completed && worker.mailbox.latest()?.picks.contains(where:{$0.target == .unit("native")})==true},"resource readiness republishes native unit geometry without another state or view update")
    let first=worker.mailbox.latest()!,firstPosition=first.unitPositions["native"]!
    try require(first.picks.contains(where:{$0.target == .card(7)}) && first.batches.contains(where:{$0.primitive == .line && $0.depthDisabled}),"worker packet retains command-card picking and depth-disabled line overlays")
    try require(first.batches.contains(where:{$0.textures.first?.width==1 && !$0.depthDisabled && $0.count>0}),"worker packet contains native procedural projectile geometry")
    let attack:Obj=["kind":"attack","seq":1,"x":100,"y":100]
    completed=false;worker.submit(NativeSceneWorker.Update(state:state(2,[attack]),fog:[:],creep:[])){completed=true}
    try require(wait{completed && worker.mailbox.latest()?.nativeFrame==2},"ordered worker publishes the next native frame")
    completed=false;worker.submit(NativeSceneWorker.Update(state:state(3,[attack]),fog:[:],creep:[])){completed=true}
    try require(wait{completed && worker.mailbox.latest()?.nativeFrame==3},"replayed native event still advances worker frame")
    try require((worker.mailbox.latest()?.diagnostics["combatEvents"] as? [String:Int])?["attack"]==1,"worker applies native event sequence exactly once across state updates")
    try require(first.nativeFrame==1 && first.unitPositions["native"]==firstPosition,"later worker updates preserve retained immutable packets")
    completed=false;worker.submit(NativeSceneWorker.Update(state:state(4,[],"next-session"),fog:[:],creep:[])){completed=true}
    try require(wait{completed} && worker.mailbox.latest()==nil,"session reset waits for a matching view instead of publishing stale UI values")
    worker.view(view("next-session"))
    try require(wait{worker.mailbox.latest()?.session=="next-session"},"matching new-session view resumes publication")
    try require((worker.mailbox.latest()?.diagnostics["combatEvents"] as? [String:Int])?.isEmpty==true,"new worker session clears prior combat events")

    let boundedTerrain=try TerrainWorker.Builder()
    for center in [SIMD2<Int>(-1000,-1000),SIMD2<Int>(1000,1000)] {
        let result=boundedTerrain.apply(TerrainWorker.Request(session:"outside-map",center:center,radius:20,fog:[:],creep:[]))
        try require(result.terrain.isEmpty && result.error.isEmpty,"terrain camera outside either map edge produces an empty region")
    }

    let failedTerrain=try NativeSceneWorker.Builder(device:device)
    let terrainState:Obj=["session":"terrain-retry","frame":1,"units":[],"events":[]]
    failedTerrain.apply(NativeSceneWorker.Update(state:terrainState,fog:[:],creep:[]))
    var yielded=false
    do {_ = try failedTerrain.packet(view("terrain-retry"),yieldToNative:{true})}
    catch NativeSceneWorker.RefinementYield.nativePending {yielded=true}
    try require(yielded && failedTerrain.generation==0 && !failedTerrain.needsTerrain(view("terrain-retry")),"view refinement can yield after terrain preparation without publishing a stale native frame")
    let beforeFailure=try failedTerrain.packet(view("terrain-retry"))
    var yieldChecks=0
    do {_ = try failedTerrain.packet(view("terrain-retry"),yieldToNative:{yieldChecks+=1;return yieldChecks==2})}
    catch NativeSceneWorker.RefinementYield.nativePending {}
    try require(yieldChecks==2 && failedTerrain.generation==beforeFailure.generation,"a native arrival during view preparation cancels redundant batch construction")
    let tileKey=String(failedTerrain.terrain.mapTiles[0]),originalTile=failedTerrain.terrain.art.tileCatalog[tileKey]!
    failedTerrain.terrain.art.tileCatalog[tileKey]?["file"]="runtime-check-missing-terrain.dds"
    failedTerrain.apply(NativeSceneWorker.Update(state:terrainState,fog:[0:50],creep:[]))
    var terrainRejected=false
    do {_ = try failedTerrain.packet(view("terrain-retry"))}
    catch SceneError.invalid(let message) {terrainRejected=message.contains("Terrain:")}
    try require(terrainRejected && failedTerrain.generation==beforeFailure.generation,"terrain loading failure rejects publication of an incomplete scene packet")
    failedTerrain.terrain.art.tileCatalog[tileKey]=originalTile
    let recovered=try failedTerrain.packet(view("terrain-retry"))
    try require(recovered.picks.filter{$0.target == .terrain}.count==1,"failed terrain retries and recovers with unchanged fog and camera")

    let tiled=try MetalBatches(device:device)
    let tileA=failedTerrain.terrain.terrain[0]!,tileB=SCNNode(geometry:failedTerrain.terrain.terrain[0]!.geometry)
    tileB.position=SCNVector3(1,0,0)
    try tiled.prepare(MetalBatches.Input(sceneRevision:1,terrainRevision:1,terrainFogRevision:1,mapWidth:2,terrain:[0:tileA,1:tileB],billboards:[],overlays:[],objectCount:0,retireTerrainSources:true))
    let page=tiled.batches.first(where:{$0.terrain})!
    try require(page.textures[0].arrayLength==1 && page.textures[1].arrayLength==2,"repeated terrain shares diffuse storage while retaining independent fog slices")
    let boundTile=tileA.geometry!.firstMaterial!.diffuse.contents as! MTLTexture
    try require(boundTile.parent === page.textures[0] && boundTile.textureType == .type2D,"terrain scene nodes release upload textures and reference immutable array slices")
    try tiled.prepare(MetalBatches.Input(sceneRevision:2,terrainRevision:2,terrainFogRevision:2,mapWidth:2,terrain:[0:tileA,1:tileB],billboards:[],overlays:[],objectCount:0,retireTerrainSources:true))
    try require(tiled.batches.first(where:{$0.terrain})!.textures[0] === page.textures[0],"terrain slice bindings preserve page reuse on unchanged terrain")
    let packed=page.vertices.contents().bindMemory(to:MetalBatches.Vertex.self,capacity:page.vertices.length/MemoryLayout<MetalBatches.Vertex>.stride)
    let tileVertices=Array(UnsafeBufferPointer(start:packed,count:page.vertices.length/MemoryLayout<MetalBatches.Vertex>.stride))
    try require(Set(tileVertices.map{$0.uv.z})==[0] && Set(tileVertices.map{$0.uv.w})==[0,1],"terrain vertex indices distinguish shared artwork from per-tile visibility")
    var deltaState=terrainState;deltaState["frame"]=2
    failedTerrain.apply(NativeSceneWorker.Update(state:deltaState,fog:[:],creep:[]))
    let delta=try failedTerrain.packet(view("terrain-retry"),includeTerrain:false)
    try require(delta.nativeFrame==2 && delta.picks.contains(where:{$0.target == .terrain}),"native delta publishes before terrain removal rebuild")
    try require((recovered.diagnostics["fog"] as? [String:Int])==["0":50] && (delta.diagnostics["fog"] as? [String:Int])==[:],"fog diagnostic snapshots update on native changes without mutating retained packets")
    let refined=try failedTerrain.packet(view("terrain-retry"))
    try require(!refined.picks.contains(where:{$0.target == .terrain}) && delta.picks.contains(where:{$0.target == .terrain}),"terrain refinement retires picks without mutating the displayed delta")

    failedTerrain.apply(NativeSceneWorker.Update(state:deltaState,fog:[:],creep:[],region:["x":2,"y":3,"width":2,"fog":"2120","creep":"1001"]))
    let firstIndex=3*failedTerrain.terrain.mapWidth+2,secondRow=firstIndex+failedTerrain.terrain.mapWidth
    try require(failedTerrain.fog==[firstIndex:50,firstIndex+1:49,secondRow:50,secondRow+1:48] && failedTerrain.creep==[firstIndex,secondRow+1],"bridge regions map fog and creep into worker-owned map coordinates")
    failedTerrain.apply(NativeSceneWorker.Update(state:deltaState,fog:[:],creep:[],region:["x":2,"y":3,"width":1,"fog":"1","creep":"0"]))
    try require(failedTerrain.fog[firstIndex]==49 && failedTerrain.fog[secondRow]==50 && failedTerrain.creep==[secondRow+1],"successive bridge regions retain outside visibility and remove changed creep")
    var resetRegion=deltaState;resetRegion["session"]="region-reset"
    failedTerrain.apply(NativeSceneWorker.Update(state:resetRegion,fog:[:],creep:[],region:["x":0,"y":0,"width":1,"fog":"2","creep":"0"]))
    try require(failedTerrain.fog==[0:50] && failedTerrain.creep.isEmpty,"new native session clears accumulated bridge regions")

    #if SOJU_OPENXR
    let inputQueue=XRInputQueue()
    var allAccepted=true,motion=SojuXRInput()
    for i in 0..<1000 {motion.px=Float(i);allAccepted=inputQueue.submit(motion) && allAccepted}
    try require(allAccepted && inputQueue.take()?.px==999 && inputQueue.take()==nil,"XR continuous poses coalesce through a long UI stall without filling the event queue")
    var press=SojuXRInput();press.leftPressed=1;press.leftHeld=1
    var held=press;held.leftPressed=0
    var release=SojuXRInput();release.leftReleased=1
    _=inputQueue.submit(press)
    for i in 0..<1000 {held.px=Float(i);_ = inputQueue.submit(held)}
    _=inputQueue.submit(release)
    try require(inputQueue.take()?.leftPressed==1 && inputQueue.take()?.px==999 && inputQueue.take()?.leftReleased==1 && inputQueue.take()==nil,"XR pose coalescing retains ordered press held and release samples")
    var grip=SojuXRInput();grip.leftGripHeld=1
    _=inputQueue.submit(grip);_ = inputQueue.submit(SojuXRInput())
    try require(inputQueue.take()?.leftGripHeld==1 && inputQueue.take()?.leftGripHeld==0,"XR grip release remains an event even without a release bit")
    var turn=SojuXRInput();turn.turnX=1
    _=inputQueue.submit(turn);_ = inputQueue.submit(SojuXRInput());_ = inputQueue.submit(turn)
    try require(inputQueue.take()?.turnX==1 && inputQueue.take()?.turnX==0 && inputQueue.take()?.turnX==1 && inputQueue.take()==nil,"XR pose coalescing preserves snap-turn latch press reset and second press")
    let full=XRInputQueue()
    for _ in 0..<127 {_ = full.submit(press)}
    try require(!full.submit(release) && full.take()?.leftPressed==1,"XR edge queue reports overflow while preserving already queued commands")
    #endif

    let renderer=try MetalBatches(device:device),red=card.node()
    try renderer.prepare(MetalBatches.Input(sceneRevision:1,terrainRevision:1,terrainFogRevision:1,mapWidth:1,terrain:[:],billboards:[],overlays:[red,line.node()],objectCount:0))
    let descriptor=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.bgra8Unorm_srgb,width:32,height:32,mipmapped:false)
    descriptor.usage=[.renderTarget,.shaderRead];descriptor.storageMode = .private
    let output=device.makeTexture(descriptor:descriptor)!,readback=device.makeBuffer(length:32*256,options:.storageModeShared)!
    let command=renderer.queue.makeCommandBuffer()!
    var projection=matrix_identity_float4x4;projection.columns.2.z = -1
    var pose=matrix_identity_float4x4;pose.columns.3.z=0.5
    let renderCamera=MetalBatches.CameraPacket(transform:pose,projection:projection,orientation:simd_quatf(),orthographic:true)
    try renderer.render(command:command,output:output,camera:renderCamera,eye:SIMD3<Float>(0,0,0.5),rightTransform:pose,rightProjection:projection)
    renderer.copyEye(command:command,to:output,eye:0)
    let blit=command.makeBlitCommandEncoder()!
    blit.copy(from:output,sourceSlice:0,sourceLevel:0,sourceOrigin:MTLOrigin(),sourceSize:MTLSize(width:32,height:32,depth:1),to:readback,destinationOffset:0,destinationBytesPerRow:256,destinationBytesPerImage:32*256)
    blit.endEncoding();command.commit();command.waitUntilCompleted()
    try require(command.status == .completed,"pure Metal packet encodes colored triangles and line topology on the device")
    let bytes=readback.contents().bindMemory(to:UInt8.self,capacity:32*256)
    var redPixels=0,greenPixels=0
    for y in 0..<32 {for x in 0..<32 {let i=y*256+x*4;if bytes[i+2]>200 && bytes[i+1]<80 {redPixels+=1};if bytes[i+1]>100 && bytes[i+2]<80 {greenPixels+=1}}}
    print("Packet GPU readback: red=\(redPixels), green=\(greenPixels)")
    print("Packet GPU center: \(bytes[16*256+16*4]),\(bytes[16*256+16*4+1]),\(bytes[16*256+16*4+2]); draws=\(renderer.draws), culled=\(renderer.culled)")
    try require(redPixels>100 && greenPixels>=20,"GPU readback retains red command-card triangles and green targeting line pixels")
}
