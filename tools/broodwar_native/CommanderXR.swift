import Cocoa
import SceneKit
import Metal
import QuartzCore
import CoreImage
import Darwin
import simd
import os.signpost
#if SOJU_OPENXR
func xrDraw(_ context:UnsafeMutableRawPointer?,_ texture:UnsafeMutableRawPointer?,_ queue:UnsafeMutableRawPointer?,_ packet:UnsafePointer<SojuXREye>?) {
    guard let context=context,let texture=texture,let queue=queue,let packet=packet else { return }
    let c=Unmanaged<Commander>.fromOpaque(context).takeUnretainedValue()
    c.renderEye(Unmanaged<AnyObject>.fromOpaque(texture).takeUnretainedValue() as! MTLTexture,
        Unmanaged<AnyObject>.fromOpaque(queue).takeUnretainedValue() as! MTLCommandQueue,packet.pointee)
}
func xrControls(_ context:UnsafeMutableRawPointer?,_ packet:UnsafePointer<SojuXRInput>?) {
    guard let context=context,let packet=packet else { return }
    Unmanaged<Commander>.fromOpaque(context).takeUnretainedValue().controls(packet.pointee)
}
extension Commander {
    func pumpXR() {
        precondition(!useMetalBatches,"SceneKit pump requires explicit fallback renderer")
        guard xrPumpActive else {return}
        os_signpost(.begin,log:XRFrameMetrics.log,name:"Frame loop")
        defer {os_signpost(.end,log:XRFrameMetrics.log,name:"Frame loop")}
        frameMetrics.beginFrameAllocationSample()
        let result=soju_xr_step(Unmanaged.passUnretained(self).toOpaque(),xrDraw,xrControls)
        frameMetrics.endFrameAllocationSample()
        var timing=SojuXRFrameTiming()
        if result==1,soju_xr_frame_timing(&timing) != 0 {
            frameMetrics.lifecycle(wait:timing.waitMs,acquire:timing.acquireMs,end:timing.endMs,afterWait:timing.afterWaitMs,period:timing.periodMs)
        }
        guard result != 0 else {xrPumpActive=false;notice=String(cString:soju_xr_status());soju_xr_close();return}
        // xrWaitFrame sets the deadline. Yield to input/state work between frames,
        // without imposing a second periodic timer on an active session.
        if result==2 {DispatchQueue.main.asyncAfter(deadline:.now()+0.01) { [weak self] in self?.pumpXR() }}
        else {DispatchQueue.main.async { [weak self] in self?.pumpXR() }}
    }

