import Cocoa
import Metal
import SceneKit
import simd
import CoreImage

final class DesktopSurface:NSView {
    override func makeBackingLayer()->CALayer {CAMetalLayer()}
    var metalLayer:CAMetalLayer {layer as! CAMetalLayer}
    override func layout() {super.layout();updateDrawableSize()}
    override func setFrameSize(_ size:NSSize) {super.setFrameSize(size);updateDrawableSize()}
    override func viewDidChangeBackingProperties() {super.viewDidChangeBackingProperties();updateDrawableSize()}
    func updateDrawableSize() {
        guard layer != nil else {return}
        let scale=window?.backingScaleFactor ?? 1
        metalLayer.contentsScale=scale
        metalLayer.drawableSize=CGSize(width:bounds.width*scale,height:bounds.height*scale)
    }
    override func hitTest(_ point:NSPoint)->NSView? {nil}
}
final class DesktopMetal:NSObject,CAMetalDisplayLinkDelegate {
    unowned let commander:Commander
    let surface:DesktopSurface,queue:MTLCommandQueue
    init(commander:Commander)throws {
        self.commander=commander
        surface=DesktopSurface(frame:commander.view.bounds)
        queue=commander.art.device.makeCommandQueue()!
        super.init()
        commander.metalBatches=try MetalBatches(device:commander.art.device)
        surface.wantsLayer=true
        surface.metalLayer.device=commander.art.device
        surface.metalLayer.pixelFormat = .bgra8Unorm_srgb;surface.metalLayer.framebufferOnly=false
        surface.autoresizingMask=[.width,.height]
        if let layer=surface.layer as? CAMetalLayer {
            layer.maximumDrawableCount=2
            layer.displaySyncEnabled=true
            layer.presentsWithTransaction=false
        }
        commander.view.isPlaying=false;commander.view.rendersContinuously=false;commander.view.scene=nil
        commander.view.addSubview(surface,positioned:.below,relativeTo:commander.label)
        surface.updateDrawableSize()
        DispatchQueue.main.async { [weak self] in self?.configureDisplayLink() }
    }
    private var displayLink:CAMetalDisplayLink?
    private var refreshRate=0
    private var displayScreen:NSScreen?
    private var displayFrames=0
    private func configureDisplayLink() {
        guard let screen=surface.window?.screen else {return}
        displayLink?.invalidate();refreshRate=0;displayScreen=screen
        let link=CAMetalDisplayLink(metalLayer:surface.metalLayer)
        link.delegate=self;link.preferredFrameLatency=1
        displayLink=link;updateRefreshRate();link.add(to:.main,forMode:.common)
    }
    deinit {displayLink?.invalidate()}
    private func updateRefreshRate() {
        guard let screen=surface.window?.screen,let link=displayLink else {return}
        let rate=max(1,screen.maximumFramesPerSecond)
        guard rate != refreshRate else {return}
        refreshRate=rate
        link.preferredFrameRateRange=CAFrameRateRange(minimum:Float(rate),maximum:Float(rate),preferred:Float(rate))
        fputs("Desktop display link: \(rate) Hz, two drawables, vsync enabled\n",stderr)
    }
    private let renderQueue=DispatchQueue(label:"soju.bw.desktop-frame",qos:.userInteractive)
    private let frameAdmission=DispatchSemaphore(value:1)
    private struct Input {
        let camera:MetalBatches.CameraPacket,viewport:SIMD2<Float>,mailbox:SceneMailbox?
        let fade:Float,vignette:Float,manual:URL?,metadata:Obj
    }
    func metalDisplayLink(_ link:CAMetalDisplayLink,needsUpdate update:CAMetalDisplayLink.Update) {
        if surface.window?.screen != displayScreen {configureDisplayLink();return}
        updateRefreshRate()
        displayFrames+=1
        if displayFrames%1000==1 {fputs("Desktop Metal link: renderLead=\((update.targetTimestamp-CACurrentMediaTime())*1000)ms presentationLead=\((update.targetPresentationTimestamp-CACurrentMediaTime())*1000)ms\n",stderr)}
        guard commander.initialized,let camera=commander.desktopCameraPacket,
            frameAdmission.wait(timeout:.now()) == .success else {return}
        let viewport=SIMD2<Float>(Float(surface.bounds.width),Float(surface.bounds.height))
        let manual=requestedCapture;requestedCapture=nil
        let metadata=(manual != nil || argument("--record-dir") != nil) ? commander.desktopCaptureMetadata():[:]
        let input=Input(camera:camera,viewport:viewport,mailbox:commander.sceneWorker?.mailbox,
            fade:commander.comfortFade,vignette:commander.motionVignette,manual:manual,metadata:metadata)
        presentation=Presentation(camera:camera,viewport:viewport)
        let delay=max(0,update.targetTimestamp-CACurrentMediaTime()-0.003)
        renderQueue.asyncAfter(deadline:.now()+delay) { [self] in
            defer {frameAdmission.signal()}
            autoreleasepool {draw(input,drawable:update.drawable)}
        }
    }
    var requestedCapture:URL?
    private var framePacket:ScenePacket?
    struct Presentation {let camera:MetalBatches.CameraPacket,viewport:SIMD2<Float>}
    private(set) var presentation:Presentation?
    let captureContext=CIContext()
    private func draw(_ input:Input,drawable:CAMetalDrawable) {
        let started=CACurrentMediaTime()
        guard let command=queue.makeCommandBuffer(),let batches=commander.metalBatches else {return}
        commander.frameMetrics.stage("desktopDrawableWait",milliseconds:(CACurrentMediaTime()-started)*1000)
        input.mailbox?.acquire(into:&framePacket)
        let camera=input.camera
        let position=camera.transform.columns.3
        do {
        if let packet=framePacket {batches.consume(packet)}else {batches.suspend()}
        try batches.render(command:command,output:drawable.texture,camera:camera,eye:SIMD3<Float>(position.x,position.y,position.z),rightTransform:camera.transform,rightProjection:camera.projection)
        batches.copyEye(command:command,to:drawable.texture,eye:0,fade:input.fade,vignette:input.vignette)
        } catch {command.commit();fputs("Desktop batches: \(error)\n",stderr);return}
        let index=commander.captureFrames.reserve(),manual=input.manual
        if index != nil || manual != nil {
            let metadata=input.metadata
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
