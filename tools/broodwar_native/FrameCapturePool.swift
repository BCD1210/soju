import Cocoa
import Metal
import CoreImage

// Texture and completion slots stay leased through GPU copy AND image conversion.
// The fixed dispatch source moves conversion/PNG/metadata work off the frame owner.
final class FrameCapturePool {
    struct Request {
        let packet:ScenePacket?,camera:MetalBatches.CameraPacket
        let frame:Int,eye:Int,index:Int?,mirror:Bool,scale:Float
    }
    struct Statistics {let textureCreations:Int,exhausted:Int,completed:Int,failed:Int,saveFailed:Int,mirrorFrame:Int}
    private final class Slot {
        let available=DispatchSemaphore(value:1),descriptor=MTLTextureDescriptor()
        var texture:MTLTexture?,request:Request?,ready=false,success=false
        var handler:MTLCommandBufferHandler!
        init() {descriptor.textureType = .type2D;descriptor.usage = .shaderRead;descriptor.storageMode = .private}
    }
    private let slots=(0..<3).map{_ in Slot()}
    private let lock=NSLock(),queue=DispatchQueue(label:"soju.bw.capture-conversion",qos:.utility)
    private let signal:DispatchSourceUserDataAdd
    private let context:CIContext,captures:CaptureFrames,metrics:XRFrameMetrics,mirror:(CGImage)->Void
    private let snapshotSource=argument("--snapshot") ?? "live"
    private var textureCreations=0,exhausted=0,completed=0,failed=0,saveFailed=0,lastMirrorFrame = -1
    private var prewarmed=false
    init(device:MTLDevice,captures:CaptureFrames,metrics:XRFrameMetrics,mirror:@escaping(CGImage)->Void) {
        context=CIContext(mtlDevice:device);self.captures=captures;self.metrics=metrics;self.mirror=mirror
        signal=DispatchSource.makeUserDataAddSource(queue:queue)
        signal.setEventHandler { [weak self] in self?.drain() }
        for slot in slots {
            slot.handler={ [weak self,weak slot] command in
                guard let self=self,let slot=slot else {return}
                self.lock.lock();slot.success=command.status == .completed;slot.ready=true;self.lock.unlock()
                self.signal.add(data:1)
            }
        }
        signal.resume()
    }
    deinit {signal.cancel()}
    func statistics()->Statistics {
        lock.lock();defer{lock.unlock()}
        return Statistics(textureCreations:textureCreations,exhausted:exhausted,completed:completed,failed:failed,saveFailed:saveFailed,mirrorFrame:lastMirrorFrame)
    }
    @discardableResult func enqueue(_ command:MTLCommandBuffer,source:MTLTexture,request:Request)->Bool {
        if !prewarmed {
            // No slot can be in flight until the complete startup pool exists.
            for slot in slots where !prepare(slot,source:source) {
                lock.lock();failed+=1;lock.unlock();return false
            }
            prewarmed=true
        }
        for slot in slots {
            guard slot.available.wait(timeout:.now()) == .success else {continue}
            guard prepare(slot,source:source),let texture=slot.texture else {
                lock.lock();failed+=1;lock.unlock();slot.available.signal();return false
            }
            metrics.created(.blitEncoder)
            guard let blit=command.makeBlitCommandEncoder() else {
                lock.lock();failed+=1;lock.unlock();slot.available.signal();return false
            }
            slot.request=request
            blit.copy(from:source,to:texture);blit.endEncoding();command.addCompletedHandler(slot.handler);return true
        }
        lock.lock();exhausted+=1;lock.unlock();return false
    }
    private func prepare(_ slot:Slot,source:MTLTexture)->Bool {
        if slot.texture?.width==source.width && slot.texture?.height==source.height && slot.texture?.pixelFormat==source.pixelFormat {return true}
        slot.descriptor.width=source.width;slot.descriptor.height=source.height;slot.descriptor.pixelFormat=source.pixelFormat
        metrics.created(.captureTexture)
        slot.texture=TextureResidency.shared.make(source.device,slot.descriptor,owner:"captures")
        guard slot.texture != nil else {return false}
        lock.lock();textureCreations+=1;lock.unlock();return true
    }
    private func drain() {
        for slot in slots {
            lock.lock();let ready=slot.ready,success=slot.success;slot.ready=false;lock.unlock()
            guard ready else {continue}
            var converted=false
            autoreleasepool {
                if success,let texture=slot.texture,let request=slot.request,
                   let ci=CIImage(mtlTexture:texture,options:nil),
                   let image=context.createCGImage(ci.transformed(by:CGAffineTransform(translationX:0,y:CGFloat(texture.height)).scaledBy(x:1,y:-1)),from:ci.extent) {
                    lock.lock();let publishMirror=request.mirror && request.frame>lastMirrorFrame
                    if publishMirror {lastMirrorFrame=request.frame};lock.unlock()
                    if publishMirror {mirror(image)}
                    if let index=request.index {
                        let position=request.camera.transform.columns.3,orientation=request.camera.orientation
                        var metadata=request.packet?.diagnostics ?? [:]
                        metadata["xrFrame"]=request.frame;metadata["eye"]=request.eye;metadata["source"]="OpenXR Metal scene packet"
                        metadata["snapshotSource"]=snapshotSource;metadata["worldUnitsPerMeter"]=request.scale
                        metadata["camera"]=[position.x,position.y,position.z]
                        metadata["orientation"]=[orientation.imag.x,orientation.imag.y,orientation.imag.z,orientation.real]
                        metadata["packetGeneration"]=request.packet?.generation ?? -1
                        if !captures.save(image,index:index,metadata:metadata,suffix:request.eye==0 ? "":"-right") {lock.lock();saveFailed+=1;lock.unlock()}
                    }
                    converted=true
                }
                slot.request=nil
            }
            slot.available.signal()
            lock.lock();if converted {completed+=1}else {failed+=1};lock.unlock()
        }
    }
}
