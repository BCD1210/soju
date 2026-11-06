import Cocoa
import SceneKit
import Metal
import GLTFKit2

// Deterministic renderer regression tests; live native-engine acceptance is
// recorded separately. No commands are issued to the game by these checks.
func runtimeChecks()throws {
    let commander=try Commander(art:Art())
    commander.requireFidelity=false // Exercise rig lifecycle independently of the live fidelity gate.
    let map=commander.art.manifest["map"] as! Obj
    commander.mapWidth=Int(number(map,"width"));commander.mapHeight=Int(number(map,"height"))
    commander.mapTiles=map["tiles"] as! [Int]
    let x=100.0,y=100.0
    func unit(_ id:String)->Obj {["id":id,"type":0,"owner":6,"x":x,"y":y,"hp":40,"completed":true,"order":2,"image":["animation":0]]}
    func state(_ frame:Double,_ units:[Obj],_ events:[Obj]=[],session:String="check")->Obj {
        ["ok":true,"protocol":2,"session":session,"frame":frame,"player":6,
         "map":["width":commander.mapWidth*32,"height":commander.mapHeight*32],
         "units":units,"events":events,"eventSeq":events.map{number($0,"seq")}.max() ?? commander.eventSequence]
    }
    var passed=[String]()
    func require(_ ok:Bool,_ message:String)throws {if !ok {throw SceneError.invalid("Runtime check: "+message)};passed.append(message)}
    let rigNumbers=try JSONSerialization.jsonObject(with:Data("[0,0.5,0.7071068,1]".utf8))
    try require(MetalRigAsset.floats(rigNumbers)?.count==4 && abs((MetalRigAsset.floats(rigNumbers)?[2] ?? 0)-0.7071068)<0.000001,"rig JSON rest transforms preserve mixed numeric values")
    let nativeReview:Obj=["method":"visual-native","passed":true,"visualReviewed":true,
        "nativeChecks":["scale":true,"picking":true,"teamColors":true,"clips":true,"textureBudget":true]]
    try require(Models3D.passesReviewCriteria(nativeReview),"visual native review does not require sprite similarity scores")
    var pendingReview=nativeReview;pendingReview["visualReviewed"]=false
    try require(!Models3D.passesReviewCriteria(pendingReview),"pending visual review cannot activate a model")
    for missing in ["scale","picking","teamColors","clips","textureBudget"] {
        var incomplete=nativeReview,checks=nativeReview["nativeChecks"] as! [String:Bool]
        checks.removeValue(forKey:missing);incomplete["nativeChecks"]=checks
        try require(!Models3D.passesReviewCriteria(incomplete),"native review requires \(missing) evidence")
    }
    try require(!Models3D.passesReviewCriteria(["passed":true,"minimumIoU":0.8,"minimumSSIM":0.9]),"legacy review retains its similarity threshold")
    commander.camera.simdPosition=SIMD3<Float>(12,3,24)
    commander.yaw=0.3;commander.pitch = -0.2
    let originalView=commander.viewBookmark
    commander.toggleTabletop()
    try require(commander.tabletop && commander.terrainRadius>=max(commander.mapWidth,commander.mapHeight),"table view covers full map")
    commander.adjustTable(scale:100,height:100,rotation:0.5)
    try require(commander.wrists.zoom==256 && commander.tableHeight==1.5,"table scale and height clamp safely")
    commander.adjustTable(scale:-100,height:-100,rotation:0)
    try require(commander.wrists.zoom==4 && commander.tableHeight==0.25,"table minimum scale and height remain positive")
    commander.toggleTabletop()
    try require(commander.camera.simdPosition==originalView.position && commander.yaw==originalView.yaw && commander.pitch==originalView.pitch,"table exit restores ground pose exactly")
    commander.state=["player":6]
    var rider=unit("rider");rider["facing"]=64
    commander.units=["rider":rider]
    commander.enterPossession("rider")
    try require(commander.possessedUnit=="rider" && !commander.tabletop,"owned ground unit can be possessed")
    let ridingYaw=commander.yaw
    rider["x"]=132;rider["facing"]=128;commander.units["rider"]=rider
    commander.updatePossession(0.016)
    try require(commander.yaw==ridingYaw,"unit facing changes do not rotate possessed camera")
    rider["transported"]=true;commander.units["rider"]=rider;commander.updatePossession(0.016)
    try require(commander.possessedUnit==nil && commander.camera.simdPosition==originalView.position,"transport entry restores previous view")
    rider["transported"]=false;rider["airborne"]=true;commander.units["rider"]=rider
    commander.toggleTabletop();let tableView=commander.viewBookmark
    commander.enterPossession("rider")
    try require(commander.possessedUnit=="rider","owned air unit can be possessed")
    commander.units.removeValue(forKey:"rider");commander.updatePossession(0.016)
    try require(commander.possessedUnit==nil && commander.tabletop && commander.camera.simdPosition==tableView.position,"unit disappearance restores previous table view")
    for flag in ["dead","burrowed","transported","renderHidden","building"] {
        var invalid=unit("invalid");invalid[flag]=true
        try require(!commander.canPossess(invalid),"possession rejects \(flag) unit")
    }
    var enemy=unit("enemy");enemy["owner"]=1
    try require(!commander.canPossess(enemy),"possession rejects enemy unit")
    commander.toggleTabletop();commander.state=[:];commander.units=[:]
    let longRun=FrameDistribution()
    for _ in 0..<54000 {longRun.add(1000.0/90)}
    longRun.add(2000)
    let longSummary=longRun.summary()
    try require(longRun.count==54001,"timing retains all samples beyond ten minutes at 90 Hz")
    try require(abs(number(longSummary,"p95Ms")-11.12)<0.0001 && number(longSummary,"maxMs")==2000,"timing quantiles retain overflow stalls")
    try require(number(longSummary,"estimatedMissedRefreshSlots")==179,"missed refresh estimate distinguishes a long stall")
    let timing=XRFrameMetrics()
    timing.completed(frame:1,eye:1,milliseconds:2)
    timing.completed(frame:1,eye:0,milliseconds:3)
    timing.completed(frame:2,eye:0,milliseconds:4)
    timing.completed(frame:2,eye:1,milliseconds:5)
    try require(timing.completedStereoSamples==2,"GPU timing pairs either completion callback order")
    let batchCheck = try MetalBatches(device:commander.art.device)
    try require(true,"instanced stereo Metal pipelines compile on this device")
    let opacityParent=SCNNode(),opacityChild=SCNNode()
    opacityParent.opacity=0.45;opacityChild.opacity=0.5;opacityParent.addChildNode(opacityChild)
    try require(abs(batchCheck.inheritedOpacity(opacityChild)-0.225)<0.00001,"batched sprites preserve inherited cloak opacity")
    let pickGeometry=SCNGeometry(sources:[SCNGeometrySource(vertices:[SCNVector3(-1,-1,0),SCNVector3(1,-1,0),SCNVector3(0,1,0)])],elements:[SCNGeometryElement(indices:[Int32(0),1,2],primitiveType:.triangles)])
    let pickScene=SCNScene(),pickNode=SCNNode(geometry:pickGeometry)
    let pickRoot=pickScene.rootNode
    pickNode.categoryBitMask=4;pickRoot.addChildNode(pickNode)
    let pickRenderer=SCNRenderer(device:commander.art.device,options:nil)
    pickRenderer.scene=pickScene
    let pickCamera=SCNNode();pickCamera.camera=SCNCamera();pickCamera.position=SCNVector3(0,0,2)
    pickRoot.addChildNode(pickCamera);pickRenderer.pointOfView=pickCamera
    _=pickRenderer.snapshot(atTime:0,with:CGSize(width:8,height:8),antialiasingMode:.none)
    let picks=pickRoot.hitTestWithSegment(from:SCNVector3(0,0,2),to:SCNVector3(0,0,-2),options:[SCNHitTestOption.categoryBitMask.rawValue:5])
    try require(!picks.isEmpty,"selection mask includes batched geometry")
    let bitmap=NSBitmapImageRep(bitmapDataPlanes:nil,pixelsWide:2,pixelsHigh:2,bitsPerSample:8,samplesPerPixel:4,hasAlpha:true,isPlanar:false,colorSpaceName:.deviceRGB,bytesPerRow:8,bitsPerPixel:32)!
    for y in 0..<2 {for x in 0..<2 {var pixel=[y==0 ? 255:0,y==0 ? 255:0,y==0 ? 255:0,255];bitmap.setPixel(&pixel,atX:x,y:y)}}
    let image=NSImage(size:NSSize(width:2,height:2));image.addRepresentation(bitmap)
    for topLeft in [true,false] {
        let texture=try batchCheck.texture(image,fallback:batchCheck.white,topLeft:topLeft)
        try require(texture.pixelFormat == .rgba8Unorm_srgb || texture.pixelFormat == .bgra8Unorm_srgb,"color textures decode sRGB even when the loader returns unorm")
        let readback=commander.art.device.makeBuffer(length:512,options:.storageModeShared)!
        let command=batchCheck.queue.makeCommandBuffer()!,blit=command.makeBlitCommandEncoder()!
        blit.copy(from:texture,sourceSlice:0,sourceLevel:0,sourceOrigin:MTLOrigin(),sourceSize:MTLSize(width:2,height:2,depth:1),to:readback,destinationOffset:0,destinationBytesPerRow:256,destinationBytesPerImage:512)
        blit.endEncoding();command.commit();command.waitUntilCompleted()
        let first=readback.contents().load(as:UInt8.self)
        try require(topLeft ? first>250:first<5,"Metal texture loader honors requested origin")
    }
    let (planeVertices,_)=batchCheck.mesh(SCNPlane(width:1,height:1))
    let planeTop=planeVertices.max(by:{$0.position.y<$1.position.y})!
    try require(planeTop.uv.y==0,"SceneKit plane UVs also start at the image top")
    let streaming=FrameStream()
    var completedRequests=0
    streaming.onReady={completedRequests+=1}
    streaming.request(239,frame:68,color:[244,4,4],prefetch:true)
    let streamDeadline=Date().addingTimeInterval(30)
    while streaming.get(239,frame:68,color:[244,4,4])==nil && Date()<streamDeadline {Thread.sleep(forTimeInterval:0.01)}
    let prepared=streaming.get(239,frame:68,color:[244,4,4])
    streaming.queue.sync {}
    try require(completedRequests==0,"speculative frame readiness does not republish an unchanged scene")
    streaming.queue.suspend()
    streaming.request(239,frame:76,color:[244,4,4],prefetch:true)
    streaming.request(239,frame:76,color:[244,4,4])
    streaming.queue.resume()
    streaming.queue.sync {}
    try require(completedRequests==1 && streaming.get(239,frame:76,color:[244,4,4]) != nil,"a demanded pending prefetch wakes scene publication exactly once")
    try require(prepared?.geometry != nil,"background stream publishes a complete relief frame")
    try require(prepared?.image.pixelFormat == .rgba8Unorm_srgb || prepared?.image.pixelFormat == .bgra8Unorm_srgb,"streamed color is explicitly sRGB")
    if let prepared=prepared,let payload=prepared.geometry {
        let referenceArt=WorldSprites()
        let referenceMaterial=referenceArt.relief.material(239,frame:68,color:[244,4,4],art:referenceArt)
        func pixels(_ texture:MTLTexture)->Data {
            let stride=(texture.width*4+255)/256*256
            let storage=commander.art.device.makeBuffer(length:stride*texture.height,options:.storageModeShared)!
            let command=batchCheck.queue.makeCommandBuffer()!,blit=command.makeBlitCommandEncoder()!
            blit.copy(from:texture,sourceSlice:0,sourceLevel:0,sourceOrigin:MTLOrigin(),sourceSize:MTLSize(width:texture.width,height:texture.height,depth:1),to:storage,destinationOffset:0,destinationBytesPerRow:stride,destinationBytesPerImage:stride*texture.height)
            blit.endEncoding();command.commit();command.waitUntilCompleted()
            var result=Data()
            for row in 0..<texture.height {result.append(storage.contents().advanced(by:row*stride).assumingMemoryBound(to:UInt8.self),count:texture.width*4)}
            return result
        }
        var equalMaps=true
        for name in ["normal","bright","specular","emissive","ao_depth"] {
            let fallback=name=="normal" ? batchCheck.normal:name=="ao_depth" ? batchCheck.white:batchCheck.black
            let original=try batchCheck.texture((referenceMaterial.value(forKey:"relief_"+name) as? SCNMaterialProperty)?.contents,fallback:fallback)
            let streamed=try batchCheck.texture(prepared.material.maps[name]?.contents,fallback:fallback)
            equalMaps = equalMaps && original.width==streamed.width && original.height==streamed.height && original.pixelFormat==streamed.pixelFormat && pixels(original)==pixels(streamed)
        }
        try require(equalMaps,"streamed lighting maps preserve reference pixels, dimensions and color formats")
        let first=payload.geometry(),second=payload.geometry()
        let expected=batchCheck.mesh(second)
        try require(first.sources[0] !== second.sources[0] && first.elements[0] !== second.elements[0],"streamed relief reconstruction owns distinct SceneKit sources and elements")
        second.elements[0].primitiveRange=NSRange(location:0,length:1)
        let originalRange=second.elements[0].primitiveRange
        first.elements[0].primitiveRange=NSRange(location:0,length:0);first.materials=[SCNMaterial()]
        let preserved=batchCheck.mesh(second)
        try require(!expected.0.isEmpty && expected.1==preserved.1 && second.elements[0].primitiveCount==payload.indices.count/3 && second.elements[0].primitiveRange==originalRange,"mutating one streamed relief reconstruction cannot change another owner's mesh")
        let firstMaterial=prepared.material.material(),secondMaterial=prepared.material.material()
        let firstMap=firstMaterial.value(forKey:"relief_normal") as! SCNMaterialProperty
        let secondMap=secondMaterial.value(forKey:"relief_normal") as! SCNMaterialProperty
        firstMap.contents=NSColor.red
        try require(firstMap !== secondMap && !(secondMap.contents is NSColor),"streamed material reconstruction owns independent map properties")
        let sharedFirst=prepared.material.material(sharedForMetal:true),sharedSecond=prepared.material.material(sharedForMetal:true)
        try require(useMetalBatches ? sharedFirst === sharedSecond:sharedFirst !== sharedSecond,"only the read-only Metal path shares prepared frame materials")
        try require(sharedFirst !== firstMaterial && sharedFirst !== secondMaterial && (sharedFirst.value(forKey:"relief_normal") as? SCNMaterialProperty)?.contents is MTLTexture,"shared Metal material remains independent of mutable fallback copies")
        let geometryArt=WorldSprites(streaming:true)
        let sharedMesh=prepared.material.geometry(payload,art:geometryArt,id:239,frame:68)
        let repeatedMesh=prepared.material.geometry(payload,art:geometryArt,id:239,frame:68)
        try require(useMetalBatches ? sharedMesh === repeatedMesh:sharedMesh !== repeatedMesh,"read-only Metal frames reuse complete geometry while fallback retains private geometry")
        let warped=payload.geometry();warped.materials=[SCNMaterial()]
        try require((!useMetalBatches || sharedMesh.firstMaterial === sharedFirst) && sharedMesh.firstMaterial !== warped.firstMaterial && batchCheck.mesh(sharedMesh).1==payload.indices.map{UInt32($0)},"private contact geometry cannot replace the shared frame's material or indices")
    }
    let streamedArt=WorldSprites(streaming:true),streamedBillboard=UnitBillboard(id:"stream-transition")
    let catalogFirst=streamedArt.frameRecords(239)!,catalogAgain=streamedArt.frameRecords(239)!
    let sharedCatalog=catalogFirst.withUnsafeBufferPointer { first in
        catalogAgain.withUnsafeBufferPointer { again in first.baseAddress==again.baseAddress }
    }
    try require(!catalogFirst.isEmpty && sharedCatalog,"repeated frame metadata lookups reuse typed array storage")
    let streamedUnit:Obj=["x":100,"y":100,"image":["id":239,"frame":68,"flags":0]]
    let streamRed=[244,4,4],streamBlue=[4,4,244]
    func waitForColor(_ color:[Int]) {
        let deadline=Date().addingTimeInterval(30)
        while streamedBillboard.shownColors[0] != color && Date()<deadline {
            streamedBillboard.updateView(eye:SIMD3<Float>(3,4,5),orientation:simd_quatf(),perspective:false,art:streamedArt)
            Thread.sleep(forTimeInterval:0.01)
        }
    }
    streamedBillboard.apply(streamedUnit,ground:commander.ground,art:streamedArt,color:streamRed);waitForColor(streamRed)
    try require(streamedBillboard.shownImages[0]==239 && streamedBillboard.shownColors[0]==streamRed,"initial streamed placeholder becomes visible")
    streamedBillboard.apply(streamedUnit,ground:commander.ground,art:streamedArt,color:streamBlue)
    try require(streamedBillboard.shownColors[0]==streamRed,"pending owner change retains the displayed texture identity")
    waitForColor(streamBlue)
    try require(streamedBillboard.shownColors[0]==streamBlue,"same-frame owner change publishes its replacement texture")
    let idlePose=streamedBillboard.poseRevision,idleGeometry=streamedBillboard.geometryRevision
    var healthUpdate=streamedUnit;healthUpdate["hp"]=17;healthUpdate["order"]=3
    streamedBillboard.apply(healthUpdate,ground:commander.ground,art:streamedArt,color:streamBlue)
    streamedBillboard.updateView(eye:SIMD3<Float>(3,4,5),orientation:simd_quatf(),perspective:false,art:streamedArt)
    try require(streamedBillboard.poseRevision==idlePose && streamedBillboard.geometryRevision==idleGeometry && number(streamedBillboard.unit,"hp")==17 && number(streamedBillboard.unit,"order")==3,"nonvisual native updates retain the displayed pose while accepting current unit state")
    let movingGeometry=streamedBillboard.layers[0]!.geometry,movingRevision=streamedBillboard.geometryRevision
    var movedUnit=streamedUnit
    movedUnit["x"]=132
    movedUnit["image"]=["id":239,"frame":68,"flags":0,"mapX":132,"mapY":100]
    streamedBillboard.apply(movedUnit,ground:commander.ground,art:streamedArt,color:streamBlue)
    streamedBillboard.updateView(eye:SIMD3<Float>(3,4,5),orientation:simd_quatf(),perspective:false,art:streamedArt)
    try require(streamedBillboard.layers[0]!.geometry === movingGeometry && streamedBillboard.geometryRevision==movingRevision && abs(streamedBillboard.root.position.x-4.125)<0.001 && number(streamedBillboard.nativeLayers[0]!,"mapX")==132,"native translation updates pose and evidence without rebuilding unchanged image geometry")
    movedUnit["image"]=["id":239,"frame":68,"flags":0,"mapX":132,"mapY":100,"offsetX":4]
    streamedBillboard.apply(movedUnit,ground:commander.ground,art:streamedArt,color:streamBlue)
    try require(streamedBillboard.geometryRevision>movingRevision && streamedBillboard.layers[0]!.geometry === movingGeometry && abs(streamedBillboard.layers[0]!.position.x-0.125)<0.001,"image offset changes update the layer pose while retaining its frame geometry")
    var directional=streamedUnit
    directional["image"]=["id":239,"frame":68,"frameset":68,"direction":0,"flags":8]
    streamedBillboard.apply(directional,ground:commander.ground,art:streamedArt,color:streamBlue)
    let observer=streamedBillboard.root.simdWorldPosition+SIMD3<Float>(3,4,0)
    let directionDeadline=Date().addingTimeInterval(30)
    repeat {
        streamedBillboard.updateView(eye:observer,orientation:simd_quatf(),perspective:true,art:streamedArt)
        if streamedBillboard.shown[0]==76 {break}
        Thread.sleep(forTimeInterval:0.01)
    } while Date()<directionDeadline
    try require(streamedBillboard.shown[0]==76,"streamed directional unit resolves its observer-facing frame")
    directional["image"]=["id":239,"frame":68,"frameset":68,"direction":0,"flags":8,"offsetX":1]
    streamedBillboard.apply(directional,ground:commander.ground,art:streamedArt,color:streamBlue)
    let observerRevision=streamedBillboard.geometryRevision
    try require(streamedBillboard.shown[0]==76,"native layer changes construct the already-known observer frame directly")
    streamedBillboard.updateView(eye:observer,orientation:simd_quatf(),perspective:true,art:streamedArt)
    try require(streamedBillboard.geometryRevision==observerRevision,"observer update does not rebuild a native frame that will not be displayed")
    streamedBillboard.apply(streamedUnit,ground:commander.ground,art:streamedArt,color:streamBlue)
    commander.billboards["stream-transition"]=streamedBillboard
    commander.sceneRevision+=1
    try batchCheck.prepare(commander)
    let firstBuffer=batchCheck.batches.first(where:{!$0.terrain})!.vertices
    let retainedBatch=batchCheck.batches.first(where:{!$0.terrain})!
    let bufferBytes=retainedBatch.vertices.length+retainedBatch.indices.length+(retainedBatch.flatVertices?.length ?? 0)+retainedBatch.instances.reduce(0){$0+$1.length}
    try require(MetalBatches.retainedBytes(retainedBatch)>bufferBytes,"published native batches account for textures as well as buffers")
    commander.sceneRevision+=1
    try batchCheck.prepare(commander)
    try require(batchCheck.batches.first(where:{!$0.terrain})!.vertices === firstBuffer,"unchanged native groups reuse packed vertex buffers")
    if let node=streamedBillboard.layers[0] {
        node.geometry=SCNPlane(width:2,height:3)
        streamedBillboard.geometryRevision+=1;commander.sceneRevision+=1
        try batchCheck.prepare(commander)
        try require(batchCheck.batches.first(where:{!$0.terrain})!.vertices !== firstBuffer,"new geometry source invalidates cached native group")
    }
    let asynchronous=try MetalBatches(device:commander.art.device)
    let originalLayer=streamedBillboard.layers[0]!
    let originalTransform=originalLayer.simdWorldTransform
    asynchronous.prepareAsync(commander)
    originalLayer.position.x += 10
    let deadline=Date().addingTimeInterval(15)
    while !asynchronous.ready && Date()<deadline {RunLoop.current.run(until:Date().addingTimeInterval(0.01))}
    try require(asynchronous.ready,"background batch preparation publishes a completed snapshot")
    let captured=asynchronous.batches.first(where:{!$0.terrain})!.drawNodes[0].transform
    try require(simd_equal(captured,originalTransform),"published batch retains captured transforms when source nodes move")
    originalLayer.simdWorldTransform=originalTransform
    let poseBatches=try MetalBatches(device:commander.art.device)
    func poseInput(_ revision:Int,_ units:[UnitBillboard])->MetalBatches.Input {
        MetalBatches.Input(sceneRevision:revision,terrainRevision:revision,terrainFogRevision:revision,mapWidth:1,terrain:[:],billboards:units,overlays:[],objectCount:units.count,reuseUnitPoses:true)
    }
    try poseBatches.prepare(poseInput(1,[streamedBillboard]))
    let referenceMesh=batchCheck.mesh(streamedBillboard.layers[0]!.geometry!)
    let directBatch=poseBatches.batches[0]
    let referenceVertices=referenceMesh.0.withUnsafeBytes{Data($0)}
    let referenceIndices=referenceMesh.1.withUnsafeBytes{Data($0)}
    try require(directBatch.vertices.length==referenceVertices.count && Data(bytes:directBatch.vertices.contents(),count:directBatch.vertices.length)==referenceVertices && directBatch.indices.length==referenceIndices.count && Data(bytes:directBatch.indices.contents(),count:directBatch.indices.length)==referenceIndices,"typed relief upload preserves SceneKit reference vertex, UV and index bytes")
    let retainedPose=poseBatches.batches[0].drawNodes[0].transform
    let projectedBefore=streamedBillboard.projectedSpriteBounds(projection:matrix_identity_float4x4,viewport:SIMD2<Float>(320,200),art:streamedArt)
    try poseBatches.prepare(poseInput(2,[streamedBillboard]))
    try require(simd_equal(poseBatches.batches[0].drawNodes[0].transform,retainedPose),"terrain-only refinement preserves cached unit poses")
    let originalRoot=streamedBillboard.root.simdPosition
    streamedBillboard.root.simdPosition.x+=1;streamedBillboard.poseRevision+=1
    try poseBatches.prepare(poseInput(3,[streamedBillboard]))
    let changedPose=poseBatches.batches[0].drawNodes[0].transform
    let projectedAfter=streamedBillboard.projectedSpriteBounds(projection:matrix_identity_float4x4,viewport:SIMD2<Float>(320,200),art:streamedArt)
    try require(abs(changedPose.columns.3.x-retainedPose.columns.3.x-1)<0.001,"native pose revision invalidates cached unit transforms")
    try require(!projectedBefore.isEmpty && abs(projectedAfter[0].left-projectedBefore[0].left-160)<0.01,"projected bounds update with native poses while retained bounds stay immutable")
    let resizedBounds=streamedBillboard.projectedSpriteBounds(projection:matrix_identity_float4x4,viewport:SIMD2<Float>(640,200),art:streamedArt)
    try require(abs(resizedBounds[0].left-projectedAfter[0].left*2)<0.01,"projected bounds cache accounts for viewport changes")
    streamedBillboard.root.isHidden=true
    try poseBatches.prepare(poseInput(4,[streamedBillboard]))
    try require(poseBatches.batches.isEmpty,"hidden units retire cached unit draws")
    streamedBillboard.root.isHidden=false;streamedBillboard.root.simdPosition=originalRoot;streamedBillboard.poseRevision+=1
    try poseBatches.prepare(poseInput(5,[]))
    try require(poseBatches.batches.isEmpty,"removed units cannot reuse cached unit draws")
    commander.billboards.removeValue(forKey:"stream-transition")
    let ownership=ResidentCache<NSObject>(limit:8),retainedObject=NSObject()
    ownership.insert(retainedObject,key:"retained",bytes:4);ownership.insert(NSObject(),key:"old",bytes:4)
    _=ownership.get("retained")
    try require(ownership.trim(to:4)==4 && ownership.get("old")==nil && ownership.get("retained") === retainedObject,"pressure trim drops least-recent cache ownership while preserving referenced resources")
    try require(ownership.trim(to:0)==4 && ownership.bytes==0 && ownership.get("retained")==nil,"pressure trim releases the final cache entry with exact accounting")
    ownership.insert(retainedObject,key:"reused",bytes:4)
    try require(ownership.get("reused") === retainedObject && ownership.trim(to:8)==0,"trimmed cache remains reusable without evicting below its target")
    let cache=ResidentCache<Int>(limit:4)
    cache.insert(1,key:"first",bytes:2);cache.insert(2,key:"second",bytes:2)
    _=cache.get("first");cache.insert(3,key:"third",bytes:2)
    try require(cache.get("first")==1 && cache.get("second")==nil && cache.get("third")==3,"resident cache evicts least recently used resource")
    cache.insert(4,key:"oversized",bytes:5)
    try require(cache.bytes==4 && cache.get("oversized")==nil,"resident cache respects byte budget")

    cache.insert(5,key:"first",bytes:1);cache.insert(6,key:"fourth",bytes:3)
    try require(cache.bytes==4 && cache.get("third")==nil && cache.get("first")==5 && cache.get("fourth")==6,"LRU replacement updates links and byte accounting before eviction")
    cache.insert(7,key:"fourth",bytes:4)
    try require(cache.bytes==4 && cache.get("first")==nil && cache.get("fourth")==7,"LRU replacement of its newest entry preserves an evictable oldest entry")

    let nearCamera=SCNNode();nearCamera.camera=SCNCamera();nearCamera.camera!.zNear=0.03
    let nearProjection=simd_float4x4(nearCamera.camera!.projectionTransform)
    for offset:Float in [-0.032,0.032] {
        var eyeTransform=matrix_identity_float4x4;eyeTransform.columns.3.x=offset
        let volume=PossessionPresentation.eyeVolume(transform:eyeTransform,projection:nearProjection)
        let inverse=simd_inverse(nearProjection)
        for x:Float in [-1,0,1] {for y:Float in [-1,0,1] {
            let h=inverse*SIMD4<Float>(x,y,-1,1),p=eyeTransform*(h/h.w)
            try require(PossessionPresentation.intersectsEye(center:SIMD3<Float>(p.x,p.y,p.z),radius:0.001,volume:volume),"both stereo near rectangles exclude intersecting rig bounds")
        }}
        try require(PossessionPresentation.intersectsEye(center:.zero,radius:3,volume:volume),"eye enclosed inside a large rig excludes its geometry")
        try require(!PossessionPresentation.intersectsEye(center:SIMD3<Float>(0,0,-4),radius:0.5,volume:volume),"distant rig remains visible in possession")
    }
    let largeBuilding=SCNNode(geometry:SCNBox(width:2,height:2,length:2,chamferRadius:0))
    largeBuilding.simdScale=SIMD3<Float>(4,2,3);largeBuilding.simdPosition=SIMD3<Float>(4,0,0)
    try require(PossessionPresentation.near(largeBuilding,eye:.zero),"transformed building bounds protect an eye inside the model")
    commander.apply(state(1,[unit("a"),unit("b")]),[:])
    try require(commander.initialized,"an owned army initializes the camera without a town hall")
    try require(commander.models.count==2,"two independent live models")
    let aMaterials=Set(commander.models["a"]!.source.materials.map{ObjectIdentifier($0)})
    let bMaterials=Set(commander.models["b"]!.source.materials.map{ObjectIdentifier($0)})
    try require(aMaterials.isDisjoint(with:bMaterials),"team tint materials are isolated between units")
    commander.enterPossession("a");commander.updatePossessionVisibility()
    let ownedMesh=commander.models["a"]!.allNodes.first(where:{$0.geometry != nil})!
    let nearbyMesh=commander.models["b"]!.allNodes.first(where:{$0.geometry != nil})!
    try require(!commander.models["a"]!.root.isHidden && ownedMesh.castsShadow && ownedMesh.categoryBitMask==PossessionPresentation.hiddenCategory,"possessed body retains shadow geometry and uses camera-only exclusion")
    try require(nearbyMesh.categoryBitMask==PossessionPresentation.hiddenCategory,"nearby body cannot obscure possession view")
    try require(ownedMesh.categoryBitMask & PossessionPresentation.pickCategories != 0,"camera-excluded mesh remains in desktop and XR pick mask")
    let head=commander.possessionEye(unit("a")),height=commander.modelLibrary.catalog.definitions[0]?.targetHeight ?? 1
    try require(abs(Double(head.y)-commander.ground.elevation(x/32,y/32)-max(0.20,height*0.86))<0.00001,"possession eye uses scaled visor height")
    commander.models["b"]!.root.position.x += 5;commander.updatePossessionVisibility()
    try require(nearbyMesh.categoryBitMask != PossessionPresentation.hiddenCategory,"distant bodies remain visible")
    commander.exitPossession()
    try require(ownedMesh.categoryBitMask != PossessionPresentation.hiddenCategory && !commander.models["a"]!.root.isHidden,"possession exit restores body visibility without mutating native hidden state")
    commander.apply(state(2,[unit("a"),unit("b")]),[:])
    let attack:Obj=["kind":"attack","unit":"a","seq":1,"x":x,"y":y]
    commander.apply(state(12,[unit("a"),unit("b")],[attack]),[:])
    try require(commander.models["a"]?.action=="attack","native fire selects attack animation")
    let authoredDeath=commander.models["b"]?.clip(for:"death") != nil
    let death:Obj=["kind":"death","unit":"b","seq":2,"state":unit("b")]
    commander.apply(state(24,[unit("a")],[death]),[:])
    try require(commander.models["b"]==nil && (authoredDeath ? commander.retiring["b"]?.dying==true : commander.retiring["b"]==nil) && commander.deathCount==(authoredDeath ? 1:0),"confirmed death retires only an authored death clip")
    commander.apply(state(25,[unit("a")],[death]),[:])
    try require(commander.deathCount==(authoredDeath ? 1:0),"replayed event is ignored")
    commander.apply(state(26,[]),[:])
    try require(commander.models.isEmpty && commander.deathCount==(authoredDeath ? 1:0),"fog disappearance does not create death")
    commander.apply(state(1000,[]),[:])
    try require(commander.retiring.isEmpty,"death model is released")
    let model=try commander.modelLibrary.make(0)
    var u=unit("air");u["airborne"]=true
    model.apply(u,height:2,frame:1)
    try require(abs(Double(model.targetPosition.y)-3.5)<0.001,"airborne model clears ground")
    u["renderHidden"]=true;u["transported"]=true;model.apply(u,height:0,frame:2)
    try require(model.root.isHidden,"transported model is hidden")
    u["renderHidden"]=false;u["cloaked"]=true;model.apply(u,height:0,frame:3)
    try require(!model.root.isHidden && model.root.opacity<1,"visible cloaked model is translucent")
    var deathDefinition=model.definition.raw
    var mapping=deathDefinition["animations"] as? Obj ?? [:]
    mapping["death"]=model.clip(for:"attack") ?? model.clip(for:"idle")
    deathDefinition["animations"]=mapping
    let deathModel=try ModelInstance(asset:commander.modelLibrary.assets[0]!.1,
        definition:ModelDefinition(kind:0,object:deathDefinition),revision:"one-shot-regression")
    deathModel.beginDeath(frame:1)
    try require(!deathModel.activePlayers.isEmpty && deathModel.activePlayers.allSatisfy{$0.animation.repeatCount<=1},"death clip is bound with one-shot playback")
    deathModel.apply(unit("dead"),height:0,frame:2)
    try require(deathModel.action=="death","later state cannot restart a dying rig")
    for (order,action) in [(116,"burrow"),(117,"burrowed"),(118,"unburrow"),(98,"siege"),(99,"unsiege"),(42,"morph"),(72,"lift"),(71,"land"),(34,"repair")] {
        var sample=unit("state");sample["order"]=order;sample["burrowed"]=order==117
        try require(ModelInstance.stateAction(sample,moving:false,attacking:false)==action,"native order \(order) maps to \(action)")
    }
    let bullet:Obj=["id":1,"weapon":0,"x":x,"y":y]
    commander.effects.apply(bullets:[bullet],events:[],units:[:],frame:1001,ground:commander.ground)
    try require(commander.effects.projectiles.count==1,"native projectile is displayed")
    commander.effects.apply(bullets:[],events:[],units:[:],frame:1002,ground:commander.ground)
    try require(commander.effects.projectiles.isEmpty,"expired native projectile is removed")
    commander.fog[0]=50;commander.controlGroups[4]=["a"]
    commander.apply(state(1,[unit("c")],session:"new-game"),[:])
    try require(commander.fog.isEmpty && commander.controlGroups.isEmpty && commander.effects.sprites.renderedFrames==0,"new game clears fog control groups and sprite counters")
    try require(commander.deathCount==0 && commander.effects.counts.isEmpty && Set(commander.models.keys)==["c"],"new game clears old effects and identities")
    var nativeDeath=unit("fallback");nativeDeath["image"]=["id":239,"frame":0,"flags":0]
    commander.worldSprites.apply(["dyingUnits":[nativeDeath]],ground:commander.ground,authored:[])
    try require(commander.worldSprites.nodes.count==1,"native death image creates fallback geometry")
    commander.worldSprites.apply(["dyingUnits":[nativeDeath]],ground:commander.ground,authored:["fallback"])
    try require(commander.worldSprites.nodes.isEmpty,"authored death suppresses duplicate sprite")
    commander.worldSprites.apply(["loneSprites":[nativeDeath]],ground:commander.ground,authored:[])
    try require(commander.worldSprites.nodes.count==1,"visible independent native sprite is rendered")
    commander.worldSprites.apply([:],ground:commander.ground,authored:[])
    try require(commander.worldSprites.nodes.isEmpty,"native disappearance removes sprite without a fabricated timeline")
    let fallback=UnitBillboard(id:"fallback-live")
    var sprite=unit("fallback-live")
    sprite["image"]=["id":239,"frame":77,"frameset":68,"direction":9,"flags":2]
    fallback.apply(sprite,ground:commander.ground,art:commander.worldSprites)
    try require(fallback.layers.count==1 && fallback.evidence.first?["frame"] as? Int == 77 && fallback.layers[0]?.scale.x == -1,
        "fallback preserves native directional frame and mirror flag")
    sprite["image"]=["id":239,"frame":68,"frameset":68,"direction":0,"flags":8]
    fallback.apply(sprite,ground:commander.ground,art:commander.worldSprites)
    for (angle,frame,mirror) in [(0.0,68,false),(Double.pi/2,76,false),(Double.pi,84,false),(-Double.pi/2,76,true)] {
        let origin=fallback.root.simdWorldPosition
        fallback.updateView(eye:origin+SIMD3<Float>(Float(sin(angle)*4),2,Float(cos(angle)*4)),orientation:simd_quatf(),perspective:true,art:commander.worldSprites)
        try require(fallback.evidence.first?["frame"] as? Int == frame && fallback.evidence.first?["mirrored"] as? Bool == mirror,
            "observer angle \(angle) selects the native animation group and mirrored direction")
        try require(fallback.layers[0]!.renderingOrder==0,"relief uses depth ordering rather than the 2D sprite bucket")
        let facing=fallback.layers[0]!.simdOrientation.act(SIMD3<Float>(0,0,1))
        try require(abs(facing.x-Float(sin(angle)*cos(Terrain3D.canonicalElevation)))<0.0001 && abs(facing.z-Float(cos(angle)*cos(Terrain3D.canonicalElevation)))<0.0001,"relief preserves source elevation and faces observer azimuth at \(angle)")
    }
    try require(WorldSprites.playerColor([[0,0,0,0]],owner:0)==Models3D.playerColors[0],"zero initialization colors use player palette")
    if let mesh=commander.worldSprites.relief.geometry(239,frame:68,art:commander.worldSprites) {
        try require(!(mesh is SCNPlane) && mesh.sources(for:.vertex).first!.vectorCount>100,"HD depth produces tessellated geometry")
        let box=mesh.boundingBox
        try require(box.max.z-box.min.z>0.01,"relief has nonzero physical depth")
        let material=commander.worldSprites.relief.material(239,frame:68,color:[244,4,4],art:commander.worldSprites)
        try require(material.readsFromDepthBuffer && material.writesToDepthBuffer,"relief participates in scene occlusion")
        try require(material.shaderModifiers?[.surface]?.contains("relief_specular")==true,"relief shader binds material layers")
    } else {try require(false,"prepared Marine relief required")}
    var profileContinuous=true
    for y in stride(from:1,to:commander.ground.height-1,by:3) {
        for x in stride(from:1,to:commander.ground.width-1,by:3) {
            let px=Double(x)/4,pz=(Double(y)+0.37)/4
            if abs(commander.ground.profile(px-0.000001,pz)-commander.ground.profile(px+0.000001,pz))>0.001 {profileContinuous=false}
        }
    }
    try require(profileContinuous,"subdivided cliff profiles meet across minitile boundaries")
    let fixed=UnitBillboard.viewFrame(["frame":3,"frameset":0,"flags":0],azimuth:.pi,count:50)
    try require(fixed.0==3 && !fixed.1,"non-directional building animation does not become a directional frame")
    let short=UnitBillboard.viewFrame(["frame":0,"frameset":0,"flags":8],azimuth:.pi,count:1)
    try require(short.0==0,"incomplete directional group keeps the valid native frame")
    sprite["renderHidden"]=true;fallback.apply(sprite,ground:commander.ground,art:commander.worldSprites)
    try require(fallback.root.isHidden,"hidden native unit cannot leak through fallback")
    sprite["image"]=["id":239,"frame":999999,"flags":0]
    fallback.apply(sprite,ground:commander.ground,art:commander.worldSprites)
    try require(fallback.layers.isEmpty,"invalid native frame removes stale fallback geometry")
    try require(!commander.devOverlay,"developer overlay defaults off")
    try require(commander.worldSprites.creepEdgeFrame(tileset:"twilight",neighbors:0)==0 &&
        commander.worldSprites.creepEdgeFrame(tileset:"twilight",neighbors:64)==1 &&
        commander.worldSprites.creepEdgeFrame(tileset:"twilight",neighbors:255)==37,
        "creep border lookup distinguishes empty north and surrounded tiles")
    try require(commander.worldSprites.creepEdgeMaterial(tileset:"twilight",frame:1) != nil,
        "owned HD creep border is loadable")
    try require(commander.wrists.ray.categoryBitMask==2 && commander.wrists.box.categoryBitMask==2,
        "decorative rays and drag outlines cannot intercept command hits")
    let tileFiles=((commander.art.manifest["tiles"] as? [String:Obj]) ?? [:]).values.compactMap{$0["file"] as? String}.sorted()
    if tileFiles.count>=2 {
        let neighbors=[tileFiles[1]]+[String](repeating:tileFiles[0],count:8)
        let material=try commander.art.terrainMaterial(neighbors)
        let padded=material.diffuse.contents as! MTLTexture,original=try commander.art.texture(tileFiles[1])
        var border=[UInt8](repeating:0,count:4),expected=[UInt8](repeating:0,count:4)
        padded.getBytes(&border,bytesPerRow:4,from:MTLRegionMake2D(0,0,1,1),mipmapLevel:0)
        original.getBytes(&expected,bytesPerRow:4,from:MTLRegionMake2D(original.width-8,original.height-8,1,1),mipmapLevel:0)
        try require(padded.width==original.width+16 && border==expected,"terrain mip gutter contains the real diagonal neighbor")
    }
    let card=CommandCard();card.accept(["types":[["id":7,"mineralCost":75,"gasCost":9]]])
    let waiting=card.choices(["id":"producer","type":106,"building":true]).first{$0.payload["op"] as? String == "train"}!
    try require(waiting.unavailable(["minerals":1000,"gas":1000],[:])=="Checking native requirements","production fails closed until native requirements arrive")
    card.acceptAvailability(["unit":"producer","choices":[["payload":["op":"train","type":7],"requirementsMet":true,"mineralCost":75,"gasCost":9]]])
    let production=card.choices(["id":"producer","type":106,"building":true])
    try require(production.contains{$0.payload["op"] as? String == "train" && $0.title.hasSuffix("75/9")},"command card uses native DAT prices")
    let scvChoice=production.first{$0.payload["op"] as? String == "train"}!
    try require(scvChoice.unavailable(["minerals":74,"gas":9],[:])=="Not enough minerals","native price disables an unaffordable command")
    try require(scvChoice.unavailable(["minerals":75,"gas":8],[:])=="Not enough gas","gas shortages disable commands")
    try require(scvChoice.unavailable(["minerals":75,"gas":9],[:])==nil,"exact resources enable commands")
    try require(scvChoice.hotkey=="S","SCV training displays its command hotkey")
    try require(card.choices(["type":0]).contains{$0.payload["op"] as? String == "patrol"},"Marine card includes patrol")
    try require(!production.contains{$0.payload["op"] as? String == "cancel"},"idle producers do not offer cancellation")
    try require(card.choices(["type":106,"building":true,"buildQueue":[7,228,228,228,228]]).contains{$0.payload["action"] as? String == "train"},"active native queue offers cancellation")
    let hiveMorph=card.choices(["type":131,"building":true,"completed":false,"order":45,"buildQueue":[132,228,228,228,228]]).filter{$0.payload["op"] as? String=="cancel"}
    try require(hiveMorph.count==1 && hiveMorph.first?.payload["action"] as? String=="morph","building morph offers its original cancellation handler without a train cancellation")
    let repair=card.choices(["type":7,"worker":true]).first{$0.payload["op"] as? String == "repair"}
    try require(repair?.target == .unit,"repair requires a unit target")
    card.accept(["spells":[["action":"storm","title":"Psionic storm","casters":[67],"target":"ground","energy":75]]])
    let storm=card.choices(["type":67]).first{$0.payload["op"] as? String == "spell"}
    try require(storm?.target == .ground && storm?.title.hasSuffix("75 energy")==true,"spell targeting and energy come from native metadata")
    let defiler=card.choices(["type":136,"building":true])
    try require(defiler.contains{$0.title=="Consume" && number($0.payload,"type")==16} && defiler.contains{$0.title=="Plague" && number($0.payload,"type")==15},"Defiler research uses correct technology IDs")
    let larva=card.choices(["type":35])
    try require(larva.contains{$0.payload["op"] as? String == "morph" && number($0.payload,"type")==47} && !larva.contains{number($0.payload,"type")==50},"larva produces Scourge rather than Infested Terrans")
    let evolution=card.choices(["type":139,"building":true]),pool=card.choices(["type":142,"building":true])
    try require(evolution.contains{$0.payload["op"] as? String == "upgrade" && number($0.payload,"type")==10} && !pool.contains{$0.title=="Melee attacks"},"Zerg weapon upgrades belong to Evolution Chamber")
    let originalCard=CommandCard()
    originalCard.originalStrings=["1":["label":"Synthetic command","hotkey":"Z"]]
    originalCard.acceptOriginal(["evaluated":true,"buttons":[
        ["position":2,"conditionState":0,"family":"stop","enabledString":1],
        ["position":7,"conditionState": -1,"family":"train","actionParameter":65000,"enabledString":1],
        ["position":9,"conditionState":1,"family":"submenu","actionParameter":228,"enabledString":1]
    ]],selection:["test"],menu:228)
    let originals=originalCard.choices(["id":"test"])
    try require(originals.map{$0.originalPosition!} == [7,9],"native cards preserve holes and omit hidden conditions")
    try require(originals[0].unavailable(["minerals":99999],[:]) != nil && originals[1].unavailable(["minerals":0],[:]) == nil,"native condition states remain authoritative regardless of local resource guesses")
    try require(originalCard.choices(["id":"different"]).isEmpty,"native cards reject a stale selected unit")
    originalCard.nativeMenu=239
    originalCard.acceptOriginal(["evaluated":false],selection:["stale"],menu:228)
    try require(originalCard.nativeSelection == ["test"],"late root replies cannot replace a newer submenu")
    commander.wrists.pending=CommandChoice("Old repair",["op":"repair"],.unit);commander.selected=["new-selection"]
    commander.wrists.update(commander)
    try require(commander.wrists.pending==nil,"selection changes cancel stale targeting")
    commander.tabletop=true;commander.wrists.zoom=10;let closeHeight=commander.commanderHeight;commander.wrists.zoom=20
    try require(closeHeight==9 && commander.commanderHeight==18,"tabletop zoom changes world viewing distance at a centered head pose")
    commander.tabletop=false;commander.placedEyeHeight=1.65
    commander.camera.position=Terrain3D.worldPosition(7,commander.ground.elevation(7,11),11)
    commander.camera.position.y += 1.65
    commander.toggleTabletop();commander.walk(0,0)
    try require(abs(commander.cameraMapPosition.x-7)<0.00001 && abs(commander.cameraMapPosition.z-11)<0.00001,
        "changing commander scale keeps native ground location fixed")
    for (top,bottom,expected) in [([2.0,0.0],[0.0,2.0],0.0...0.5),([0.0,2.0],[2.0,0.0],0.5...1.0),([2.0,2.0],[0.0,0.0],0.0...1.0)] {
        try require(Terrain3D.exposedEdge(top,bottom)==expected,"cliff edge clips exposed interval \(top)")
    }
    try require(Terrain3D.exposedEdge([0,0],[2,2])==nil,"lower cliff neighbor emits no duplicate wall")
    for height in [0.0,1.0,2.0,4.0] {
        let placed=Terrain3D.worldPosition(7,height,11),native=Terrain3D.mapPosition(placed)
        try require(abs(native.x-7)<0.00001 && abs(native.z-11)<0.00001,"relief hit returns native map coordinates at height \(height)")
        try require(abs(placed.z-11)<0.00001,"terrain height preserves its horizontal map domain at height \(height)")
    }
    let footprintURL=runtime.appendingPathComponent("runtime-footprint-terrain.json")
    let footprintData:Obj=["width":16,"height":16,"flags":[Int](repeating:1,count:256),"heights":(0..<256).map{$0%16>=10 ? 2.0:0.0}]
    try JSONSerialization.data(withJSONObject:footprintData).write(to:footprintURL)
    let footprintGround=try Terrain3D(url:footprintURL)
    var exactCorners=true
    for y in -2...footprintGround.height+2 {for x in -2...footprintGround.width+2 {
        let g=footprintGround
        let expected=(g.heights[g.index(x-1,y-1)]+g.heights[g.index(x,y-1)]+g.heights[g.index(x-1,y)]+g.heights[g.index(x,y)])/4
        exactCorners = exactCorners && g.corner(x,y,x,y)==expected
    }}
    try require(exactCorners,"immutable terrain corners match original height sampling inside and outside the map")
    var exactElevation=true
    for row in -8...footprintGround.height*8+8 {for column in -8...footprintGround.width*8+8 {
        let x=Double(column)/32,z=Double(row)/32,sx=x*16,sy=z*16,ix=floor(sx),iy=floor(sy),fx=sx-ix,fy=sy-iy,g=footprintGround
        let a=g.profile(ix/16,iy/16),b=g.profile(ix/16,(iy+1)/16),c=g.profile((ix+1)/16,(iy+1)/16),d=g.profile((ix+1)/16,iy/16)
        let expected=fy>=fx ? a*(1-fy)+b*(fy-fx)+c*fx:a*(1-fx)+c*fy+d*(fx-fy)
        exactElevation = exactElevation && g.elevation(x,z)==expected
    }}
    try require(exactElevation,"cached contact grid matches original interpolation across slopes, triangles and map edges")
    let footprintUnit:Obj=["x":64,"y":64,"building":true,"footprintWidth":64,"footprintHeight":64]
    try require(footprintGround.elevation(2,2)==0 && footprintGround.buildingElevation(footprintUnit)==2,"building support includes elevated footprint corners beyond its center")
    let foundationSource=SpriteRelief.Mesh(vertices:[SCNVector3(-1,0,0),SCNVector3(1,0,0),SCNVector3(-1,1,0),SCNVector3(1,1,0)],uv:[CGPoint](repeating:.zero,count:4),indices:[0,1,2,1,3,2],baseHeights:[0,0,0,0],baseSamples:[0,1])
    var foundationTransform=simd_float4x4(simd_quatf(angle:-Float(Terrain3D.canonicalElevation),axis:SIMD3<Float>(1,0,0)))
    foundationTransform.columns.3=SIMD4<Float>(2,2,2,1)
    let foundation=SpriteRelief.conform(foundationSource,ground:footprintGround,transform:foundationTransform)
    let lowBase=foundationTransform*SIMD4<Float>(SIMD3<Float>(foundation.vertices[0]),1)
    let highBase=foundationTransform*SIMD4<Float>(SIMD3<Float>(foundation.vertices[1]),1)
    try require(abs(lowBase.y)<0.0001 && abs(highBase.y-2)<0.0001,"building base follows both low and high footprint samples")
    try require(foundation.vertices[2].y==foundationSource.vertices[2].y && foundation.vertices[3].y==foundationSource.vertices[3].y,"footprint conformance preserves the upper relief structure")
    try require(foundationSource.vertices[0].y==0,"footprint conformance cannot mutate a shared source mesh")
    var flyingFootprint=footprintUnit;flyingFootprint["airborne"]=true
    try require(footprintGround.buildingElevation(flyingFootprint)==0,"lifted buildings do not inherit landed footprint support")
    var continuous=true
    for y in 0..<commander.ground.height-1 {for x in 0..<commander.ground.width-1 {
        let a=commander.ground.surface(x,y),right=commander.ground.surface(x+1,y),below=commander.ground.surface(x,y+1)
        continuous = continuous && a[3]==right[0] && a[2]==right[1] && a[1]==below[0] && a[2]==below[3]
    }}
    try require(continuous,"all loaded cliff and ramp edges share both height endpoints")
    let red=commander.worldSprites.texture(239,frame:0,color:[255,0,0])!
    let blue=commander.worldSprites.texture(239,frame:0,color:[0,0,255])!
    let redPixels=NSBitmapImageRep(data:red.tiffRepresentation!)!,bluePixels=NSBitmapImageRep(data:blue.tiffRepresentation!)!
    var teamPixel=false
    for y in 0..<redPixels.pixelsHigh where !teamPixel {for x in 0..<redPixels.pixelsWide {
        let a=redPixels.colorAt(x:x,y:y)!.usingColorSpace(.deviceRGB)!,b=bluePixels.colorAt(x:x,y:y)!.usingColorSpace(.deviceRGB)!
        if a.redComponent>b.redComponent+0.1 && b.blueComponent>a.blueComponent+0.1 && abs(a.alphaComponent-b.alphaComponent)<0.001 {teamPixel=true;break}
    }}
    try require(teamPixel,"HD team mask changes color while preserving alpha and cache isolation")
    commander.requireFidelity=true
    let acceptedSignature=commander.modelLibrary.catalog.signatures[0]
    commander.modelLibrary.catalog.signatures[0]="unapproved-runtime-fixture"
    try require(!commander.modelLibrary.approvedForLive(0),"approval rejects a different model revision")
    commander.modelLibrary.catalog.signatures[0]=acceptedSignature
    var unapproved=unit("unapproved");unapproved["type"]=999
    commander.apply(state(1001,[unapproved]),[:])
    try require(commander.models.isEmpty && commander.billboards["unapproved"] != nil,"unapproved live model uses the HD image fallback")
    let residency=TextureResidency(limit:1024)
    try require(residency.reserve(768,owner:"terrain-pages") && !residency.reserve(512),"texture admission counts pinned owners across caches")
    try require((residency.report()["owners"] as? [String:Int])?["terrain-pages"]==768,"texture owners report admitted storage only")
    residency.release(768,owner:"terrain-pages")
    try require(residency.reserve(1024) && !residency.reserve(1),"texture admission reuses released capacity without overshoot")
    residency.release(1024)
    try require(number(residency.report(),"residentBytes")==0,"texture residency returns to zero after release")
    let textureBudget=TextureResidency(limit:8*1024*1024)
    var retainedView:MTLTexture?
    autoreleasepool {
        let descriptor=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.rgba8Unorm,width:64,height:64,mipmapped:false)
        let texture=textureBudget.make(commander.art.device,descriptor)
        retainedView=texture?.makeTextureView(pixelFormat:.rgba8Unorm_srgb)
    }
    try require(retainedView != nil && number(textureBudget.report(),"residentBytes")>0,"a retained view keeps its texture residency lease")
    retainedView=nil
    try require(number(textureBudget.report(),"residentBytes")==0,"final texture owner releases the residency lease")
    commander.fog.removeAll();commander.creepTiles.removeAll();commander.lastTerrainUpdate=nil
    let visiblePacket:Obj=["x":0,"y":0,"width":1,"height":1,"fog":"2","creep":"0"]
    commander.apply(state(2000,[]),visiblePacket)
    try require(commander.terrain[0] != nil,"visible terrain packet creates its tile")
    let stableTerrainRevision=commander.terrainRevision
    commander.apply(state(2001,[]),visiblePacket)
    try require(commander.terrainRevision==stableTerrainRevision,"unchanged terrain packet preserves geometry across native frames")
    commander.apply(state(2002,[]),["x":0,"y":0,"width":1,"height":1,"fog":"0","creep":"0"])
    try require(commander.terrain[0]==nil && commander.fog[0]==48,"changed fog packet removes hidden terrain without stale visibility")
    let terrainPacket:Obj=["x":0,"y":0,"width":3,"height":3,"fog":"222222222","creep":"000000000"]
    commander.apply(state(2003,[]),terrainPacket)
    let terrainBatches=try MetalBatches(device:commander.art.device)
    try terrainBatches.prepare(commander)
    let beforeTerrain=terrainBatches.batches.first(where:{$0.terrain})!
    commander.apply(state(2004,[]),["x":0,"y":0,"width":3,"height":3,"fog":"222212222","creep":"000000000"])
    try terrainBatches.prepare(commander)
    let afterTerrain=terrainBatches.batches.first(where:{$0.terrain})!
    try require(beforeTerrain.vertices === afterTerrain.vertices && beforeTerrain.textures[0] === afterTerrain.textures[0],"fog changes preserve terrain vertex buffers and diffuse arrays")
    try require(beforeTerrain.textures[1] !== afterTerrain.textures[1],"fog changes publish a new mask array without mutating the previous snapshot")
    let distantTerrain=SCNNode(geometry:commander.terrain[0]!.geometry!)
    distantTerrain.position=SCNVector3(16,0,0);commander.terrain[16]=distantTerrain;commander.terrainRevision+=1
    try terrainBatches.prepare(commander)
    let unchangedPage=terrainBatches.batches.filter{$0.terrain}.last!
    commander.terrain.removeValue(forKey:0);commander.terrainRevision+=1
    try terrainBatches.prepare(commander)
    let retainedPage=terrainBatches.batches.filter{$0.terrain}.last!
    try require(unchangedPage.vertices === retainedPage.vertices && unchangedPage.textures[0] === retainedPage.textures[0],"terrain topology changes preserve unrelated spatial page buffers and textures")
    let previous=state(2100,[unit("stable"),unit("changed")])
    var moved=unit("changed");moved["x"]=x+32
    let diff=PreparedState(state(2101,[unit("stable"),moved]),previous:previous)
    try require(diff.dirty==Set(["changed"]),"state diff preserves stationary units while marking movement")
    try require(PreparedState(state(2102,[unit("stable")],session:"next"),previous:previous).dirty==Set(["stable"]),"session changes invalidate otherwise unchanged units")
    var damaged=unit("stable");damaged["hp"]=1
    let nativePrevious=try JSONSerialization.jsonObject(with:JSONSerialization.data(withJSONObject:previous)) as! Obj
    let nativeChanged=try JSONSerialization.jsonObject(with:JSONSerialization.data(withJSONObject:state(2103,[damaged]))) as! Obj
    try require(PreparedState(nativeChanged,previous:nativePrevious).dirty==Set(["stable"]),"native dictionary comparison retains nonvisual unit changes after JSON decoding")
    let terrainWorker=try TerrainWorker.Builder()
    let workerCenter=commander.mapWidth+1
    var workerFog=[Int:UInt8]()
    for y in 0..<3 {for x in 0..<3 {workerFog[y*commander.mapWidth+x]=50}}
    let workerRequest=TerrainWorker.Request(session:"terrain-worker",center:SIMD2<Int>(1,1),radius:20,fog:workerFog,creep:[])
    let workerFirst=terrainWorker.apply(workerRequest)
    try require(workerFirst.terrain.count==9,"terrain worker constructs the visible topology")
    let immutableTile=workerFirst.terrain.first{$0.index==workerCenter}!
    let immutableMask=immutableTile.geometry.firstMaterial!.multiply.contents as AnyObject
    workerFog[workerCenter]=49
    let workerNext=terrainWorker.apply(TerrainWorker.Request(session:"terrain-worker",center:SIMD2<Int>(1,1),radius:20,fog:workerFog,creep:[]))
    try require(!workerNext.terrain.isEmpty && immutableTile.geometry.firstMaterial!.multiply.contents as AnyObject === immutableMask,"terrain worker mask changes preserve previously published geometry")
    try require(terrainWorker.apply(TerrainWorker.Request(session:"terrain-worker",center:SIMD2<Int>(1,1),radius:20,fog:workerFog,creep:[])).terrain.isEmpty,"terrain worker unchanged fog produces no geometry publication")
    workerFog[workerCenter]=48
    let workerHidden=terrainWorker.apply(TerrainWorker.Request(session:"terrain-worker",center:SIMD2<Int>(1,1),radius:20,fog:workerFog,creep:[]))
    try require(workerHidden.removedTerrain==[workerCenter],"terrain worker publishes hidden tile removal incrementally")
    let isolatedCamera=SCNNode();isolatedCamera.camera=SCNCamera()
    isolatedCamera.simdPosition=SIMD3<Float>(1,2,3)
    let cameraPacket=MetalBatches.CameraPacket(isolatedCamera)
    isolatedCamera.simdPosition=SIMD3<Float>(9,8,7)
    isolatedCamera.camera!.usesOrthographicProjection=true
    try require(cameraPacket.transform.columns.3==SIMD4<Float>(1,2,3,1) && !cameraPacket.orthographic,"typed camera packet retains its pose and projection mode independently of scene mutations")
    try scenePacketChecks(device:commander.art.device,require:require)
    try frameResourceChecks(device:commander.art.device,require:require)
    let report:Obj=["passed":true,"checks":passed,"source":"deterministic renderer inputs; not live-game acceptance"]
    let path=argument("--runtime-report").map{URL(fileURLWithPath:$0)} ?? runtime.appendingPathComponent("runtime-checks.json")
    try JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys]).write(to:path,options:.atomic)
    print("Runtime checks passed: \(passed.count)")
}
