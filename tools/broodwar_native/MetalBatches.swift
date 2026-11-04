import Cocoa
import SceneKit
import Metal
import MetalKit
import simd

// Shared desktop/stereo terrain and sprite batches; other geometry uses a depth-preserving overlay.
final class MetalBatches {
    struct Vertex {var position:SIMD4<Float>;var uv:SIMD4<Float>}
    struct Instance {var model:simd_float4x4;var parameters:SIMD4<Float>}
    struct Uniforms {var left:simd_float4x4;var right:simd_float4x4;var leftEye:SIMD4<Float>;var rightEye:SIMD4<Float>}
    struct Batch {
        var vertices:MTLBuffer;var indices:MTLBuffer;var count:Int;var flatVertices:MTLBuffer?
        var instances:[MTLBuffer];var nodes:[SCNNode];var textures:[MTLTexture]
        var terrain:Bool;var shadow:Bool;var mode:Float;var depthDisabled:Bool;var bounds:[(SIMD3<Float>,Float)]
    }
    let device:MTLDevice,loader:MTKTextureLoader,queue:MTLCommandQueue
    let pipelines:[MTLRenderPipelineState]
    let compositePipeline:MTLRenderPipelineState
    var overlayColor:MTLTexture?,overlayResolved:MTLTexture?
    private var depthViews=[MTLTexture]()
    private let overlayPass=MTLRenderPassDescriptor(),compositePass=MTLRenderPassDescriptor()
    let samples:Int
    let depth:MTLDepthStencilState,shadowDepth:MTLDepthStencilState,overlayDepth:MTLDepthStencilState
    var batches=[Batch]()
    let textures=ResidentCache<(NSImage,MTLTexture)>(limit:512*1024*1024)
    var ready=false,draws=0,visible=0,culled=0,lodCount=0
    var sourceRevision = -1,terrainRevision = -1
    var revisions=[(UnitBillboard,Int)]()
    lazy var flatIndices=buffer([UInt32(0),1,2,2,1,3])
    var stereoColor:MTLTexture?,stereoDepth:MTLTexture?,stereoResolved:MTLTexture?
    var stereoPass=MTLRenderPassDescriptor()
    let white:MTLTexture,black:MTLTexture,normal:MTLTexture
    private let available=DispatchSemaphore(value:3)
    private var ring=0,frame=0,visibilityEpoch=0
    private let visibilityLock=NSLock()
    private var visibilityBuffers=[MTLBuffer](),visibleLast=[Bool](),visibleNow=[Bool]()
    private var lastVisibilityFrame = -1
    private var visibleVP=matrix_identity_float4x4,visibleRightVP=matrix_identity_float4x4
    private(set) var occlusionSkipped=0
    private var planes=[SIMD4<Float>](repeating:.zero,count:6)
    init(device:MTLDevice)throws {
        guard device.supportsVertexAmplificationCount(2) else {throw SceneError.invalid("Two-view vertex amplification unavailable")}
        let samples=xrRequested ? 1:4;self.samples=samples
        self.device=device;loader=MTKTextureLoader(device:device);queue=device.makeCommandQueue()!
        func solid(_ rgba:[UInt8])->MTLTexture {
            let d=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.rgba8Unorm,width:1,height:1,mipmapped:false)
            let t=device.makeTexture(descriptor:d)!
            rgba.withUnsafeBytes{t.replace(region:MTLRegionMake2D(0,0,1,1),mipmapLevel:0,withBytes:$0.baseAddress!,bytesPerRow:4)}
            return t
        }
        white=solid([255,255,255,255]);black=solid([0,0,0,255]);normal=solid([128,128,255,255])
        let library=try device.makeLibrary(source:Self.shader,options:nil)
        func makePipeline(_ mode:UInt32)throws->MTLRenderPipelineState {
            let shadow=mode==0 || mode==3
            let d=MTLRenderPipelineDescriptor();d.label=shadow ? "Batched ground shadows":"Batched terrain and relief"
            d.inputPrimitiveTopology = .triangle
            d.vertexFunction=library.makeFunction(name:"batchVertex");let constants=MTLFunctionConstantValues();var value=mode
            constants.setConstantValue(&value,type:.uint,index:0)
            d.fragmentFunction=try library.makeFunction(name:"batchFragment",constantValues:constants)
            d.colorAttachments[0].pixelFormat = .bgra8Unorm_srgb;d.depthAttachmentPixelFormat = .depth32Float_stencil8;d.stencilAttachmentPixelFormat = .depth32Float_stencil8
            d.rasterSampleCount=samples;d.maxVertexAmplificationCount=2
            d.colorAttachments[0].isBlendingEnabled=true
            d.colorAttachments[0].sourceRGBBlendFactor = .sourceAlpha;d.colorAttachments[0].destinationRGBBlendFactor = .oneMinusSourceAlpha
            d.colorAttachments[0].sourceAlphaBlendFactor = .one;d.colorAttachments[0].destinationAlphaBlendFactor = .oneMinusSourceAlpha
            return try device.makeRenderPipelineState(descriptor:d)
        }
        pipelines=try (UInt32(0)..<4).map{try makePipeline($0)}
        let composite=MTLRenderPipelineDescriptor()
        composite.vertexFunction=library.makeFunction(name:"compositeVertex")
        composite.fragmentFunction=library.makeFunction(name:"compositeFragment")
        composite.colorAttachments[0].pixelFormat = .bgra8Unorm_srgb
        composite.colorAttachments[0].isBlendingEnabled=true
        composite.colorAttachments[0].sourceRGBBlendFactor = .one
        composite.colorAttachments[0].destinationRGBBlendFactor = .oneMinusSourceAlpha
        composite.colorAttachments[0].sourceAlphaBlendFactor = .one
        composite.colorAttachments[0].destinationAlphaBlendFactor = .oneMinusSourceAlpha
        compositePipeline=try device.makeRenderPipelineState(descriptor:composite)
        let d=MTLDepthStencilDescriptor();d.depthCompareFunction = .lessEqual;d.isDepthWriteEnabled=true;depth=device.makeDepthStencilState(descriptor:d)!
        d.isDepthWriteEnabled=false;shadowDepth=device.makeDepthStencilState(descriptor:d)!
        d.depthCompareFunction = .always;overlayDepth=device.makeDepthStencilState(descriptor:d)!
    }
    static func colorImage(_ image:CGImage)->CGImage {
        guard let space=CGColorSpace(name:CGColorSpace.sRGB),
            let context=CGContext(data:nil,width:image.width,height:image.height,bitsPerComponent:8,bytesPerRow:image.width*4,space:space,bitmapInfo:CGImageAlphaInfo.premultipliedLast.rawValue) else {return image}
        context.draw(image,in:CGRect(x:0,y:0,width:image.width,height:image.height))
        return context.makeImage() ?? image
    }
    static func textureBytes(_ texture:MTLTexture)->Int {
        max(texture.allocatedSize,texture.parent?.allocatedSize ?? 0,texture.width*texture.height*4)
    }
    static func colorView(_ texture:MTLTexture)->MTLTexture {
        let format:MTLPixelFormat
        switch texture.pixelFormat {
        case .rgba8Unorm:format = .rgba8Unorm_srgb
        case .bgra8Unorm:format = .bgra8Unorm_srgb
        default:return texture
        }
        return texture.makeTextureView(pixelFormat:format) ?? texture
    }
    func texture(_ value:Any?,fallback:MTLTexture,topLeft:Bool=true)throws->MTLTexture {
        if let t=value as? MTLTexture {return t}
        guard let image=value as? NSImage else {return fallback}
        let key=String(describing:ObjectIdentifier(image))+"/\(topLeft)"
        if let cached=textures.get(key) {return cached.1}
        guard let cg=image.cgImage(forProposedRect:nil,context:nil,hints:nil) else {return fallback}
        let t=Self.colorView(try loader.newTexture(cgImage:Self.colorImage(cg),options:[.SRGB:true,.generateMipmaps:true,.origin:topLeft ? MTKTextureLoader.Origin.topLeft:MTKTextureLoader.Origin.bottomLeft]))
        textures.insert((image,t),key:key,bytes:Self.textureBytes(t));return t
    }
    func components(_ source:SCNGeometrySource,_ i:Int)->[Float] {
        source.data.withUnsafeBytes { bytes in
            (0..<source.componentsPerVector).map { c in
                let offset=source.dataOffset+i*source.dataStride+c*source.bytesPerComponent
                return source.bytesPerComponent==8 ? Float(bytes.loadUnaligned(fromByteOffset:offset,as:Double.self)):bytes.loadUnaligned(fromByteOffset:offset,as:Float.self)
            }
        }
    }
    func mesh(_ geometry:SCNGeometry,transform:simd_float4x4=matrix_identity_float4x4,slice:Float=0,uvTransform:SCNMatrix4=SCNMatrix4Identity)->([Vertex],[UInt32]) {
        guard let positions=geometry.sources(for:.vertex).first,let coords=geometry.sources(for:.texcoord).first else {return ([],[])}
        var vertices=[Vertex](),indices=[UInt32]()
        vertices.reserveCapacity(positions.vectorCount)
        for i in 0..<positions.vectorCount {
            let p=components(positions,i),uv=components(coords,i)
            let u=uv[0]*Float(uvTransform.m11)+Float(uvTransform.m41),v=uv[1]*Float(uvTransform.m22)+Float(uvTransform.m42)
            vertices.append(Vertex(position:transform*SIMD4<Float>(p[0],p[1],p[2],1),uv:SIMD4<Float>(u,v,slice,0)))
        }
        for element in geometry.elements where element.primitiveType == .triangles {
            element.data.withUnsafeBytes { bytes in
                for i in 0..<element.primitiveCount*3 {
                    let offset=i*element.bytesPerIndex
                    let index:UInt32=element.bytesPerIndex==4 ? bytes.loadUnaligned(fromByteOffset:offset,as:UInt32.self):element.bytesPerIndex==2 ? UInt32(bytes.loadUnaligned(fromByteOffset:offset,as:UInt16.self)):UInt32(bytes[offset])
                    indices.append(index)
                }
            }
        }
        return (vertices,indices)
    }
    func buffer<T>(_ values:[T])->MTLBuffer {values.withUnsafeBytes{device.makeBuffer(bytes:$0.baseAddress!,length:$0.count,options:.storageModeShared)!}}
    func prepare(_ commander:Commander)throws {
        if ready,sourceRevision==commander.sceneRevision,revisions.count==commander.billboards.count,revisions.allSatisfy({$0.0.geometryRevision==$0.1}) {return}
        if terrainRevision != commander.terrainRevision {ready=false;batches.removeAll()}
        else if ready {batches.removeAll{!$0.terrain}}
        sourceRevision=commander.sceneRevision;terrainRevision=commander.terrainRevision
        revisions=commander.billboards.values.map{($0,$0.geometryRevision)}
        // Array pages retain each tile's own gutter and mip chain. No cross-tile
        // filtering can bleed unrelated tiles into one another.
        let tiles=commander.terrain.sorted{$0.key<$1.key}.map{$0.value}
        if !ready {for start in stride(from:0,to:tiles.count,by:128) {
            let nodes=Array(tiles[start..<min(start+128,tiles.count)])
            guard let first=nodes.first?.geometry?.firstMaterial?.diffuse.contents as? MTLTexture else {continue}
            let d=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:first.pixelFormat,width:first.width,height:first.height,mipmapped:true)
            d.textureType = .type2DArray;d.arrayLength=nodes.count;d.storageMode = .private;d.usage = .shaderRead
            let array=device.makeTexture(descriptor:d)!
            let firstMask=try texture(nodes[0].geometry?.firstMaterial?.multiply.contents,fallback:white)
            let fd=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.rgba8Unorm_srgb,width:firstMask.width,height:firstMask.height,mipmapped:false)
            fd.textureType = .type2DArray;fd.arrayLength=nodes.count;fd.storageMode = .private;fd.usage = .shaderRead
            let fog=device.makeTexture(descriptor:fd)!
            let command=queue.makeCommandBuffer()!,blit=command.makeBlitCommandEncoder()!
            var vertices=[Vertex](),indices=[UInt32]()
            for (slice,node) in nodes.enumerated() {
                guard let geometry=node.geometry,let material=geometry.firstMaterial,let source=material.diffuse.contents as? MTLTexture else {continue}
                for level in 0..<source.mipmapLevelCount {
                    blit.copy(from:source,sourceSlice:0,sourceLevel:level,sourceOrigin:MTLOrigin(),sourceSize:MTLSize(width:max(1,source.width>>level),height:max(1,source.height>>level),depth:1),to:array,destinationSlice:slice,destinationLevel:level,destinationOrigin:MTLOrigin())
                }
                let mask=try texture(material.multiply.contents,fallback:white)
                if mask.width==fog.width && mask.height==fog.height {blit.copy(from:mask,sourceSlice:0,sourceLevel:0,sourceOrigin:MTLOrigin(),sourceSize:MTLSize(width:mask.width,height:mask.height,depth:1),to:fog,destinationSlice:slice,destinationLevel:0,destinationOrigin:MTLOrigin())}
                node.categoryBitMask=4
                let (v,ind)=mesh(geometry,transform:node.simdWorldTransform,slice:Float(slice),uvTransform:material.diffuse.contentsTransform)
                let base=UInt32(vertices.count);vertices+=v;indices+=ind.map{$0+base}
            }
            blit.endEncoding();command.commit();command.waitUntilCompleted()
            let instance=Instance(model:matrix_identity_float4x4,parameters:SIMD4<Float>(2,1,Float(first.width),Float(first.height)))
            batches.append(Batch(vertices:buffer(vertices),indices:buffer(indices),count:indices.count,flatVertices:nil,instances:(0..<3).map{_ in buffer([instance])},nodes:[],textures:[array,fog],terrain:true,shadow:false,mode:2,depthDisabled:false,bounds:[]))
        }
        }
        var groups=[String:[SCNNode]]()
        for billboard in commander.billboards.values where !billboard.root.isHidden {
            for (index,native) in billboard.nativeLayers {
                guard let node=billboard.layers[index],let frame=billboard.shown[index] else {continue}
                node.categoryBitMask=4
                let shadow=Int(number(native,"drawfunc"))==10
                let key="\(billboard.shownImages[index] ?? Int(number(native,"id")))/\(frame)/\(billboard.shownColors[index] ?? billboard.team)/\(shadow)"
                groups[key,default:[]].append(node)
            }
        }
        for key in groups.keys.sorted() {
            let nodes=groups[key]!,node=nodes[0]
            guard let geometry=node.geometry,let material=geometry.firstMaterial else {continue}
            let shadow=key.hasSuffix("/true"),relief = !(geometry is SCNPlane)
            let (vertices,indices)=mesh(geometry)
            if vertices.isEmpty || indices.isEmpty {continue}
            var maps=[try texture(material.diffuse.contents,fallback:white,topLeft:true)]
            for name in ["normal","bright","specular","emissive","ao_depth"] {
                maps.append(try texture(relief ? (material.value(forKey:"relief_"+name) as? SCNMaterialProperty)?.contents:nil,fallback:name=="normal" ? normal:name=="ao_depth" ? white:black,topLeft:true))
            }
            var flat:MTLBuffer?
            if relief {
                let xs=vertices.map{$0.position.x},ys=vertices.map{$0.position.y},zs=vertices.map{$0.position.z}
                let us=vertices.map{$0.uv.x},vs=vertices.map{$0.uv.y}
                let x0=xs.min()!,x1=xs.max()!,y0=ys.min()!,y1=ys.max()!,z=zs.min()!
                let u0=us.min()!,u1=us.max()!,v0=vs.min()!,v1=vs.max()!
                flat=buffer([Vertex(position:SIMD4<Float>(x0,y0,z,1),uv:SIMD4<Float>(u0,v1,0,0)),Vertex(position:SIMD4<Float>(x1,y0,z,1),uv:SIMD4<Float>(u1,v1,0,0)),Vertex(position:SIMD4<Float>(x0,y1,z,1),uv:SIMD4<Float>(u0,v0,0,0)),Vertex(position:SIMD4<Float>(x1,y1,z,1),uv:SIMD4<Float>(u1,v0,0,0))])
            }
            let instances=nodes.map {Instance(model:$0.simdWorldTransform,parameters:SIMD4<Float>(shadow ? 3:relief ? 1:0,Float($0.opacity),0,0))}
            let bounds=nodes.map { n -> (SIMD3<Float>,Float) in
                let (lo,hi)=n.boundingBox
                let center=(SIMD3<Float>(Float(lo.x),Float(lo.y),Float(lo.z))+SIMD3<Float>(Float(hi.x),Float(hi.y),Float(hi.z)))*0.5
                let radius=simd_length(SIMD3<Float>(Float(hi.x-lo.x),Float(hi.y-lo.y),Float(hi.z-lo.z)))*0.5
                return (center,radius)
            }
            batches.append(Batch(vertices:buffer(vertices),indices:buffer(indices),count:indices.count,flatVertices:flat,instances:(0..<3).map{_ in buffer(instances+instances)},nodes:nodes,textures:maps,terrain:false,shadow:shadow,mode:shadow ? 3:relief ? 1:0,depthDisabled:false,bounds:bounds))
        }
        // Preserve creep edges and independent native sprite chains as overlays.
        for node in Array(commander.creepNodes.values)+Array(commander.worldSprites.nodes.values) {
            guard let geometry=node.geometry,let material=geometry.firstMaterial else {continue}
            node.categoryBitMask=4
            let (vertices,indices)=mesh(geometry,uvTransform:material.diffuse.contentsTransform)
            guard !vertices.isEmpty && !indices.isEmpty else {continue}
            let diffuse=try texture(material.diffuse.contents,fallback:white)
            let instance=Instance(model:node.simdWorldTransform,parameters:SIMD4<Float>(0,Float(node.opacity),0,0))
            let (lo,hi)=node.boundingBox
            let center=SIMD3<Float>(Float((lo.x+hi.x)/2),Float((lo.y+hi.y)/2),Float((lo.z+hi.z)/2))
            let radius=simd_length(SIMD3<Float>(Float(hi.x-lo.x),Float(hi.y-lo.y),Float(hi.z-lo.z)))*0.5
            batches.append(Batch(vertices:buffer(vertices),indices:buffer(indices),count:indices.count,flatVertices:nil,instances:(0..<3).map{_ in buffer([instance])},nodes:[node],textures:[diffuse,normal,black,black,black,white],terrain:false,shadow:true,mode:(material.multiply.contents as? NSColor)==NSColor.black ? 3:0,depthDisabled:!material.readsFromDepthBuffer,bounds:[(center,radius)]))
        }
        visibilityLock.lock()
        visibilityEpoch+=1;lastVisibilityFrame = -1
        visibleLast=[Bool](repeating:true,count:batches.count);visibleNow=visibleLast
        visibilityBuffers=(0..<3).map{_ in device.makeBuffer(length:max(8,batches.count*8),options:.storageModeShared)!}
        visibilityLock.unlock()
        ready=true
        fputs("Metal batches ready: \(batches.count) draws, \(commander.nodes.count) objects\n",stderr)
    }
    func renderOverlay(renderer:SCNRenderer,command:MTLCommandBuffer,output:MTLTexture,camera:SCNNode,eye:Int,time:Double) {
        guard depthViews.indices.contains(eye) else {return}
        let depth=depthViews[eye]
        camera.camera?.categoryBitMask=3
        if overlayColor?.width != output.width || overlayColor?.height != output.height {
            let descriptor=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.bgra8Unorm_srgb,width:output.width,height:output.height,mipmapped:false)
            descriptor.usage=[.renderTarget,.shaderRead];descriptor.storageMode = .private
            overlayResolved=device.makeTexture(descriptor:descriptor)
            if samples>1 {descriptor.textureType = .type2DMultisample;descriptor.sampleCount=samples;descriptor.usage = .renderTarget;overlayColor=device.makeTexture(descriptor:descriptor)}
            else {overlayColor=overlayResolved}
        }
        let pass=overlayPass
        pass.colorAttachments[0].texture=overlayColor;pass.colorAttachments[0].clearColor=MTLClearColorMake(0,0,0,0);pass.colorAttachments[0].loadAction = .clear;pass.colorAttachments[0].storeAction = samples>1 ? .multisampleResolve:.store
        pass.colorAttachments[0].resolveTexture=samples>1 ? overlayResolved:nil
        pass.depthAttachment.texture=depth;pass.depthAttachment.loadAction = .load;pass.depthAttachment.storeAction = .store
        pass.stencilAttachment.texture=depth;pass.stencilAttachment.loadAction = .load;pass.stencilAttachment.storeAction = .store
        renderer.render(atTime:time,viewport:CGRect(x:0,y:0,width:output.width,height:output.height),commandBuffer:command,passDescriptor:pass)
        let composite=compositePass;composite.colorAttachments[0].texture=output
        composite.colorAttachments[0].loadAction = .load;composite.colorAttachments[0].storeAction = .store
        if let encoder=command.makeRenderCommandEncoder(descriptor:composite) {
            encoder.setRenderPipelineState(compositePipeline);encoder.setFragmentTexture(overlayResolved,index:0)
            encoder.drawPrimitives(type:.triangle,vertexStart:0,vertexCount:3);encoder.endEncoding()
        }
    }
    func copyEye(command:MTLCommandBuffer,to output:MTLTexture,eye:Int) {
        guard let source=stereoResolved,let blit=command.makeBlitCommandEncoder() else {return}
        blit.copy(from:source,sourceSlice:eye,sourceLevel:0,sourceOrigin:MTLOrigin(),sourceSize:MTLSize(width:output.width,height:output.height,depth:1),to:output,destinationSlice:0,destinationLevel:0,destinationOrigin:MTLOrigin());blit.endEncoding()
    }
    func inheritedOpacity(_ node:SCNNode)->Float {
        var value=Float(node.opacity),parent=node.parent
        while let current=parent {value*=Float(current.opacity);parent=current.parent}
        return value
    }
    func render(command:MTLCommandBuffer,output:MTLTexture,camera:SCNNode,eye:SIMD3<Float>,rightTransform:simd_float4x4,rightProjection:simd_float4x4) {
        if stereoResolved?.width != output.width || stereoResolved?.height != output.height {
            let d=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.bgra8Unorm_srgb,width:output.width,height:output.height,mipmapped:false)
            d.textureType = .type2DArray;d.arrayLength=2;d.storageMode = .private;d.usage = [.renderTarget,.shaderRead]
            stereoResolved=device.makeTexture(descriptor:d)
            d.usage = .renderTarget
            if samples>1 {d.textureType = .type2DMultisampleArray;d.sampleCount=samples;stereoColor=device.makeTexture(descriptor:d)}
            else {stereoColor=stereoResolved}
            d.pixelFormat = .depth32Float_stencil8;stereoDepth=device.makeTexture(descriptor:d)
            depthViews=(0..<2).compactMap {eye in stereoDepth?.makeTextureView(pixelFormat:.depth32Float_stencil8,textureType:samples>1 ? .type2DMultisample:.type2D,levels:0..<1,slices:eye..<(eye+1))}
            stereoPass.renderTargetArrayLength=2
            stereoPass.colorAttachments[0].texture=stereoColor;stereoPass.colorAttachments[0].resolveTexture=samples>1 ? stereoResolved:nil
            stereoPass.colorAttachments[0].loadAction = .clear;stereoPass.colorAttachments[0].storeAction = samples>1 ? .multisampleResolve:.store
            stereoPass.colorAttachments[0].clearColor=MTLClearColorMake(0.0019349845,0.0027089783,0.0039359395,1)
            stereoPass.depthAttachment.texture=stereoDepth;stereoPass.depthAttachment.loadAction = .clear;stereoPass.depthAttachment.storeAction = .store;stereoPass.depthAttachment.clearDepth=1
        }
        stereoPass.stencilAttachment.texture=stereoDepth
        stereoPass.stencilAttachment.loadAction = .clear;stereoPass.stencilAttachment.storeAction = .store
        guard ready else {return}
        available.wait();let slot=ring;ring=(ring+1)%3
        let query=visibilityBuffers[slot];memset(query.contents(),0,query.length)
        stereoPass.visibilityResultBuffer=query
        guard let encoder=command.makeRenderCommandEncoder(descriptor:stereoPass) else {available.signal();return}
        encoder.label="Instanced terrain, relief and shadows"
        // SceneKit projection uses OpenGL's [-1,1] depth. Metal requires [0,1].
        var clip=matrix_identity_float4x4;clip.columns.2.z=0.5;clip.columns.3.z=0.5
        let vp=clip*simd_float4x4(camera.camera!.projectionTransform)*simd_inverse(camera.simdWorldTransform)
        let rightVP=clip*rightProjection*simd_inverse(rightTransform)
        frame+=1
        visibilityLock.lock()
        let occlusionValid=lastVisibilityFrame>=0 && simd_equal(vp,visibleVP) && simd_equal(rightVP,visibleRightVP) && frame%30 != 0
        for i in visibleNow.indices {visibleNow[i]=visibleLast[i]}
        let epoch=visibilityEpoch,submittedFrame=frame
        visibilityLock.unlock()
        command.addCompletedHandler { [weak self,available] completed in
            defer {available.signal()}
            guard completed.status == .completed,let self=self else {return}
            self.visibilityLock.lock();defer{self.visibilityLock.unlock()}
            guard epoch==self.visibilityEpoch,submittedFrame>self.lastVisibilityFrame else {return}
            let result=query.contents().bindMemory(to:UInt64.self,capacity:self.batches.count)
            for i in self.visibleLast.indices {self.visibleLast[i]=result[i]>0}
            self.visibleVP=vp;self.visibleRightVP=rightVP;self.lastVisibilityFrame=submittedFrame
        }
        var uniforms=Uniforms(left:vp,right:rightVP,leftEye:SIMD4<Float>(eye,camera.camera!.usesOrthographicProjection ? 0:1),rightEye:rightTransform.columns.3)
        encoder.setVertexBytes(&uniforms,length:MemoryLayout<Uniforms>.stride,index:2)
        encoder.setFragmentBytes(&uniforms,length:MemoryLayout<Uniforms>.stride,index:2)
        encoder.setVertexAmplificationCount(2,viewMappings:nil)
        encoder.setCullMode(.none);draws=0;visible=0;culled=0;lodCount=0;occlusionSkipped=0
        let r0=SIMD4<Float>(vp.columns.0.x,vp.columns.1.x,vp.columns.2.x,vp.columns.3.x)
        let r1=SIMD4<Float>(vp.columns.0.y,vp.columns.1.y,vp.columns.2.y,vp.columns.3.y)
        let r2=SIMD4<Float>(vp.columns.0.z,vp.columns.1.z,vp.columns.2.z,vp.columns.3.z)
        let r3=SIMD4<Float>(vp.columns.0.w,vp.columns.1.w,vp.columns.2.w,vp.columns.3.w)
        planes[0]=r3+r0;planes[1]=r3-r0;planes[2]=r3+r1;planes[3]=r3-r1;planes[4]=r2;planes[5]=r3-r2
        for index in 0..<6 {encoder.setFragmentTexture(white,index:index)}
        if let terrain=batches.first(where:{$0.terrain}) {encoder.setFragmentTexture(terrain.textures[0],index:6);encoder.setFragmentTexture(terrain.textures[1],index:7)}
        for shadow in [false,true] {
            encoder.setDepthStencilState(shadow ? shadowDepth:depth)
            for (batchIndex,batch) in batches.enumerated() where batch.shadow==shadow {
                if occlusionValid && !visibleNow[batchIndex] {occlusionSkipped+=1;continue}
                encoder.setRenderPipelineState(pipelines[Int(batch.mode)])
                encoder.setDepthStencilState(batch.depthDisabled ? overlayDepth:shadow ? shadowDepth:depth)
                encoder.setVisibilityResultMode(.counting,offset:batchIndex*8)
                var count=1,farCount=0
                if !batch.terrain {
                    count=0
                    let data=batch.instances[slot].contents().bindMemory(to:Instance.self,capacity:batch.nodes.count*2)
                    for (index,node) in batch.nodes.enumerated() {
                        if node.isHidden {continue}
                        var model=node.simdWorldTransform
                        if node.constraints?.contains(where:{$0 is SCNBillboardConstraint}) == true {
                            let translation=model.columns.3
                            model=simd_float4x4(camera.simdWorldOrientation);model.columns.3=translation
                            model.columns.0 *= node.simdScale.x;model.columns.1 *= node.simdScale.y;model.columns.2 *= node.simdScale.z
                        }
                        let center=model*SIMD4<Float>(batch.bounds[index].0,1),radius=batch.bounds[index].1+simd_length(SIMD3<Float>(rightTransform.columns.3.x,rightTransform.columns.3.y,rightTransform.columns.3.z)-eye)
                        let p=vp*center
                        // Conservative sphere support against all six homogeneous planes.
                        let outside=planes.contains { plane in simd_dot(plane,center) < -radius*simd_length(SIMD3<Float>(plane.x,plane.y,plane.z)) }
                        if outside || p.w<=0 {culled+=1;continue}
                        let instance=Instance(model:model,parameters:SIMD4<Float>(batch.mode,inheritedOpacity(node),0,0))
                        let pixels=batch.bounds[index].1*Float(camera.camera!.projectionTransform.m22)*Float(output.height)/max(p.w,0.001)
                        if batch.flatVertices != nil && pixels<48 {
                            data[batch.nodes.count+farCount]=instance;farCount+=1
                        } else {data[count]=instance;count+=1}
                    }
                    visible+=count+farCount;lodCount+=farCount
                }
                guard count+farCount>0 else {continue}
                encoder.setVertexBuffer(batch.vertices,offset:0,index:0);encoder.setVertexBuffer(batch.instances[slot],offset:0,index:1)
                if batch.terrain {encoder.setFragmentTexture(batch.textures[0],index:6);encoder.setFragmentTexture(batch.textures[1],index:7)}
                else {for (index,texture) in batch.textures.enumerated(){encoder.setFragmentTexture(texture,index:index)}}
                if count>0 {encoder.drawIndexedPrimitives(type:.triangle,indexCount:batch.count,indexType:.uint32,indexBuffer:batch.indices,indexBufferOffset:0,instanceCount:count);draws+=1}
                if farCount>0,let flat=batch.flatVertices {
                    encoder.setVertexBuffer(flat,offset:0,index:0)
                    encoder.setVertexBuffer(batch.instances[slot],offset:batch.nodes.count*MemoryLayout<Instance>.stride,index:1)
                    encoder.drawIndexedPrimitives(type:.triangle,indexCount:6,indexType:.uint32,indexBuffer:flatIndices,indexBufferOffset:0,instanceCount:farCount);draws+=1
                }
            }
        }
        encoder.endEncoding()
    }
    static let shader="""
    #include <metal_stdlib>
    using namespace metal;
    struct CompositeRaster {float4 position [[position]];float2 uv;};
    vertex CompositeRaster compositeVertex(uint id [[vertex_id]]) {
        float2 p=float2((id<<1)&2,id&2);
        return {float4(p*float2(2,-2)+float2(-1,1),0,1),p};
    }
    fragment float4 compositeFragment(CompositeRaster r [[stage_in]],texture2d<float> overlay [[texture(0)]]) {
        constexpr sampler s(filter::nearest);return overlay.sample(s,r.uv);
    }
    constant uint batchMode [[function_constant(0)]];
    struct Vertex {float4 position;float4 uv;};
    struct Instance {float4x4 model;float4 parameters;};
    struct Uniforms {float4x4 vp[2];float4 eye[2];};
    struct Raster {float4 position [[position]];uint slice [[render_target_array_index]];float3 uv;float3 light;float3 view;float4 parameters [[flat]];};
    vertex Raster batchVertex(uint v [[vertex_id]],uint i [[instance_id]],uint eye [[amplification_id]],device const Vertex* vertices [[buffer(0)]],device const Instance* instances [[buffer(1)]],constant Uniforms& u [[buffer(2)]]) {
        Vertex a=vertices[v];Instance b=instances[i];Raster r;
        float4 world=b.model*a.position;r.position=u.vp[eye]*world;r.slice=eye;r.uv=a.uv.xyz;r.parameters=b.parameters;
        float3x3 rotation=float3x3(b.model[0].xyz,b.model[1].xyz,b.model[2].xyz);
        r.light=u.eye[0].w==0 ? float3(-0.22033581,0.54387457,0.80972377):transpose(rotation)*float3(-0.22033581,0.89032002,0.39847509);
        r.view=u.eye[0].w==0 ? float3(0,0,1):transpose(rotation)*(u.eye[eye].xyz-world.xyz);return r;
    }
    fragment float4 batchFragment(Raster r [[stage_in]],texture2d<float> diffuse [[texture(0)]],texture2d<float> normal [[texture(1)]],texture2d<float> bright [[texture(2)]],texture2d<float> specular [[texture(3)]],texture2d<float> emissive [[texture(4)]],texture2d<float> ao [[texture(5)]],texture2d_array<float> terrain [[texture(6)]],texture2d_array<float> fog [[texture(7)]]) {
        constexpr sampler s(coord::normalized,address::clamp_to_edge,filter::linear,mip_filter::none);
        constexpr sampler terrainSampler(coord::normalized,address::clamp_to_edge,filter::linear,mip_filter::linear,max_anisotropy(16));
        if(batchMode==2) {
            float2 size=r.parameters.zw;float2 original=(r.uv.xy*size-8.0)/(size-16.0);
            return float4(terrain.sample(terrainSampler,r.uv.xy,uint(r.uv.z)).rgb*fog.sample(s,float2(original.x,1.0-original.y),uint(r.uv.z)).rgb,1);
        }
        float4 color=diffuse.sample(s,r.uv.xy);if(color.a<0.01) discard_fragment();
        if(batchMode==3) return float4(0,0,0,color.a*r.parameters.y);
        if(batchMode==1) {
            float3 n=normalize(normal.sample(s,r.uv.xy).rgb*2.0-1.0);
            float3 l0=float3(-0.22033581,0.54387457,0.80972377),l=normalize(r.light);
            float a=ao.sample(s,r.uv.xy).g;float3 b=bright.sample(s,r.uv.xy).rgb,sp=specular.sample(s,r.uv.xy).rgb,e=emissive.sample(s,r.uv.xy).rgb;
            float h0=pow(max(dot(n,normalize(l0+float3(0,0,1))),0.0),24.0);
            float h=pow(max(dot(n,normalize(l+normalize(r.view))),0.0),24.0);
            float3 residual=color.rgb-(b*(0.19+a*max(dot(n,l0),0.0))+sp*h0+e);
            color.rgb=max(color.rgb*0.35,residual+b*(0.19+a*max(dot(n,l),0.0))+sp*h+e);
        }
        return float4(color.rgb,color.a*r.parameters.y);
    }
    """
}
