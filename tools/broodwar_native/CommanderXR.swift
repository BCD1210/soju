import Cocoa
import SceneKit
import Metal
import QuartzCore
import CoreImage
import Darwin
import simd
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
    func renderEye(_ texture:MTLTexture,_ queue:MTLCommandQueue,_ eye:SojuXREye) {
        guard let renderer=xrRenderer,let buffer=queue.makeCommandBuffer() else { return }
        if eye.eye==0 { xrFrameTime=CACurrentMediaTime() }
        if xrOrigin==nil && eye.tracked != 0 {
            xrOrigin=SIMD3<Float>(eye.headX,eye.headY,eye.headZ)
            let forward=simd_quatf(ix:eye.qx,iy:eye.qy,iz:eye.qz,r:eye.qw).act(SIMD3<Float>(0,0,-1))
            xrOriginYaw=atan2(-forward.x,-forward.z)
        }
        let body=xrBody
        let base=SIMD3<Float>(Float(camera.position.x),Float(camera.position.y),Float(camera.position.z))
        let position=base+body.act((SIMD3<Float>(eye.px,eye.py,eye.pz)-(xrOrigin ?? .zero))*worldUnitsPerMeter)
        if eye.eye==0 {
            let center=base+body.act((SIMD3<Float>(eye.headX,eye.headY,eye.headZ)-(xrOrigin ?? .zero))*worldUnitsPerMeter)
            let headRotation=body*simd_quatf(ix:eye.qx,iy:eye.qy,iz:eye.qz,r:eye.qw)
            hud.simdPosition=center+headRotation.act(SIMD3<Float>(0,-0.54,-1.1)*worldUnitsPerMeter);hud.simdOrientation=headRotation;hud.simdScale=SIMD3<Float>(repeating:worldUnitsPerMeter)
            let text="Selected: \(selected.count)   Minerals: \(Int(number(state,"minerals")))   Action: \(actionNames[actionMode])\nLeft trigger: select / drag   Right trigger: command   Left grip: scale\n\(notice)"
            if text != hudText {
                hudText=text
                let size=NSSize(width:1200,height:180),image=NSImage(size:NSSize(width:1200,height:180))
                image.lockFocus();NSColor.black.withAlphaComponent(0.8).setFill();NSRect(origin:.zero,size:size).fill()
                (text as NSString).draw(in:NSRect(x:20,y:12,width:1160,height:158),withAttributes:[.font:NSFont.monospacedSystemFont(ofSize:30,weight:.medium),.foregroundColor:NSColor.white])
                image.unlockFocus();hud.geometry?.firstMaterial?.diffuse.contents=image
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
        guard let output = texture.pixelFormat == .bgra8Unorm ? texture.makeTextureView(pixelFormat:.bgra8Unorm_srgb) : texture else {
            fputs("XR sRGB render view unavailable\n",stderr);return
        }
        let index=Int(eye.eye)
        if xrDepth[index]?.width != texture.width || xrDepth[index]?.height != texture.height {
            let desc=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.depth32Float_stencil8,width:texture.width,height:texture.height,mipmapped:false)
            desc.storageMode = .private;desc.usage = .renderTarget
            desc.textureType = .type2DMultisample;desc.sampleCount=4
            xrDepth[index]=art.device.makeTexture(descriptor:desc)
            desc.pixelFormat=output.pixelFormat;xrColor[index]=art.device.makeTexture(descriptor:desc)
        }
        let pass=MTLRenderPassDescriptor();pass.colorAttachments[0].texture=xrColor[index]
        pass.colorAttachments[0].resolveTexture=output
        pass.colorAttachments[0].loadAction = .clear;pass.colorAttachments[0].storeAction = .multisampleResolve
        pass.colorAttachments[0].clearColor=MTLClearColorMake(0.025,0.035,0.05,1)
        pass.depthAttachment.texture=xrDepth[index];pass.depthAttachment.loadAction = .clear
        pass.depthAttachment.storeAction = .dontCare;pass.depthAttachment.clearDepth=1
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
                "worldUnitsPerMeter":worldUnitsPerMeter,"wristCardClicks":wrists.cardClicks,"boxSelections":wrists.boxSelections,"doubleTaps":wrists.doubleTaps,"grabs":wrists.grabs,
                "orientation":[xrCamera.simdOrientation.imag.x,xrCamera.simdOrientation.imag.y,xrCamera.simdOrientation.imag.z,xrCamera.simdOrientation.real]]
            if let data=try? JSONSerialization.data(withJSONObject:diagnostic,options:.prettyPrinted) { try? data.write(to:runtime.appendingPathComponent("xr-view-diagnostics.json"),options:.atomic) }
        }
        renderer.render(atTime:xrFrameTime,viewport:CGRect(x:0,y:0,width:texture.width,height:texture.height),commandBuffer:buffer,passDescriptor:pass)
        let captureIndex=eye.eye==0 && initialized ? captureFrames.reserve():nil
        let captureMetadata:Obj=["nativeFrame":number(state,"frame"),"time":Date().timeIntervalSince1970,"xrFrame":xrDrawCount,"eye":0,
            "worldUnitsPerMeter":worldUnitsPerMeter,"selected":selected,"combatEvents":effects.counts,"deathFrames":worldSprites.deathFrames,
            "loneFrames":worldSprites.loneFrames,"creepTiles":creepNodes.count,"source":"OpenXR Metal swapchain eye; original live simulation"]
        if eye.eye==0 && (xrDrawCount%10==0 || captureIndex != nil),let context=mirrorContext {
            let desc=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:texture.pixelFormat,width:texture.width,height:texture.height,mipmapped:false)
            desc.usage = .shaderRead;desc.storageMode = .private
            if let copy=art.device.makeTexture(descriptor:desc),let blit=buffer.makeBlitCommandEncoder() {
                blit.copy(from:texture,to:copy);blit.endEncoding()
                buffer.addCompletedHandler { [weak self] command in
                    guard command.status == .completed,let ci=CIImage(mtlTexture:copy,options:nil),let cg=context.createCGImage(ci.transformed(by:CGAffineTransform(translationX:0,y:CGFloat(copy.height)).scaledBy(x:1,y:-1)),from:ci.extent) else { return }
                    DispatchQueue.main.async { self?.mirror?.image=NSImage(cgImage:cg,size:NSSize(width:copy.width,height:copy.height)) }
                    if let index=captureIndex {self?.captureFrames.save(cg,index:index,metadata:captureMetadata)}
                }
            }
        }
        buffer.addCompletedHandler { command in if let error=command.error { fputs("XR GPU: \(error)\n",stderr) } }
        buffer.commit()
    }
}
#endif
