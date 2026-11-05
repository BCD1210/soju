import Cocoa
import MetalKit
import SceneKit
import simd
import CoreImage

final class DesktopSurface:MTKView {
    override func hitTest(_ point:NSPoint)->NSView? {nil}
}
final class DesktopMetal:NSObject,MTKViewDelegate {
    unowned let commander:Commander
    let surface:DesktopSurface,queue:MTLCommandQueue
    init(commander:Commander)throws {
        self.commander=commander
        surface=DesktopSurface(frame:commander.view.bounds,device:commander.art.device)
        queue=commander.art.device.makeCommandQueue()!
        super.init()
        commander.metalBatches=try MetalBatches(device:commander.art.device)
        surface.colorPixelFormat = .bgra8Unorm_srgb;surface.framebufferOnly=false
        surface.autoresizingMask=[.width,.height];surface.preferredFramesPerSecond=90
        surface.delegate=self
        commander.view.isPlaying=false;commander.view.rendersContinuously=false;commander.view.scene=nil
        commander.view.addSubview(surface,positioned:.below,relativeTo:commander.label)
    }
    func mtkView(_ view:MTKView,drawableSizeWillChange size:CGSize) {}
    var requestedCapture:URL?
    private var framePacket:ScenePacket?
    struct Presentation {let camera:MetalBatches.CameraPacket,viewport:SIMD2<Float>}
    private(set) var presentation:Presentation?
    let captureContext=CIContext()
    func draw(in view:MTKView) {
        let started=CACurrentMediaTime()
        guard commander.initialized,let drawable=view.currentDrawable,let command=queue.makeCommandBuffer(),let batches=commander.metalBatches else {return}
        commander.frameMetrics.stage("desktopDrawableWait",milliseconds:(CACurrentMediaTime()-started)*1000)
        commander.sceneWorker?.mailbox.acquire(into:&framePacket)
        guard let camera=commander.desktopCameraPacket else {return}
        let position=camera.transform.columns.3
        do {
        if let packet=framePacket {batches.consume(packet)}else {batches.suspend()}
        try batches.render(command:command,output:drawable.texture,camera:camera,eye:SIMD3<Float>(position.x,position.y,position.z),rightTransform:camera.transform,rightProjection:camera.projection)
        batches.copyEye(command:command,to:drawable.texture,eye:0,fade:commander.comfortFade,vignette:commander.motionVignette)
        presentation=Presentation(camera:camera,viewport:SIMD2<Float>(Float(commander.view.bounds.width),Float(commander.view.bounds.height)))
        } catch {command.commit();fputs("Desktop batches: \(error)\n",stderr);return}
        let index=commander.captureFrames.reserve(),manual=requestedCapture
        requestedCapture=nil
        if index != nil || manual != nil {
            let metadata=commander.desktopCaptureMetadata()
            let descriptor=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:drawable.texture.pixelFormat,width:drawable.texture.width,height:drawable.texture.height,mipmapped:false)
            descriptor.usage = .shaderRead;descriptor.storageMode = .private
            guard let texture=TextureResidency.shared.make(commander.art.device,descriptor,owner:"captures"),let blit=command.makeBlitCommandEncoder() else {present(command,drawable);return}
            blit.copy(from:drawable.texture,to:texture);blit.endEncoding()
            command.addCompletedHandler { [weak self] completed in
                guard completed.status == .completed,let self=self,
                    let ci=CIImage(mtlTexture:texture,options:nil),
                    let image=self.captureContext.createCGImage(ci.transformed(by:CGAffineTransform(translationX:0,y:CGFloat(texture.height)).scaledBy(x:1,y:-1)),from:ci.extent) else {return}
                if let index=index {self.commander.captureFrames.save(image,index:index,metadata:metadata)}
                if let folder=manual {
                    try? FileManager.default.createDirectory(at:folder,withIntermediateDirectories:true)
                    let file=folder.appendingPathComponent("frame-\(Int(number(metadata,"frame")))")
                    try? NSBitmapImageRep(cgImage:image).representation(using:.png,properties:[:])?.write(to:file.appendingPathExtension("png"))
                    try? JSONSerialization.data(withJSONObject:metadata,options:.prettyPrinted).write(to:file.appendingPathExtension("json"))
                }
            }
        }
        let timing=framePacket?.timing ?? SceneTiming(),metrics=commander.frameMetrics
        metrics.begin(units:framePacket?.unitPositions.count ?? 0)
        command.addCompletedHandler { completed in
            if completed.status == .completed {metrics.latencySample(timing,completed:completed.gpuEndTime)}
        }
        let submitted=CACurrentMediaTime()
        drawable.addPresentedHandler { shown in
            metrics.presented(timing,at:shown.presentedTime)
            metrics.stage("desktopSubmitToPresented",milliseconds:(shown.presentedTime-submitted)*1000)
        }
        present(command,drawable)
    }
    private func present(_ command:MTLCommandBuffer,_ drawable:CAMetalDrawable) {
        command.present(drawable);command.commit()
    }
}
