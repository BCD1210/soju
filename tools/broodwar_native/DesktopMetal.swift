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
    let surface:DesktopSurface,queue:MTLCommandQueue,overlay:SCNRenderer
    init(commander:Commander)throws {
        self.commander=commander
        surface=DesktopSurface(frame:commander.view.bounds,device:commander.art.device)
        queue=commander.art.device.makeCommandQueue()!
        overlay=SCNRenderer(device:commander.art.device,options:nil)
        super.init()
        commander.metalBatches=try MetalBatches(device:commander.art.device)
        surface.colorPixelFormat = .bgra8Unorm_srgb;surface.framebufferOnly=false
        surface.autoresizingMask=[.width,.height];surface.preferredFramesPerSecond=90
        surface.delegate=self
        overlay.scene=commander.world;overlay.pointOfView=commander.camera;overlay.usesReverseZ=false
        commander.view.isPlaying=false;commander.view.rendersContinuously=false
        commander.view.addSubview(surface,positioned:.below,relativeTo:commander.label)
    }
    func mtkView(_ view:MTKView,drawableSizeWillChange size:CGSize) {}
    var requestedCapture:URL?
    let captureContext=CIContext()
    func draw(in view:MTKView) {
        guard commander.initialized,let drawable=view.currentDrawable,let command=queue.makeCommandBuffer(),let batches=commander.metalBatches else {return}
        let camera=commander.camera,aspect=Float(view.drawableSize.width/view.drawableSize.height)
        let near=Float(camera.camera!.zNear),far=Float(camera.camera!.zFar)
        var projection=matrix_identity_float4x4
        if camera.camera!.usesOrthographicProjection {
            let scale=Float(camera.camera!.orthographicScale)
            projection.columns.0.x=1/(scale*aspect);projection.columns.1.y=1/scale
            projection.columns.2.z = -2/(far-near);projection.columns.3.z = -(far+near)/(far-near)
        } else {
            let f=1/tan(Float(camera.camera!.fieldOfView)*Float.pi/360)
            projection.columns.0.x=f/aspect;projection.columns.1.y=f
            projection.columns.2=SIMD4<Float>(0,0,-(far+near)/(far-near),-1)
            projection.columns.3=SIMD4<Float>(0,0,-2*far*near/(far-near),0)
        }
        camera.camera!.projectionTransform=SCNMatrix4(projection)
        do {
        batches.prepareAsync(commander)
        try batches.render(command:command,output:drawable.texture,camera:camera,eye:camera.simdWorldPosition,rightTransform:camera.simdWorldTransform,rightProjection:projection)
        batches.copyEye(command:command,to:drawable.texture,eye:0)
        try batches.renderOverlay(renderer:overlay,command:command,output:drawable.texture,camera:camera,eye:0,time:CACurrentMediaTime())
        } catch {command.commit();fputs("Desktop batches: \(error)\n",stderr);return}
        let index=commander.captureFrames.reserve(),manual=requestedCapture
        requestedCapture=nil
        if index != nil || manual != nil {
            let metadata=commander.desktopCaptureMetadata()
            let descriptor=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:drawable.texture.pixelFormat,width:drawable.texture.width,height:drawable.texture.height,mipmapped:false)
            descriptor.usage = .shaderRead;descriptor.storageMode = .private
            guard let texture=TextureResidency.shared.make(commander.art.device,descriptor),let blit=command.makeBlitCommandEncoder() else {command.present(drawable);command.commit();return}
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
        command.present(drawable);command.commit()
    }
}
