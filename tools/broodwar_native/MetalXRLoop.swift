import Cocoa
import Metal
import CoreImage
import simd
import os.signpost
#if SOJU_OPENXR
private func packetXRDraw(_ context:UnsafeMutableRawPointer?,_ texture:UnsafeMutableRawPointer?,_ queue:UnsafeMutableRawPointer?,_ eye:UnsafePointer<SojuXREye>?) {
    guard let context=context,let texture=texture,let queue=queue,let eye=eye else {return}
    Unmanaged<MetalXRLoop>.fromOpaque(context).takeUnretainedValue().draw(Unmanaged<AnyObject>.fromOpaque(texture).takeUnretainedValue() as! MTLTexture,
        Unmanaged<AnyObject>.fromOpaque(queue).takeUnretainedValue() as! MTLCommandQueue,eye.pointee)
}
private func packetXRControls(_ context:UnsafeMutableRawPointer?,_ input:UnsafePointer<SojuXRInput>?) {
    guard let context=context,let input=input else {return}
    Unmanaged<MetalXRLoop>.fromOpaque(context).takeUnretainedValue().enqueue(input.pointee)
}
final class MetalXRLoop {
    struct Body {let position:SIMD3<Float>,yaw:Float,scale:Float,tabletop:Bool}
    struct Feedback {let origin:SIMD3<Float>?,originYaw:Float,center:SIMD3<Float>?,camera:MetalBatches.CameraPacket?,image:CGImage?}
    private struct Status {
        let frame:Int,generation:Int,nativeFrame:Double,publishedAt:Double,units:Int,draws:Int,visible:Int,culled:Int,completionSlotsExhausted:Int
    }
    private let lock=NSLock(),ended=DispatchSemaphore(value:0)
    private var active=true,body=Body(position:.zero,yaw:0,scale:1,tabletop:false),reset=false
    private var feedback=Feedback(origin:nil,originYaw:0,center:nil,camera:nil,image:nil)
    private let inputs=XRInputQueue()
    private var failure=""
    private let mailbox:SceneMailbox,batches:MetalBatches,metrics:XRFrameMetrics,captures:CaptureFrames,completionPool:GPUCompletionPool
    private var capturePool:FrameCapturePool!
    private var origin:SIMD3<Float>?,originYaw:Float=0,frameBody:Body?,framePacket:ScenePacket?
    private var outputs=[ObjectIdentifier:MTLTexture](),frame=0,lastMirror=0.0,captureIndex:Int?
    private let diagnostics=DispatchQueue(label:"soju.bw.xr-packet-diagnostics",qos:.utility)
    private var status:Status?,reporter:DispatchSourceTimer?
    private let stereoCapture=CommandLine.arguments.contains("--record-stereo")
    init(mailbox:SceneMailbox,batches:MetalBatches,metrics:XRFrameMetrics,captures:CaptureFrames,device:MTLDevice) {
        self.mailbox=mailbox;self.batches=batches;self.metrics=metrics;self.captures=captures;completionPool=GPUCompletionPool(metrics:metrics)
        batches.frameMetrics=metrics
        capturePool=FrameCapturePool(device:device,captures:captures,metrics:metrics) { [weak self] image in
            guard let self=self else {return}
            self.lock.lock();let old=self.feedback
            self.feedback=Feedback(origin:old.origin,originYaw:old.originYaw,center:old.center,camera:old.camera,image:image);self.lock.unlock()
        }
        let timer=DispatchSource.makeTimerSource(queue:diagnostics)
        timer.schedule(deadline:.now()+1,repeating:1)
        timer.setEventHandler { [weak self] in self?.report() };timer.resume();reporter=timer
    }
    deinit {reporter?.cancel()}
    private func report() {
        lock.lock();let snapshot=status,error=failure;lock.unlock()
        guard let snapshot=snapshot else {return}
        let capture=capturePool.statistics()
        let data:Obj=["xrFrame":snapshot.frame,"frame":snapshot.nativeFrame,"packetGeneration":snapshot.generation,
            "packetNativeFrame":snapshot.nativeFrame,"packetAgeMs":(CACurrentMediaTime()-snapshot.publishedAt)*1000,
            "unitCount":snapshot.units,"batchDraws":snapshot.draws,"batchVisible":snapshot.visible,"batchCulled":snapshot.culled,
            "renderer":"native-metal-packet","sceneWorkerError":mailbox.error(),"xrOwnerError":error,
            "completionSlotsExhausted":snapshot.completionSlotsExhausted,"captureTexturesCreated":capture.textureCreations,
            "capturePoolExhausted":capture.exhausted,"captureConverted":capture.completed,"captureConversionFailed":capture.failed,
            "captureSaveFailed":capture.saveFailed,"mirrorFrame":capture.mirrorFrame,
            "precreatedMetricHandlers":16,"precreatedCaptureHandlers":3]
        if let json=try? JSONSerialization.data(withJSONObject:data,options:.prettyPrinted) {try? json.write(to:runtime.appendingPathComponent("xr-view-diagnostics.json"),options:.atomic)}
    }
    func update(_ value:Body) {lock.lock();body=value;lock.unlock()}
    func recenter() {lock.lock();reset=true;lock.unlock()}
    func start() {Thread.detachNewThread { [self] in run() }}
    func stop() {lock.lock();active=false;lock.unlock();_ = ended.wait(timeout:.now()+2)}
    func takeInput()->SojuXRInput? {inputs.take()}
    func error()->String {lock.lock();defer{lock.unlock()};return failure}
    func latest()->Feedback {lock.lock();defer{lock.unlock()};return feedback}
    fileprivate func enqueue(_ input:SojuXRInput) {
        guard inputs.submit(input) else {
            lock.lock();active=false;failure="XR button event backlog exceeded 127 entries; session stopped before dropping commands";lock.unlock()
            fputs("XR input button-event queue overflow; session stopped\n",stderr);return
        }
    }
    private func run() {
        pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE,0)
        pthread_setname_np("soju.bw.xr-frame")
        defer {soju_xr_close();ended.signal()}
        while true {
            lock.lock();let running=active;lock.unlock();if !running {return}
            autoreleasepool {
                os_signpost(.begin,log:XRFrameMetrics.log,name:"Frame loop")
                metrics.beginFrameAllocationSample()
                let result=soju_xr_step(Unmanaged.passUnretained(self).toOpaque(),packetXRDraw,packetXRControls)
                metrics.endFrameAllocationSample()
                var timing=SojuXRFrameTiming()
                if result==1,soju_xr_frame_timing(&timing) != 0 {metrics.lifecycle(wait:timing.waitMs,acquire:timing.acquireMs,end:timing.endMs,afterWait:timing.afterWaitMs,period:timing.periodMs)}
                os_signpost(.end,log:XRFrameMetrics.log,name:"Frame loop")
                if result==0 {lock.lock();active=false;lock.unlock()}
                if result==2 {Thread.sleep(forTimeInterval:0.01)}
            }
        }
    }
    fileprivate func draw(_ texture:MTLTexture,_ queue:MTLCommandQueue,_ eye:SojuXREye) {
        let start=CACurrentMediaTime()
        if eye.eye==0 {
            lock.lock();frameBody=body;if reset {origin=nil;reset=false};lock.unlock()
            mailbox.acquire(into:&framePacket);frame+=1;metrics.begin(units:framePacket?.unitPositions.count ?? 0)
            if origin==nil && eye.tracked != 0 {
                origin=SIMD3<Float>(eye.headX,eye.headY,eye.headZ)
                let forward=simd_quatf(ix:eye.qx,iy:eye.qy,iz:eye.qz,r:eye.qw).act(SIMD3<Float>(0,0,-1))
                originYaw=atan2(-forward.x,-forward.z)
            }
        }
        guard let body=frameBody else {return}
        metrics.created(.commandBuffer)
        guard let command=queue.makeCommandBuffer() else {return}
        let rotation=simd_quatf(angle:body.yaw-originYaw,axis:SIMD3<Float>(0,1,0))*simd_quatf(angle:body.tabletop ? -0.75:0,axis:SIMD3<Float>(1,0,0))
        func camera(_ eye:SojuXREye)->MetalBatches.CameraPacket {
            let position=body.position+rotation.act((SIMD3<Float>(eye.px,eye.py,eye.pz)-(origin ?? .zero))*body.scale)
            let orientation=rotation*simd_quatf(ix:eye.qx,iy:eye.qy,iz:eye.qz,r:eye.qw)
            var transform=simd_float4x4(orientation);transform.columns.3=SIMD4<Float>(position,1)
            let l=tan(eye.left),r=tan(eye.right),u=tan(eye.up),d=tan(eye.down),near:Float=0.03*body.scale,far:Float=150*body.scale
            var projection=matrix_identity_float4x4
            projection.columns.0.x=2/(r-l);projection.columns.1.y=2/(u-d)
            projection.columns.2=SIMD4<Float>((r+l)/(r-l),(u+d)/(u-d),-(far+near)/(far-near),-1)
            projection.columns.3=SIMD4<Float>(0,0,-2*far*near/(far-near),0)
            return MetalBatches.CameraPacket(transform:transform,projection:projection,orientation:orientation)
        }
        let left=camera(eye),position=left.transform.columns.3
        let output:MTLTexture
        if texture.pixelFormat == .bgra8Unorm {
            let key=ObjectIdentifier(texture)
            if let existing=outputs[key] {output=existing}
            else {metrics.created(.textureView);guard let view=texture.makeTextureView(pixelFormat:.bgra8Unorm_srgb) else {return};outputs[key]=view;output=view}
        } else {output=texture}
        metrics.stage("eyeSetup",milliseconds:(CACurrentMediaTime()-start)*1000)
        let renderStart=CACurrentMediaTime()
        do {
            if eye.eye==0 {
                if let packet=framePacket {batches.consume(packet)}else {batches.suspend()}
                var rightEye=SojuXREye();_ = soju_xr_eye(1,&rightEye)
                let right=camera(rightEye)
                try batches.render(command:command,output:output,camera:left,eye:SIMD3<Float>(position.x,position.y,position.z),rightTransform:right.transform,rightProjection:right.projection)
                let center=body.position+rotation.act((SIMD3<Float>(eye.headX,eye.headY,eye.headZ)-(origin ?? .zero))*body.scale)
                lock.lock();feedback=Feedback(origin:origin,originYaw:originYaw,center:center,camera:left,image:feedback.image);lock.unlock()
            }
            batches.copyEye(command:command,to:output,eye:Int(eye.eye))
        } catch {fputs("XR packet encode: \(error)\n",stderr);command.commit();return}
        metrics.stage("renderEncodeEye",milliseconds:(CACurrentMediaTime()-renderStart)*1000)
        if eye.eye==0 {captureIndex=framePacket==nil ? nil:captures.reserve()}
        let index=(eye.eye==0 || stereoCapture) ? captureIndex:nil
        let mirrorDue=eye.eye==0 && start-lastMirror>=0.5
        if mirrorDue || index != nil {
            if mirrorDue {lastMirror=start}
            capturePool.enqueue(command,source:texture,request:FrameCapturePool.Request(packet:framePacket,camera:left,frame:frame,eye:Int(eye.eye),index:index,mirror:mirrorDue,scale:body.scale))
        }
        var timing=framePacket?.timing ?? SceneTiming();timing.predictedDisplay=eye.predictedHostTime
        completionPool.attach(command,frame:frame,eye:Int(eye.eye),timing:timing)
        command.commit()
        if eye.eye==1 {
            metrics.submitted()
            lock.lock();status=Status(frame:frame,generation:framePacket?.generation ?? -1,nativeFrame:framePacket?.nativeFrame ?? 0,
                publishedAt:framePacket?.publishedAt ?? start,units:framePacket?.unitPositions.count ?? 0,draws:batches.draws,visible:batches.visible,culled:batches.culled,completionSlotsExhausted:completionPool.exhausted);lock.unlock()
        }
    }
}
#endif
