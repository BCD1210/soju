import Cocoa
import Metal
import SceneKit
import simd

func frameResourceChecks(device:MTLDevice,require:(Bool,String)throws->Void)throws {
    let ordinaryMetrics=XRFrameMetrics(allocationSampling:false),allocationTrace=XRFrameMetrics(allocationSampling:true)
    ordinaryMetrics.beginFrameAllocationSample();ordinaryMetrics.endFrameAllocationSample()
    allocationTrace.beginFrameAllocationSample();allocationTrace.endFrameAllocationSample()
    try require(ordinaryMetrics.frameAllocationSamples==0 && allocationTrace.frameAllocationSamples==1,"frame-loop malloc sampling is opt-in and absent from ordinary cadence measurements")
    let renderer=try MetalBatches(device:device)
    let payload=SpriteRelief.Mesh(vertices:[SCNVector3(-1,-1,0),SCNVector3(1,-1,0),SCNVector3(0,1,0)],
        uv:[CGPoint(x:0,y:0),CGPoint(x:1,y:0),CGPoint(x:0.5,y:1)],indices:[0,1,2])
    let owner=WorldSprites(streaming:true)
    let sharedA=owner.geometry(payload,id:1,frame:0),sharedB=owner.geometry(payload,id:1,frame:0)
    sharedB.elements[0].primitiveRange=NSRange(location:0,length:1)
    sharedA.elements[0].primitiveRange=NSRange(location:0,length:0)
    try require(sharedA !== sharedB && sharedA.sources(for:.vertex).first === sharedB.sources(for:.vertex).first && sharedB.elements[0].primitiveRange.length == 1,"scene-owned geometry shares immutable vertices while keeping mutable elements isolated")
    final class MeshResults {
        let lock=NSLock();var checks=0,failures=0
        func record(_ okay:Bool) {lock.lock();checks+=1;if !okay {failures+=1};lock.unlock()}
    }
    let results=MeshResults()
    DispatchQueue.concurrentPerform(iterations:2) { _ in
        guard let consumer=try? MetalBatches(device:device) else {results.record(false);return}
        let cache=ResidentCache<SpriteRelief.Mesh>(limit:payload.bytes)
        for iteration in 0..<512 {autoreleasepool {
            cache.insert(payload,key:"frame",bytes:payload.bytes)
            let saved=cache.get("frame")!
            cache.insert(payload,key:"replacement",bytes:payload.bytes)
            let source=saved.geometry(),owned=saved.geometry()
            owned.elements[0].primitiveRange=NSRange(location:0,length:1)
            let originalRange=owned.elements[0].primitiveRange
            source.elements[0].primitiveRange=NSRange(location:0,length:0)
            let extracted=consumer.mesh(owned)
            results.record(cache.get("frame")==nil && owned.elements[0].primitiveRange==originalRange && extracted.0.count==3 && extracted.1==[0,1,2] && extracted.0[0].position.x == -1)
            if iteration==511 {print("Relief owner extraction: 512 evictions and reconstructions completed")}
        }}
    }
    try require(results.checks==1024 && results.failures==0,"two concurrent relief consumers survive 1024 cache evictions and independent geometry mutations")
    let plane=SCNPlane(width:0.9,height:0.065)
    let (planeMesh,_)=renderer.mesh(plane)
    let width=planeMesh.map{$0.position.x}.max()!-planeMesh.map{$0.position.x}.min()!
    let height=planeMesh.map{$0.position.y}.max()!-planeMesh.map{$0.position.y}.min()!
    try require(abs(width-0.9)<0.00001 && abs(height-0.065)<0.00001,"primitive extraction preserves non-square health-bar plane dimensions")
    let (sphereMesh,_)=renderer.mesh(SCNSphere(radius:0.065))
    let radius=sphereMesh.map{simd_length(SIMD3<Float>($0.position.x,$0.position.y,$0.position.z))}.max()!
    try require(abs(radius-0.065)<0.00001,"procedural projectile sphere extraction preserves requested radius")
    let (ringMesh,_)=renderer.mesh(SCNTorus(ringRadius:0.45,pipeRadius:0.018))
    let radii=ringMesh.map{hypot($0.position.x,$0.position.z)},tube=ringMesh.map{abs($0.position.y)}.max()!
    try require(abs(radii.min()!-0.432)<0.00001 && abs(radii.max()!-0.468)<0.00001 && abs(tube-0.018)<0.00001,"selection torus extraction preserves both ring radius and tube thickness")

    let material=SCNMaterial();material.diffuse.contents=NSColor(srgbRed:1,green:0,blue:0,alpha:1);material.readsFromDepthBuffer=false
    plane.materials=[material]
    try renderer.prepare(MetalBatches.Input(sceneRevision:1,terrainRevision:1,terrainFogRevision:1,mapWidth:1,terrain:[:],billboards:[],overlays:[SCNNode(geometry:plane)],objectCount:0))
    let descriptor=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.bgra8Unorm_srgb,width:256,height:256,mipmapped:false)
    descriptor.usage=[.renderTarget,.shaderRead];descriptor.storageMode = .private
    let output=device.makeTexture(descriptor:descriptor)!,readback=device.makeBuffer(length:256*1024,options:.storageModeShared)!
    var projection=matrix_identity_float4x4;projection.columns.2.z = -1
    var transform=matrix_identity_float4x4;transform.columns.3.z=0.5
    let camera=MetalBatches.CameraPacket(transform:transform,projection:projection,orientation:simd_quatf(),orthographic:true)
    let command=renderer.queue.makeCommandBuffer()!
    try renderer.render(command:command,output:output,camera:camera,eye:SIMD3<Float>(0,0,0.5),rightTransform:transform,rightProjection:projection)
    renderer.copyEye(command:command,to:output,eye:0)
    let blit=command.makeBlitCommandEncoder()!
    blit.copy(from:output,sourceSlice:0,sourceLevel:0,sourceOrigin:MTLOrigin(),sourceSize:MTLSize(width:256,height:256,depth:1),to:readback,destinationOffset:0,destinationBytesPerRow:1024,destinationBytesPerImage:256*1024)
    blit.endEncoding();command.commit();command.waitUntilCompleted()
    let bytes=readback.contents().bindMemory(to:UInt8.self,capacity:256*1024)
    var minX=256,maxX = -1,minY=256,maxY = -1
    for y in 0..<256 {for x in 0..<256 {let i=y*1024+x*4;if bytes[i+2]>200 && bytes[i+1]<80 {minX=min(minX,x);maxX=max(maxX,x);minY=min(minY,y);maxY=max(maxY,y)}}}
    let pixelsWide=maxX-minX+1,pixelsHigh=maxY-minY+1
    print("Health bar GPU bounds: \(pixelsWide)x\(pixelsHigh) pixels")
    try require(command.status == .completed && (110...118).contains(pixelsWide) && (6...10).contains(pixelsHigh),"GPU health-bar readback preserves a wide thin plane instead of a normalized square")

    let metrics=XRFrameMetrics(),completion=GPUCompletionPool(metrics:metrics)
    let held=(0..<16).map{_ in renderer.queue.makeCommandBuffer()!}
    var accepted=0
    for (index,buffer) in held.enumerated() {if completion.attach(buffer,frame:index/2,eye:index%2) {accepted+=1}}
    try require(accepted==16,"GPU completion pool leases all sixteen available slots")
    let overflow=renderer.queue.makeCommandBuffer()!
    try require(!completion.attach(overflow,frame:10,eye:0) && completion.exhausted==1,"GPU completion slots reject reuse while all prior commands are in flight")
    for buffer in held {buffer.commit()};for buffer in held {buffer.waitUntilCompleted()}
    let deadline=Date().addingTimeInterval(5)
    var reused=false
    while !reused && Date()<deadline {reused=completion.attach(overflow,frame:10,eye:0);if !reused {RunLoop.current.run(until:Date().addingTimeInterval(0.001))}}
    try require(reused,"GPU completion slot becomes reusable after completion")
    overflow.commit();overflow.waitUntilCompleted()

    final class Captured {
        let lock=NSLock()
        var count=0,dimensions=true,offMain=true
        func receive(_ image:CGImage) {lock.lock();count+=1;dimensions = dimensions && image.width==256 && image.height==256;offMain = offMain && !Thread.isMainThread;lock.unlock()}
        func result()->(Int,Bool,Bool) {lock.lock();defer{lock.unlock()};return (count,dimensions,offMain)}
    }
    let captured=Captured(),captures=CaptureFrames()
    let pool=FrameCapturePool(device:device,captures:captures,metrics:metrics,mirror:captured.receive)
    func request(_ frame:Int)->FrameCapturePool.Request {FrameCapturePool.Request(packet:nil,camera:camera,frame:frame,eye:0,index:nil,mirror:true,scale:1)}
    let copies=(0..<3).map{_ in renderer.queue.makeCommandBuffer()!}
    let firstAccepted=pool.enqueue(copies[0],source:output,request:request(1))
    try require(firstAccepted && pool.statistics().textureCreations==3,"first capture preallocates all three textures before any slot is in flight")
    let acceptedCopies=(firstAccepted ? 1:0)+copies.dropFirst().enumerated().reduce(0){$0+(pool.enqueue($1.element,source:output,request:request($1.offset+2)) ? 1:0)}
    try require(acceptedCopies==3,"capture pool leases all three available texture slots")
    let fourth=renderer.queue.makeCommandBuffer()!
    try require(!pool.enqueue(fourth,source:output,request:request(4)) && pool.statistics().exhausted==1,"capture pool refuses overwrite before GPU copy and conversion complete")
    for buffer in copies {buffer.commit()}
    let captureDeadline=Date().addingTimeInterval(10)
    while pool.statistics().completed<3 && Date()<captureDeadline {RunLoop.current.run(until:Date().addingTimeInterval(0.01))}
    let result=captured.result()
    try require(pool.statistics().mirrorFrame==3 && result.0>0 && result.1 && result.2,"capture conversion publishes the newest complete image off the main and frame owner")
    try require(pool.statistics().textureCreations==3 && pool.enqueue(fourth,source:output,request:request(4)),"completed capture slot reuses its retained texture")
    fourth.commit();fourth.waitUntilCompleted()
    while pool.statistics().completed<4 && Date()<captureDeadline {RunLoop.current.run(until:Date().addingTimeInterval(0.01))}
    try require(pool.statistics().completed==4 && pool.statistics().textureCreations==3 && pool.statistics().failed==0,"repeated capture keeps the fixed three-texture allocation bound")
    let publishedBeforeDelay=captured.result().0
    let delayed=renderer.queue.makeCommandBuffer()!
    try require(pool.enqueue(delayed,source:output,request:request(2)),"capture pool accepts a delayed older capture for conversion")
    delayed.commit();delayed.waitUntilCompleted()
    while pool.statistics().completed<5 && Date()<captureDeadline {RunLoop.current.run(until:Date().addingTimeInterval(0.01))}
    try require(pool.statistics().completed==5 && pool.statistics().mirrorFrame==4 && captured.result().0==publishedBeforeDelay,"late older capture conversion never replaces a newer mirror frame")
}