    func renderEye(_ texture:MTLTexture,_ queue:MTLCommandQueue,_ eye:SojuXREye) {
        precondition(!useMetalBatches,"SceneKit eye callback requires explicit fallback renderer")
        if eye.eye==0,xrDrawCount==120,let path=argument("--metal-capture") {
            let capture=MTLCaptureDescriptor();capture.captureObject=queue;capture.destination = .gpuTraceDocument
            capture.outputURL=URL(fileURLWithPath:path)
            do {try MTLCaptureManager.shared().startCapture(with:capture)}
            catch {fputs("Metal capture: \(error)\n",stderr)}
        }
        guard let renderer=xrRenderer,let buffer=queue.makeCommandBuffer() else { return }
        buffer.label=eye.eye==0 ? "Brood War left eye":"Brood War right eye"
        let eyeStart=CACurrentMediaTime()
        os_signpost(.begin,log:XRFrameMetrics.log,name:"Eye encode")
        defer {os_signpost(.end,log:XRFrameMetrics.log,name:"Eye encode")}
        if eye.eye==0 { xrFrameTime=CACurrentMediaTime();frameMetrics.begin(units:nodes.count) }
        if xrOrigin==nil && eye.tracked != 0 {
            xrOrigin=SIMD3<Float>(eye.headX,eye.headY,eye.headZ)
            let forward=simd_quatf(ix:eye.qx,iy:eye.qy,iz:eye.qz,r:eye.qw).act(SIMD3<Float>(0,0,-1))
            xrOriginYaw=atan2(-forward.x,-forward.z)
        }
        let body=xrBody
        let base=SIMD3<Float>(Float(camera.position.x),Float(camera.position.y),Float(camera.position.z))
        let position=base+body.act((SIMD3<Float>(eye.px,eye.py,eye.pz)-(xrOrigin ?? .zero))*worldUnitsPerMeter)
        if eye.eye==0 {
            hud.isHidden = !devOverlay
            let center=base+body.act((SIMD3<Float>(eye.headX,eye.headY,eye.headZ)-(xrOrigin ?? .zero))*worldUnitsPerMeter)
            let headRotation=body*simd_quatf(ix:eye.qx,iy:eye.qy,iz:eye.qz,r:eye.qw)
            let updateStart=CACurrentMediaTime()
            os_signpost(.begin,log:XRFrameMetrics.log,name:"Billboard update")
            for billboard in billboards.values {billboard.updateView(eye:center,orientation:headRotation,perspective:true,art:worldSprites,recordEvidence:captureFrames.folder != nil || xrDrawCount%90==0)}
            updatePossessionVisibility(eye:center)
            os_signpost(.end,log:XRFrameMetrics.log,name:"Billboard update")
            frameMetrics.stage("billboardUpdate",milliseconds:(CACurrentMediaTime()-updateStart)*1000)
            hud.simdPosition=center+headRotation.act(SIMD3<Float>(0,0.45,-1.5)*worldUnitsPerMeter);hud.simdOrientation=headRotation;hud.simdScale=SIMD3<Float>(repeating:worldUnitsPerMeter)
            if devOverlay {
            let text="Selected: \(selected.count)   Minerals: \(Int(number(state,"minerals")))   Action: \(actionNames[actionMode])\nLeft trigger: select / drag   Right trigger: command   Left grip: scale\n\(notice)"
            if text != hudText {
                hudText=text
                let size=NSSize(width:1200,height:180),image=NSImage(size:NSSize(width:1200,height:180))
                image.lockFocus();NSColor.black.withAlphaComponent(0.8).setFill();NSRect(origin:.zero,size:size).fill()
                (text as NSString).draw(in:NSRect(x:20,y:12,width:1160,height:158),withAttributes:[.font:NSFont.monospacedSystemFont(ofSize:30,weight:.medium),.foregroundColor:NSColor.white])
                image.unlockFocus();hud.geometry?.firstMaterial?.diffuse.contents=image
            }
            }
        }
        eyePositions[Int(eye.eye)]=position
        if eye.eye==1 { headCenter=(eyePositions[0]+eyePositions[1])*0.5 }
        SCNTransaction.begin();SCNTransaction.disableActions=true
        xrCamera.simdPosition=position
        xrCamera.simdOrientation=body*simd_quatf(ix:eye.qx,iy:eye.qy,iz:eye.qz,r:eye.qw)
        let l=CGFloat(tan(eye.left)),r=CGFloat(tan(eye.right)),u=CGFloat(tan(eye.up)),d=CGFloat(tan(eye.down))
        let near=CGFloat(0.03)*CGFloat(worldUnitsPerMeter),far=CGFloat(150)*CGFloat(worldUnitsPerMeter)
        xrCamera.camera!.zNear=Double(near);xrCamera.camera!.zFar=Double(far)
        var projection=SCNMatrix4()
        projection.m11=2/(r-l);projection.m22=2/(u-d)
        projection.m31=(r+l)/(r-l);projection.m32=(u+d)/(u-d)
        projection.m33 = -(far+near)/(far-near);projection.m34 = -1
        projection.m43 = -2*far*near/(far-near);projection.m44=0
        xrCamera.camera!.projectionTransform=projection
        SCNTransaction.commit();SCNTransaction.flush()
        // SceneKit shades in linear light; the runtime consumes display-encoded bytes.
        // An sRGB view also covers runtimes advertising only BGRA8Unorm.
        let output:MTLTexture
        if texture.pixelFormat == .bgra8Unorm {
            let key=ObjectIdentifier(texture)
            if let cached=xrOutputViews[key] {output=cached}
            else {
                guard let view=texture.makeTextureView(pixelFormat:.bgra8Unorm_srgb) else {fputs("XR sRGB view unavailable\n",stderr);return}
                xrOutputViews[key]=view;output=view
            }
        } else {output=texture}
        let index=Int(eye.eye)
        if !useMetalBatches,(xrDepth[index]?.width != texture.width || xrDepth[index]?.height != texture.height) {
            let desc=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.depth32Float_stencil8,width:texture.width,height:texture.height,mipmapped:false)
            desc.storageMode = .private;desc.usage = .renderTarget
            desc.textureType = .type2DMultisample;desc.sampleCount=4
            xrDepth[index]=TextureResidency.shared.make(art.device,desc,owner:"render-targets")
            desc.pixelFormat=output.pixelFormat;xrColor[index]=TextureResidency.shared.make(art.device,desc,owner:"render-targets")
        }
        let pass=xrScenePass;pass.colorAttachments[0].texture=xrColor[index]
        pass.colorAttachments[0].resolveTexture=output
        pass.colorAttachments[0].loadAction = .clear;pass.colorAttachments[0].storeAction = .multisampleResolve
        pass.colorAttachments[0].clearColor=MTLClearColorMake(0.025,0.035,0.05,1)
        pass.depthAttachment.texture=xrDepth[index];pass.depthAttachment.loadAction = .clear
        pass.depthAttachment.storeAction = .dontCare;pass.depthAttachment.clearDepth=SceneKitDepth.clear
        pass.stencilAttachment.texture=xrDepth[index];pass.stencilAttachment.loadAction = .clear
        pass.stencilAttachment.storeAction = .dontCare;pass.stencilAttachment.clearStencil=0
        if eye.eye==0 { xrDrawCount+=1 }
        if eye.eye==0 && xrDrawCount%72==0 {
            let diagnostic:Obj=["eye":[eye.px,eye.py,eye.pz],"desired":[position.x,position.y,position.z],
                "presentation":[xrCamera.presentation.simdWorldPosition.x,xrCamera.presentation.simdWorldPosition.y,xrCamera.presentation.simdWorldPosition.z],
                "desktop":[camera.simdWorldPosition.x,camera.simdWorldPosition.y,camera.simdWorldPosition.z],
                "fov":[eye.left,eye.right,eye.up,eye.down],"unitCount":nodes.count,"terrainCount":terrain.count,
                "swapchainFormat":texture.pixelFormat.rawValue,"renderFormat":output.pixelFormat.rawValue,
                "exposureOffset":xrCamera.camera!.exposureOffset,"hdr":xrCamera.camera!.wantsHDR,
                "iblIntensity":world.lightingEnvironment.intensity,"tracked":eye.tracked,"selected":selected,"frame":number(state,"frame"),"yaw":yaw,
                "batchDraws":metalBatches?.draws ?? 0,"batchVisible":metalBatches?.visible ?? 0,"batchCulled":metalBatches?.culled ?? 0,"batchLOD":metalBatches?.lodCount ?? 0,"batchOcclusionSkipped":metalBatches?.occlusionSkipped ?? 0,
                "worldUnitsPerMeter":worldUnitsPerMeter,"wristCardClicks":wrists.cardClicks,"boxSelections":wrists.boxSelections,"doubleTaps":wrists.doubleTaps,"grabs":wrists.grabs,
                "orientation":[xrCamera.simdOrientation.imag.x,xrCamera.simdOrientation.imag.y,xrCamera.simdOrientation.imag.z,xrCamera.simdOrientation.real]]
            record("xr-view-diagnostics.json",diagnostic)
        }
        let renderStart=CACurrentMediaTime()
        frameMetrics.stage("eyeSetup",milliseconds:(renderStart-eyeStart)*1000)
        os_signpost(.begin,log:XRFrameMetrics.log,name:"SceneKit render")
        if useMetalBatches {
            do {
                if metalBatches==nil {metalBatches=try MetalBatches(device:art.device)}
                if eye.eye==0 {metalBatches!.prepareAsync(self)}
                if eye.eye==0 {
                    var right=SojuXREye();_ = soju_xr_eye(1,&right)
                    let rightPosition=base+body.act((SIMD3<Float>(right.px,right.py,right.pz)-(xrOrigin ?? .zero))*worldUnitsPerMeter)
                    var rightTransform=simd_float4x4(body*simd_quatf(ix:right.qx,iy:right.qy,iz:right.qz,r:right.qw))
                    rightTransform.columns.3=SIMD4<Float>(rightPosition,1)
                    let rl=Float(tan(right.left)),rr=Float(tan(right.right)),ru=Float(tan(right.up)),rd=Float(tan(right.down))
                    var rightProjection=simd_float4x4(projection)
                    rightProjection.columns.0.x=2/(rr-rl);rightProjection.columns.1.y=2/(ru-rd)
                    rightProjection.columns.2.x=(rr+rl)/(rr-rl);rightProjection.columns.2.y=(ru+rd)/(ru-rd)
                    try metalBatches!.render(command:buffer,output:output,camera:xrCamera,eye:position,rightTransform:rightTransform,rightProjection:rightProjection)
                }
                metalBatches!.copyEye(command:buffer,to:output,eye:Int(eye.eye))
                try metalBatches!.renderOverlay(renderer:renderer,command:buffer,output:output,camera:xrCamera,eye:Int(eye.eye),time:xrFrameTime)
            } catch {buffer.commit();fputs("Metal batches: \(error)\n",stderr);NSApp.terminate(nil);return}
        } else {
            renderer.render(atTime:xrFrameTime,viewport:CGRect(x:0,y:0,width:texture.width,height:texture.height),commandBuffer:buffer,passDescriptor:pass)
        }
        os_signpost(.end,log:XRFrameMetrics.log,name:"SceneKit render")
        frameMetrics.stage("renderEncodeEye",milliseconds:(CACurrentMediaTime()-renderStart)*1000)
        let postStart=CACurrentMediaTime()
        if eye.eye==0 {xrStereoCaptureIndex=initialized ? captureFrames.reserve():nil}
        let captureIndex=(eye.eye==0 || CommandLine.arguments.contains("--record-stereo")) ? xrStereoCaptureIndex:nil
        let captureSuffix=eye.eye==0 ? "":"-right"
        let captureMetadata:Obj=captureIndex != nil ? ["nativeFrame":number(state,"frame"),"time":Date().timeIntervalSince1970,"xrFrame":xrDrawCount,"eye":Int(eye.eye),
            "worldUnitsPerMeter":worldUnitsPerMeter,"selected":selected,"combatEvents":effects.counts,"deathFrames":worldSprites.deathFrames,
            "loneFrames":worldSprites.loneFrames,"creepTiles":creepNodes.count,"source":"OpenXR Metal swapchain eye", "snapshotSource":argument("--snapshot") ?? "live",
            "camera":[position.x,position.y,position.z],"orientation":[xrCamera.simdOrientation.imag.x,xrCamera.simdOrientation.imag.y,xrCamera.simdOrientation.imag.z,xrCamera.simdOrientation.real],
            "billboards":billboards.mapValues{["position":[$0.root.position.x,$0.root.position.y,$0.root.position.z],"layers":$0.evidence] as Obj}]:[:]
        let mirrorDue=eye.eye==0 && xrFrameTime-lastMirrorTime>=0.5
        if (mirrorDue || captureIndex != nil),let context=mirrorContext {
            if eye.eye==0 {lastMirrorTime=xrFrameTime}
            let desc=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:texture.pixelFormat,width:texture.width,height:texture.height,mipmapped:false)
            desc.usage = .shaderRead;desc.storageMode = .private
            if let copy=TextureResidency.shared.make(art.device,desc,owner:"captures"),let blit=buffer.makeBlitCommandEncoder() {
                blit.copy(from:texture,to:copy);blit.endEncoding()
                buffer.addCompletedHandler { [weak self] command in
                    guard command.status == .completed,let ci=CIImage(mtlTexture:copy,options:nil),let cg=context.createCGImage(ci.transformed(by:CGAffineTransform(translationX:0,y:CGFloat(copy.height)).scaledBy(x:1,y:-1)),from:ci.extent) else { return }
                    if eye.eye==0 {DispatchQueue.main.async { self?.mirror?.image=NSImage(cgImage:cg,size:NSSize(width:copy.width,height:copy.height)) }}
                    if let index=captureIndex {self?.captureFrames.save(cg,index:index,metadata:captureMetadata,suffix:captureSuffix)}
                }
            }
        }
        buffer.addCompletedHandler { command in if let error=command.error { fputs("XR GPU: \(error)\n",stderr) } }
        let measuredFrame=xrDrawCount,measuredEye=Int(eye.eye)
        buffer.addCompletedHandler { [weak self] command in
            self?.frameMetrics.completed(frame:measuredFrame,eye:measuredEye,milliseconds:(command.gpuEndTime-command.gpuStartTime)*1000)
        }
        frameMetrics.stage("eyePostRender",milliseconds:(CACurrentMediaTime()-postStart)*1000)
        buffer.commit()
        if eye.eye==1 {
            frameMetrics.submitted()
            if xrDrawCount==121,MTLCaptureManager.shared().isCapturing {MTLCaptureManager.shared().stopCapture()}
        }
    }
}
#endif
