import os.signpost
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
    struct CameraPacket {
        let transform:simd_float4x4,projection:simd_float4x4
        let orientation:simd_quatf
        let orthographic:Bool
        init(_ node:SCNNode) {
            transform=node.simdWorldTransform;projection=simd_float4x4(node.camera!.projectionTransform)
            orientation=node.simdWorldOrientation;orthographic=node.camera!.usesOrthographicProjection
        }
        init(transform:simd_float4x4,projection:simd_float4x4,orientation:simd_quatf,orthographic:Bool=false) {
            self.transform=transform;self.projection=projection;self.orientation=orientation;self.orthographic=orthographic
        }
    }
    struct DrawNode {
        let transform:simd_float4x4,scale:SIMD3<Float>,opacity:Float,hidden:Bool,billboard:Bool
    }
    struct Input {
        let sceneRevision:Int,terrainRevision:Int,terrainFogRevision:Int,mapWidth:Int
        let terrain:[Int:SCNNode],billboards:[UnitBillboard],overlays:[SCNNode],objectCount:Int
        var retireTerrainSources=false,reuseUnitPoses=false
    }
    fileprivate struct Batch {
        var vertices:MTLBuffer;var indices:MTLBuffer;var count:Int;var flatVertices:MTLBuffer?
        var instances:[MTLBuffer];var nodes:[SCNNode];var textures:[MTLTexture]
        var drawNodes=[DrawNode]()
        var terrain:Bool;var shadow:Bool;var mode:Float;var depthDisabled:Bool;var bounds:[(SIMD3<Float>,Float)]
        var primitive:MTLPrimitiveType = .triangle
    }
    // Construction nodes never cross the publication boundary. GPU instance
    // storage is written only by the render owner after acquiring its ring slot.
    struct PacketBatch {
        let vertices:MTLBuffer,indices:MTLBuffer,count:Int,flatVertices:MTLBuffer?
        let instances:[MTLBuffer],textures:[MTLTexture],drawNodes:[DrawNode]
        let terrain:Bool,shadow:Bool,mode:Float,depthDisabled:Bool,bounds:[(SIMD3<Float>,Float)]
        let primitive:MTLPrimitiveType
        fileprivate init(_ source:Batch) {
            vertices=source.vertices;indices=source.indices;count=source.count;flatVertices=source.flatVertices
            instances=source.instances;textures=source.textures;drawNodes=source.drawNodes
            terrain=source.terrain;shadow=source.shadow;mode=source.mode
            depthDisabled=source.depthDisabled;bounds=source.bounds
            primitive=source.primitive
        }
    }
    let device:MTLDevice,loader:MTKTextureLoader,queue:MTLCommandQueue
    var frameMetrics:XRFrameMetrics?
    var retireSourceOwnership:(()->Void)?
    let pipelines:[MTLRenderPipelineState]
    let linePipelines:[MTLRenderPipelineState]
    let comfortPipeline:MTLRenderPipelineState
    let compositePipeline:MTLRenderPipelineState
    var overlayColor:MTLTexture?,overlayResolved:MTLTexture?
    private var depthViews=[MTLTexture]()
    private let overlayPass=MTLRenderPassDescriptor(),compositePass=MTLRenderPassDescriptor()
    let samples:Int
    let depth:MTLDepthStencilState,shadowDepth:MTLDepthStencilState,overlayDepth:MTLDepthStencilState
    private(set) var batches=[PacketBatch]()
    private let preparationQueue=DispatchQueue(label:"soju.bw.batch-prepare",qos:.userInitiated)
    private var builder:MetalBatches?
    private var preparing=false
    private var capturedSprites=[String:(Int,[Int:SCNGeometry])]()
    private var capturedTerrain=[Int:(SCNGeometry,AnyObject,SCNNode)]()
    private var submittedRevision = -1
    private var submittedGeometry=[(UnitBillboard,Int)]()
    private var packetGeneration = -1,packetSession=""
    func suspend() {ready=false}
    func consume(_ packet:ScenePacket) {
        guard packet.generation != packetGeneration || packet.session != packetSession else {return}
        batches=packet.batches;packetGeneration=packet.generation;packetSession=packet.session
        resetVisibility();ready=true
    }

    private func input(_ commander:Commander)->Input {
        func detached(_ node:SCNNode,geometry preparedGeometry:SCNGeometry?=nil)->SCNNode {
            let copy=SCNNode()
            copy.simdTransform=node.simdWorldTransform;copy.isHidden=node.isHidden || node.parent?.isHidden==true
            copy.opacity=CGFloat(inheritedOpacity(node));copy.constraints=node.constraints
            if let geometry=preparedGeometry {copy.geometry=geometry}
            else if let geometry=node.geometry?.copy() as? SCNGeometry {
                geometry.materials=node.geometry!.materials.map{$0.copy() as! SCNMaterial};copy.geometry=geometry
            }
            node.categoryBitMask=4
            return copy
        }
        var terrain=[Int:SCNNode]()
        terrain.reserveCapacity(commander.terrain.count)
        for (index,node) in commander.terrain {
            guard let geometry=node.geometry else {continue}
            let mask=geometry.firstMaterial?.multiply.contents as AnyObject
            if let cached=capturedTerrain[index],cached.0 === geometry,cached.1 === mask {terrain[index]=cached.2}
            else {
                let copy=detached(node);capturedTerrain[index]=(geometry,mask,copy);terrain[index]=copy
            }
        }
        for index in capturedTerrain.keys where commander.terrain[index]==nil {capturedTerrain.removeValue(forKey:index)}
        let billboards=commander.billboards.values.map { original -> UnitBillboard in
            let copy=UnitBillboard(id:original.root.name ?? "")
            copy.root.isHidden=original.root.isHidden
            copy.nativeLayers=original.nativeLayers;copy.shown=original.shown;copy.shownImages=original.shownImages
            copy.shownColors=original.shownColors;copy.team=original.team;copy.geometryRevision=original.geometryRevision
            let id=original.root.name ?? ""
            let previous=capturedSprites[id]
            let reusable=previous?.0==original.geometryRevision
            var geometries=[Int:SCNGeometry]()
            for (index,node) in original.layers {
                let detachedNode=detached(node,geometry:reusable ? previous?.1[index]:nil)
                copy.layers[index]=detachedNode
                geometries[index]=detachedNode.geometry
            }
            capturedSprites[id]=(original.geometryRevision,geometries)
            return copy
        }
        for id in capturedSprites.keys where commander.billboards[id]==nil {capturedSprites.removeValue(forKey:id)}
        return Input(sceneRevision:commander.sceneRevision,terrainRevision:commander.terrainRevision,
            terrainFogRevision:commander.terrainFogRevision,mapWidth:commander.mapWidth,terrain:terrain,
            billboards:billboards,overlays:(Array(commander.creepNodes.values)+Array(commander.worldSprites.nodes.values)).map{detached($0)},objectCount:commander.nodes.count)
    }
    func prepareAsync(_ commander:Commander) {
        guard !preparing else {return}
        if ready,submittedRevision==commander.sceneRevision,submittedGeometry.count==commander.billboards.count,
           submittedGeometry.allSatisfy({$0.0.poseRevision==$0.1}) {return}
        os_signpost(.begin,log:XRFrameMetrics.log,name:"Snapshot capture")
        let snapshot=input(commander)
        os_signpost(.end,log:XRFrameMetrics.log,name:"Snapshot capture")
        submittedRevision=commander.sceneRevision;submittedGeometry=commander.billboards.values.map{($0,$0.poseRevision)}
        preparing=true
        preparationQueue.async { [self] in
            autoreleasepool {
                do {
                    if builder==nil {builder=try MetalBatches(device:device)}
                    try builder!.prepare(snapshot)
                    let completed=builder!.batches
                    DispatchQueue.main.async { [self] in
                        let retired=batches
                        visibilityLock.lock()
                        visibilityEpoch+=1;lastVisibilityFrame = -1
                        batches=completed
                        for i in completed.indices {visibleLast[i]=true;visibleNow[i]=true}
                        visibilityLock.unlock()
                        ready=true;preparing=false
                        preparationQueue.async {withExtendedLifetime(retired) {}}
                    }
                } catch {
                    DispatchQueue.main.async { [self] in preparing=false;submittedRevision = -1;commander.notice="Batch preparation: \(error)" }
                }
            }
        }
    }

    private let groupCache=ResidentCache<(SCNGeometrySource,Batch)>(limit:96*1024*1024)
    let textures=ResidentCache<(NSImage,MTLTexture)>(limit:128*1024*1024)
    var ready=false,draws=0,visible=0,culled=0,lodCount=0
    var sourceRevision = -1,terrainRevision = -1,terrainFogRevision = -1
    private struct TerrainPage {
        let keys:[Int],sources:[SCNGeometrySource],masks:[AnyObject],diffuse:[AnyObject],poses:[[SIMD4<Float>]],batch:Batch
    }
    private var terrainPageCache=[Int:TerrainPage]()
    private struct UnitStamp:Equatable {
        let unit:UnitBillboard,geometry:Int,pose:Int,hidden:Bool
        init(_ unit:UnitBillboard) {self.unit=unit;geometry=unit.geometryRevision;pose=unit.poseRevision;hidden=unit.root.isHidden}
        static func ==(a:Self,b:Self)->Bool {a.unit === b.unit && a.geometry==b.geometry && a.pose==b.pose && a.hidden==b.hidden}
    }
    private var unitStamps=[ObjectIdentifier:UnitStamp](),preparedUnits=[Batch]()
    private var preparedTerrain=[Batch]()
    var revisions=[(UnitBillboard,Int)]()
    lazy var flatIndices=buffer([UInt32(0),1,2,2,1,3])
    var stereoColor:MTLTexture?,stereoDepth:MTLTexture?,stereoResolved:MTLTexture?
    var stereoPass=MTLRenderPassDescriptor()
    let white:MTLTexture,black:MTLTexture,normal:MTLTexture
    private final class CompletionSlot {
        let available=DispatchSemaphore(value:1)
        var query:MTLBuffer?,epoch=0,frame=0,count=0
        var left=matrix_identity_float4x4,right=matrix_identity_float4x4
        var retained=[PacketBatch]()
        var handler:MTLCommandBufferHandler!
    }
    private let completionSlots=(0..<3).map{_ in CompletionSlot()}
    private var ring=0,frame=0,visibilityEpoch=0
    private let visibilityLock=NSLock()
    private var visibilityBuffers=[MTLBuffer]()
    private var visibleLast=[Bool](repeating:true,count:65536),visibleNow=[Bool](repeating:true,count:65536)
    private var lastVisibilityFrame = -1
    private var visibleVP=matrix_identity_float4x4,visibleRightVP=matrix_identity_float4x4
    private(set) var occlusionSkipped=0
    private var planes=[SIMD4<Float>](repeating:.zero,count:6)
    init(device:MTLDevice)throws {
        guard device.supportsVertexAmplificationCount(2) else {throw SceneError.invalid("Two-view vertex amplification unavailable")}
        let samples=xrRequested ? 1:4;self.samples=samples
        self.device=device;loader=MTKTextureLoader(device:device);queue=device.makeCommandQueue()!
        func solid(_ rgba:[UInt8])throws->MTLTexture {
            let d=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.rgba8Unorm,width:1,height:1,mipmapped:false)
            guard let t=TextureResidency.shared.make(device,d,owner:"render-targets") else {throw SceneError.invalid("Solid texture residency limit")}
            rgba.withUnsafeBytes{t.replace(region:MTLRegionMake2D(0,0,1,1),mipmapLevel:0,withBytes:$0.baseAddress!,bytesPerRow:4)}
            return t
        }
        white=try solid([255,255,255,255]);black=try solid([0,0,0,255]);normal=try solid([128,128,255,255])
        let library=try device.makeLibrary(source:Self.shader,options:nil)
        func makePipeline(_ mode:UInt32,_ topology:MTLPrimitiveTopologyClass)throws->MTLRenderPipelineState {
            let shadow=mode==0 || mode==3
            let d=MTLRenderPipelineDescriptor();d.label=shadow ? "Batched ground shadows":"Batched terrain and relief"
            d.inputPrimitiveTopology=topology
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
        pipelines=try (UInt32(0)..<4).map{try makePipeline($0,.triangle)}
        linePipelines=try (UInt32(0)..<4).map{try makePipeline($0,.line)}
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
        composite.fragmentFunction=library.makeFunction(name:"comfortFragment")
        comfortPipeline=try device.makeRenderPipelineState(descriptor:composite)
        let d=MTLDepthStencilDescriptor();d.depthCompareFunction = .lessEqual;d.isDepthWriteEnabled=true;depth=device.makeDepthStencilState(descriptor:d)!
        d.isDepthWriteEnabled=false;shadowDepth=device.makeDepthStencilState(descriptor:d)!
        d.depthCompareFunction = .always;overlayDepth=device.makeDepthStencilState(descriptor:d)!
        for slot in completionSlots {
            let available=slot.available
            slot.handler={ [weak self,weak slot,available] completed in
                defer {slot?.retained=[];available.signal()}
                guard completed.status == .completed,let self=self,let slot=slot,let query=slot.query else {return}
                self.visibilityLock.lock();defer{self.visibilityLock.unlock()}
                guard slot.epoch==self.visibilityEpoch,slot.frame>self.lastVisibilityFrame else {return}
                let result=query.contents().bindMemory(to:UInt64.self,capacity:slot.count)
                for i in 0..<slot.count {self.visibleLast[i]=result[i]>0}
                self.visibleVP=slot.left;self.visibleRightVP=slot.right;self.lastVisibilityFrame=slot.frame
            }
        }
        visibilityBuffers=(0..<3).map{_ in device.makeBuffer(length:65536*8,options:.storageModeShared)!}
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
    private static func retainedBytes(_ batch:Batch)->Int {
        batch.vertices.length+batch.indices.length+(batch.flatVertices?.length ?? 0)+batch.instances.reduce(0){$0+$1.length}
            + batch.textures.reduce(0){$0+textureBytes($1)}
    }
    static func retainedBytes(_ batch:PacketBatch)->Int {
        batch.vertices.length+batch.indices.length+(batch.flatVertices?.length ?? 0)+batch.instances.reduce(0){$0+$1.length}
            + batch.textures.reduce(0){$0+textureBytes($1)}
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
    func texture(_ value:Any?,fallback:MTLTexture,topLeft:Bool=true,owner:String="cards-overlays")throws->MTLTexture {
        if let t=value as? MTLTexture {return t}
        if let color=value as? NSColor,let rgb=color.usingColorSpace(.sRGB) {
            let key="solid/\(rgb.redComponent)/\(rgb.greenComponent)/\(rgb.blueComponent)/\(rgb.alphaComponent)"
            if let cached=textures.get(key) {return cached.1}
            let descriptor=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.rgba8Unorm_srgb,width:1,height:1,mipmapped:false)
            guard let texture=TextureResidency.shared.make(device,descriptor,owner:owner) else {throw SceneError.invalid("Solid texture residency limit")}
            let bytes=[rgb.redComponent,rgb.greenComponent,rgb.blueComponent,rgb.alphaComponent].map{UInt8(max(0,min(255,($0*255).rounded())))}
            bytes.withUnsafeBytes{texture.replace(region:MTLRegionMake2D(0,0,1,1),mipmapLevel:0,withBytes:$0.baseAddress!,bytesPerRow:4)}
            textures.insert((NSImage(size:NSSize(width:1,height:1)),texture),key:key,bytes:Self.textureBytes(texture));return texture
        }
        guard let image=value as? NSImage else {return fallback}
        let key=String(describing:ObjectIdentifier(image))+"/\(topLeft)"
        if let cached=textures.get(key) {return cached.1}
        guard let cg=image.cgImage(forProposedRect:nil,context:nil,hints:nil) else {return fallback}
        let t=Self.colorView(try TextureResidency.shared.load(loader,Self.colorImage(cg),options:[.SRGB:true,.generateMipmaps:true,.origin:topLeft ? MTKTextureLoader.Origin.topLeft:MTKTextureLoader.Origin.bottomLeft],owner:owner))
        textures.insert((image,t),key:key,bytes:Self.textureBytes(t));return t
    }
    func mesh(_ geometry:SCNGeometry,transform:simd_float4x4=matrix_identity_float4x4,slice:Float=0,uvTransform:SCNMatrix4=SCNMatrix4Identity)->([Vertex],[UInt32]) {
        guard let positions=geometry.sources(for:.vertex).first else {return ([],[])}
        let coords=geometry.sources(for:.texcoord).first
        var vertices=[Vertex](),indices=[UInt32]()
        vertices.reserveCapacity(positions.vectorCount)
        let pOffset=positions.dataOffset,pStride=positions.dataStride,pBytes=positions.bytesPerComponent
        let tOffset=coords?.dataOffset ?? 0,tStride=coords?.dataStride ?? 0,tBytes=coords?.bytesPerComponent ?? 4
        positions.data.withUnsafeBytes { (p:UnsafeRawBufferPointer) in
            (coords?.data ?? Data()).withUnsafeBytes { (t:UnsafeRawBufferPointer) in
                func value(_ data:UnsafeRawBufferPointer,_ offset:Int,_ bytes:Int)->Float {
                    bytes==8 ? Float(data.loadUnaligned(fromByteOffset:offset,as:Double.self)):data.loadUnaligned(fromByteOffset:offset,as:Float.self)
                }
                for i in 0..<positions.vectorCount {
                    let pi=pOffset+i*pStride,ti=tOffset+i*tStride
                    let position=SIMD4<Float>(value(p,pi,pBytes),value(p,pi+pBytes,pBytes),value(p,pi+2*pBytes,pBytes),1)
                    let u=(coords==nil ? 0:value(t,ti,tBytes))*Float(uvTransform.m11)+Float(uvTransform.m41)
                    let v=(coords==nil ? 0:value(t,ti+tBytes,tBytes))*Float(uvTransform.m22)+Float(uvTransform.m42)
                    vertices.append(Vertex(position:position,uv:SIMD4<Float>(u,v,slice,0)))
                }
            }
        }
        // SceneKit primitive sources are normalized; their dimensions live in
        // primitive parameters and are otherwise applied only by SceneKit.
        if let plane=geometry as? SCNPlane {
            let width=(vertices.map{$0.position.x}.max() ?? 0)-(vertices.map{$0.position.x}.min() ?? 0)
            let height=(vertices.map{$0.position.y}.max() ?? 0)-(vertices.map{$0.position.y}.min() ?? 0)
            for i in vertices.indices {
                if width>0 {vertices[i].position.x *= Float(plane.width)/width}
                if height>0 {vertices[i].position.y *= Float(plane.height)/height}
            }
        } else if let sphere=geometry as? SCNSphere {
            let radius=vertices.map{simd_length(SIMD3<Float>($0.position.x,$0.position.y,$0.position.z))}.max() ?? 0
            if radius>0 {for i in vertices.indices {vertices[i].position *= SIMD4<Float>(repeating:Float(sphere.radius)/radius);vertices[i].position.w=1}}
        } else if let torus=geometry as? SCNTorus {
            let radii=vertices.map{hypot($0.position.x,$0.position.z)}
            let lower=radii.min() ?? 0,upper=radii.max() ?? 0,ring=(lower+upper)/2,tube=(upper-lower)/2
            if tube>0 {for i in vertices.indices {
                let radial=radii[i],scale=Float(torus.pipeRadius)/tube
                if radial>0 {
                    let requested=Float(torus.ringRadius)+(radial-ring)*scale
                    vertices[i].position.x *= requested/radial;vertices[i].position.z *= requested/radial
                }
                vertices[i].position.y *= scale
            }}
        }
        for i in vertices.indices {vertices[i].position=transform*vertices[i].position}
        for element in geometry.elements where element.primitiveType == .triangles || element.primitiveType == .line {
            let indexBytes=element.bytesPerIndex,indexCount=element.primitiveCount*(element.primitiveType == .line ? 2:3)
            indices.reserveCapacity(indices.count+indexCount)
            element.data.withUnsafeBytes { bytes in
                for i in 0..<indexCount {
                    let offset=i*indexBytes
                    let index:UInt32=indexBytes==4 ? bytes.loadUnaligned(fromByteOffset:offset,as:UInt32.self):indexBytes==2 ? UInt32(bytes.loadUnaligned(fromByteOffset:offset,as:UInt16.self)):UInt32(bytes[offset])
                    indices.append(index)
                }
            }
        }
        return (vertices,indices)
    }
    func buffer<T>(_ values:[T])->MTLBuffer {values.withUnsafeBytes{device.makeBuffer(bytes:$0.baseAddress!,length:$0.count,options:.storageModeShared)!}}
    func prepare(_ commander:Commander)throws {
        try prepare(input(commander))
    }
    func prepare(_ commander:Input)throws {
        os_signpost(.begin,log:XRFrameMetrics.log,name:"Batch prepare")
        defer {os_signpost(.end,log:XRFrameMetrics.log,name:"Batch prepare")}
        let terrainChanged=terrainRevision != commander.terrainRevision || terrainFogRevision != commander.terrainFogRevision
        sourceRevision=commander.sceneRevision
        revisions=commander.billboards.map{($0,$0.geometryRevision)}
        if terrainChanged {
            preparedTerrain=try prepareTerrain(commander.terrain,width:commander.mapWidth,retireSources:commander.retireTerrainSources)
            terrainRevision=commander.terrainRevision;terrainFogRevision=commander.terrainFogRevision
        }
        var constructed=preparedTerrain
        let stamps=commander.reuseUnitPoses ? Dictionary(uniqueKeysWithValues:commander.billboards.map{(ObjectIdentifier($0),UnitStamp($0))}):[:]
        let unitStart=constructed.count
        if commander.reuseUnitPoses,stamps==unitStamps {constructed+=preparedUnits}
        else {
        var groups=[String:[SCNNode]]()
        for billboard in commander.billboards where !billboard.root.isHidden {
            for (index,native) in billboard.nativeLayers {
                guard let node=billboard.layers[index],let frame=billboard.shown[index] else {continue}
                node.categoryBitMask=4
                let shadow=Int(number(native,"drawfunc"))==10
                let key="\(billboard.shownImages[index] ?? Int(number(native,"id")))/\(frame)/\(WorldSprites.colorKey(billboard.shownColors[index] ?? billboard.team))/\(shadow)"
                groups[key,default:[]].append(node)
            }
        }
        for key in groups.keys.sorted() {
            let nodes=groups[key]!,node=nodes[0]
            guard let geometry=node.geometry,let material=geometry.firstMaterial else {continue}
            let source=geometry.sources(for:.vertex).first
            let shadow=key.hasSuffix("/true"),relief = !(geometry is SCNPlane)
            var maps=[try texture(material.diffuse.contents,fallback:white,topLeft:true)]
            for name in ["normal","bright","specular","emissive","ao_depth"] {
                maps.append(try texture(relief ? (material.value(forKey:"relief_"+name) as? SCNMaterialProperty)?.contents:nil,fallback:name=="normal" ? normal:name=="ao_depth" ? white:black,topLeft:true))
            }
            if let cached=groupCache.get(key),cached.0 === source,cached.1.bounds.count>=nodes.count {
                var batch=cached.1;batch.nodes=nodes;batch.textures=maps;constructed.append(batch);continue
            }
            let (vertices,indices)=mesh(geometry)
            if vertices.isEmpty || indices.isEmpty {continue}
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
            let batch=Batch(vertices:buffer(vertices),indices:buffer(indices),count:indices.count,flatVertices:flat,instances:(0..<3).map{_ in buffer(instances+instances)},nodes:nodes,textures:maps,terrain:false,shadow:shadow,mode:shadow ? 3:relief ? 1:0,depthDisabled:false,bounds:bounds)
            constructed.append(batch)
            if let source=source {
                // Cache geometry capacity only. Materials and nodes have their own
                // owners; retaining them here evicts useful meshes during animation.
                var cached=batch;cached.nodes=[];cached.textures=[]
                groupCache.insert((source,cached),key:key,bytes:Self.retainedBytes(cached))
            }
        }
        }
        let unitEnd=constructed.count
        // Preserve creep edges and independent native sprite chains as overlays.
        for node in commander.overlays {
            guard let geometry=node.geometry,let material=geometry.firstMaterial else {continue}
            node.categoryBitMask=4
            let (vertices,indices)=mesh(geometry,uvTransform:material.diffuse.contentsTransform)
            guard !vertices.isEmpty && !indices.isEmpty else {continue}
            let diffuse=try texture(material.diffuse.contents,fallback:white)
            let instance=Instance(model:node.simdWorldTransform,parameters:SIMD4<Float>(0,Float(node.opacity),0,0))
            let (lo,hi)=node.boundingBox
            let center=SIMD3<Float>(Float((lo.x+hi.x)/2),Float((lo.y+hi.y)/2),Float((lo.z+hi.z)/2))
            let radius=simd_length(SIMD3<Float>(Float(hi.x-lo.x),Float(hi.y-lo.y),Float(hi.z-lo.z)))*0.5
            constructed.append(Batch(vertices:buffer(vertices),indices:buffer(indices),count:indices.count,flatVertices:nil,instances:(0..<3).map{_ in buffer([instance])},nodes:[node],textures:[diffuse,normal,black,black,black,white],terrain:false,shadow:true,mode:(material.multiply.contents as? NSColor)==NSColor.black ? 3:0,depthDisabled:!material.readsFromDepthBuffer,bounds:[(center,radius)]))
            constructed[constructed.count-1].primitive=geometry.elements.first?.primitiveType == .line ? .line:.triangle
        }
        for i in constructed.indices where constructed[i].drawNodes.isEmpty {
            constructed[i].drawNodes=constructed[i].nodes.map { node in
                DrawNode(transform:node.simdWorldTransform,scale:node.simdScale,opacity:inheritedOpacity(node),hidden:node.isHidden,
                    billboard:node.constraints?.contains(where:{$0 is SCNBillboardConstraint})==true)
            }
        }
        if commander.reuseUnitPoses {preparedUnits=Array(constructed[unitStart..<unitEnd]);unitStamps=stamps}
        else {preparedUnits=[];unitStamps=[:]}
        batches=constructed.map(PacketBatch.init)
        resetVisibility()
        ready=true
    }
    private func prepareTerrain(_ terrain:[Int:SCNNode],width:Int,retireSources:Bool)throws->[Batch] {
        // Fixed map-index pages keep a reveal/removal from shifting every later slice.
        let columns=max(1,(width+15)/16)
        let pages=Dictionary(grouping:terrain.keys,by:{($0/max(1,width)/8)*columns+($0%max(1,width)/16)})
        var result=[Batch](),next=[Int:TerrainPage]()
        var sourceBindings=[(SCNMaterial,MTLTexture)]()
        var command=queue.makeCommandBuffer()!,blit=command.makeBlitCommandEncoder()!
        var encoderOpen=true
        defer {if encoderOpen {blit.endEncoding()}}
        func flushUploads(reopen:Bool)throws {
            blit.endEncoding();encoderOpen=false;command.commit();command.waitUntilCompleted()
            guard command.status == .completed else {throw SceneError.invalid("Terrain upload failed")}
            let retired = !sourceBindings.isEmpty
            for (material,texture) in sourceBindings {material.diffuse.contents=texture}
            sourceBindings.removeAll(keepingCapacity:true)
            if reopen && retired {retireSourceOwnership?()}
            if reopen {command=queue.makeCommandBuffer()!;blit=command.makeBlitCommandEncoder()!;encoderOpen=true}
        }
        for page in pages.keys.sorted() {try autoreleasepool {
            let keys=pages[page]!.sorted(),nodes=keys.map{terrain[$0]!}
            let sources=nodes.compactMap{$0.geometry?.sources(for:.vertex).first}
            let masks=nodes.map{$0.geometry!.firstMaterial!.multiply.contents as AnyObject}
            guard sources.count==nodes.count,let first=nodes.first?.geometry?.firstMaterial?.diffuse.contents as? MTLTexture else {return}
            let diffuseSources=nodes.map{$0.geometry!.firstMaterial!.diffuse.contents as AnyObject}
            // Repeated tiles share a diffuse slice; visibility remains per map tile.
            var unique=[MTLTexture](),sliceByTexture=[ObjectIdentifier:Int]()
            let diffuseSlices=diffuseSources.map { source -> Int in
                let identity=ObjectIdentifier(source)
                if let slice=sliceByTexture[identity] {return slice}
                let slice=unique.count;sliceByTexture[identity]=slice;unique.append(source as! MTLTexture);return slice
            }
            let poses=nodes.map{node in let t=node.simdWorldTransform;return [t.columns.0,t.columns.1,t.columns.2,t.columns.3]}
            let previous=terrainPageCache[page]
            let reuse=previous?.keys==keys && previous!.sources.count==sources.count && zip(previous!.sources,sources).allSatisfy{$0 === $1} && previous!.poses==poses && zip(previous!.diffuse,diffuseSources).allSatisfy{$0 === $1}
            if reuse,zip(previous!.masks,masks).allSatisfy({$0 === $1}) {
                next[page]=previous!;result.append(previous!.batch);return
            }
            os_signpost(.begin,log:XRFrameMetrics.log,name:"Terrain page")
            defer {os_signpost(.end,log:XRFrameMetrics.log,name:"Terrain page")}
            let firstMask=try texture(nodes[0].geometry?.firstMaterial?.multiply.contents,fallback:white,owner:"terrain-fog")
            let fd=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.rgba8Unorm_srgb,width:firstMask.width,height:firstMask.height,mipmapped:false)
            fd.textureType = .type2DArray;fd.arrayLength=nodes.count;fd.storageMode = .private;fd.usage = .shaderRead
            guard let fog=TextureResidency.shared.make(device,fd,owner:"terrain-fog") else {throw SceneError.invalid("Fog texture residency limit")}
            let diffuse:MTLTexture
            if reuse {diffuse=previous!.batch.textures[0]}
            else {
                let d=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:first.pixelFormat,width:first.width,height:first.height,mipmapped:true)
                d.textureType = .type2DArray;d.arrayLength=unique.count;d.storageMode = .private;d.usage = .shaderRead
                guard let array=TextureResidency.shared.make(device,d,owner:"terrain-pages") else {throw SceneError.invalid("Terrain texture residency limit")}
                diffuse=array
            }
            if reuse {blit.copy(from:previous!.batch.textures[1],sourceSlice:0,sourceLevel:0,to:fog,destinationSlice:0,destinationLevel:0,sliceCount:nodes.count,levelCount:1)}
            var vertices=[Vertex](),indices=[UInt32](),copied=Set<Int>()
            for (slice,node) in nodes.enumerated() {
                let geometry=node.geometry!,material=geometry.firstMaterial!
                if !reuse,let source=material.diffuse.contents as? MTLTexture {
                    if copied.insert(diffuseSlices[slice]).inserted {
                        for level in 0..<source.mipmapLevelCount {
                            blit.copy(from:source,sourceSlice:0,sourceLevel:level,sourceOrigin:MTLOrigin(),sourceSize:MTLSize(width:max(1,source.width>>level),height:max(1,source.height>>level),depth:1),to:diffuse,destinationSlice:diffuseSlices[slice],destinationLevel:level,destinationOrigin:MTLOrigin())
                        }
                    }
                    var (v,ind)=mesh(geometry,transform:node.simdWorldTransform,slice:Float(diffuseSlices[slice]),uvTransform:material.diffuse.contentsTransform)
                    for index in v.indices {v[index].uv.w=Float(slice)}
                    let base=UInt32(vertices.count);vertices+=v;indices+=ind.map{$0+base}
                }
                if !reuse || previous!.masks[slice] !== masks[slice] {
                    let mask=try texture(material.multiply.contents,fallback:white,owner:"terrain-fog")
                    if mask.width==fog.width && mask.height==fog.height {
                        blit.copy(from:mask,sourceSlice:0,sourceLevel:0,sourceOrigin:MTLOrigin(),sourceSize:MTLSize(width:mask.width,height:mask.height,depth:1),to:fog,destinationSlice:slice,destinationLevel:0,destinationOrigin:MTLOrigin())
                    }
                }
            }
            var batch:Batch
            if reuse {batch=previous!.batch;batch.textures=[diffuse,fog]}
            else {
                let instance=Instance(model:matrix_identity_float4x4,parameters:SIMD4<Float>(2,1,Float(first.width),Float(first.height)))
                batch=Batch(vertices:buffer(vertices),indices:buffer(indices),count:indices.count,flatVertices:nil,
                    instances:(0..<3).map{_ in buffer([instance])},nodes:[],textures:[diffuse,fog],terrain:true,shadow:false,mode:2,depthDisabled:false,bounds:[])
            }
            var retainedSources=diffuseSources
            if !reuse && retireSources {
                // The array now owns the uploaded pixels. Scene nodes can refer
                // to its slices instead of pinning a second full terrain copy.
                let views=try unique.indices.map { slice -> MTLTexture in
                    guard let view=diffuse.makeTextureView(pixelFormat:diffuse.pixelFormat,textureType:.type2D,
                        levels:0..<diffuse.mipmapLevelCount,slices:slice..<(slice+1)) else {throw SceneError.invalid("Terrain slice view")}
                    return view
                }
                retainedSources=diffuseSlices.map{views[$0] as AnyObject}
                for (index,node) in nodes.enumerated() {sourceBindings.append((node.geometry!.firstMaterial!,views[diffuseSlices[index]]))}
            }
            next[page]=TerrainPage(keys:keys,sources:sources,masks:masks,diffuse:retainedSources,poses:poses,batch:batch);result.append(batch)
            // A large initial reveal must retire uploaded source pages before
            // allocating the entire destination alongside the entire source.
            if !sourceBindings.isEmpty,number(TextureResidency.shared.report(),"residentBytes")>Double(TextureResidency.shared.limit)*0.75 {
                try flushUploads(reopen:true)
            }
        }}
        try flushUploads(reopen:false)
        terrainPageCache=next
        return result
    }
    private func resetVisibility() {
        visibilityLock.lock()
        visibilityEpoch+=1;lastVisibilityFrame = -1
        precondition(batches.count<=visibleLast.count,"Native batch visibility capacity exceeded")
        for i in batches.indices {visibleLast[i]=true;visibleNow[i]=true}
        visibilityLock.unlock()
    }
    func renderOverlay(renderer:SCNRenderer,command:MTLCommandBuffer,output:MTLTexture,camera:SCNNode,eye:Int,time:Double)throws {
        guard depthViews.indices.contains(eye) else {return}
        let depth=depthViews[eye]
        camera.camera?.categoryBitMask=3
        if overlayColor?.width != output.width || overlayColor?.height != output.height {
            let descriptor=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.bgra8Unorm_srgb,width:output.width,height:output.height,mipmapped:false)
            descriptor.usage=[.renderTarget,.shaderRead];descriptor.storageMode = .private
            overlayResolved=TextureResidency.shared.make(device,descriptor,owner:"cards-overlays")
            if samples>1 {descriptor.textureType = .type2DMultisample;descriptor.sampleCount=samples;descriptor.usage = .renderTarget;overlayColor=TextureResidency.shared.make(device,descriptor,owner:"cards-overlays")}
            else {overlayColor=overlayResolved}
        }
        guard overlayColor != nil,overlayResolved != nil else {throw SceneError.invalid("Overlay texture residency limit")}
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
    func copyEye(command:MTLCommandBuffer,to output:MTLTexture,eye:Int,fade:Float=0,vignette:Float=0) {
        guard let source=stereoResolved else {return}
        frameMetrics?.created(.blitEncoder)
        guard let blit=command.makeBlitCommandEncoder() else {return}
        blit.copy(from:source,sourceSlice:eye,sourceLevel:0,sourceOrigin:MTLOrigin(),sourceSize:MTLSize(width:output.width,height:output.height,depth:1),to:output,destinationSlice:0,destinationLevel:0,destinationOrigin:MTLOrigin());blit.endEncoding()
        if fade>0 || vignette>0 {
            let pass=compositePass;pass.colorAttachments[0].texture=output
            pass.colorAttachments[0].loadAction = .load;pass.colorAttachments[0].storeAction = .store
            if let encoder=command.makeRenderCommandEncoder(descriptor:pass) {
                var values=SIMD2<Float>(fade,vignette)
                encoder.setRenderPipelineState(comfortPipeline)
                encoder.setFragmentBytes(&values,length:MemoryLayout<SIMD2<Float>>.stride,index:0)
                encoder.drawPrimitives(type:.triangle,vertexStart:0,vertexCount:3);encoder.endEncoding()
            }
        }
    }
    func inheritedOpacity(_ node:SCNNode)->Float {
        var value=Float(node.opacity),parent=node.parent
        while let current=parent {value*=Float(current.opacity);parent=current.parent}
        return value
    }
    func render(command:MTLCommandBuffer,output:MTLTexture,camera:SCNNode,eye:SIMD3<Float>,rightTransform:simd_float4x4,rightProjection:simd_float4x4)throws {
        try render(command:command,output:output,camera:CameraPacket(camera),eye:eye,rightTransform:rightTransform,rightProjection:rightProjection)
    }
    func render(command:MTLCommandBuffer,output:MTLTexture,camera:CameraPacket,eye:SIMD3<Float>,rightTransform:simd_float4x4,rightProjection:simd_float4x4)throws {
        if stereoResolved?.width != output.width || stereoResolved?.height != output.height {
            frameMetrics?.created(.targetRebuild);frameMetrics?.created(.textureDescriptor)
            frameMetrics?.created(.renderTexture,samples>1 ? 3:2);frameMetrics?.created(.textureView,2)
            let d=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:.bgra8Unorm_srgb,width:output.width,height:output.height,mipmapped:false)
            d.textureType = .type2DArray;d.arrayLength=2;d.storageMode = .private;d.usage = [.renderTarget,.shaderRead]
            stereoResolved=TextureResidency.shared.make(device,d,owner:"render-targets")
            d.usage = .renderTarget
            if samples>1 {d.textureType = .type2DMultisampleArray;d.sampleCount=samples;stereoColor=TextureResidency.shared.make(device,d,owner:"render-targets")}
            else {stereoColor=stereoResolved}
            d.pixelFormat = .depth32Float_stencil8;stereoDepth=TextureResidency.shared.make(device,d,owner:"render-targets")
            depthViews=(0..<2).compactMap {eye in stereoDepth?.makeTextureView(pixelFormat:.depth32Float_stencil8,textureType:samples>1 ? .type2DMultisample:.type2D,levels:0..<1,slices:eye..<(eye+1))}
            stereoPass.renderTargetArrayLength=2
            stereoPass.colorAttachments[0].texture=stereoColor;stereoPass.colorAttachments[0].resolveTexture=samples>1 ? stereoResolved:nil
            stereoPass.colorAttachments[0].loadAction = .clear;stereoPass.colorAttachments[0].storeAction = samples>1 ? .multisampleResolve:.store
            stereoPass.colorAttachments[0].clearColor=MTLClearColorMake(0.0019349845,0.0027089783,0.0039359395,1)
            stereoPass.depthAttachment.texture=stereoDepth;stereoPass.depthAttachment.loadAction = .clear;stereoPass.depthAttachment.storeAction = .store;stereoPass.depthAttachment.clearDepth=1
        }
        guard stereoColor != nil,stereoDepth != nil,depthViews.count==2 else {throw SceneError.invalid("Stereo texture residency limit")}
        stereoPass.stencilAttachment.texture=stereoDepth
        stereoPass.stencilAttachment.loadAction = .clear;stereoPass.stencilAttachment.storeAction = .store
        guard ready else {
            frameMetrics?.created(.renderEncoder)
            command.makeRenderCommandEncoder(descriptor:stereoPass)?.endEncoding()
            return
        }
        os_signpost(.begin,log:XRFrameMetrics.log,name:"GPU ring wait")
        completionSlots[ring].available.wait()
        os_signpost(.end,log:XRFrameMetrics.log,name:"GPU ring wait")
        let slot=ring;ring=(ring+1)%3
        let query=visibilityBuffers[slot];memset(query.contents(),0,batches.count*8)
        stereoPass.visibilityResultBuffer=query
        frameMetrics?.created(.renderEncoder)
        guard let encoder=command.makeRenderCommandEncoder(descriptor:stereoPass) else {completionSlots[slot].available.signal();return}
        encoder.label="Instanced terrain, relief and shadows"
        // SceneKit projection uses OpenGL's [-1,1] depth. Metal requires [0,1].
        var clip=matrix_identity_float4x4;clip.columns.2.z=0.5;clip.columns.3.z=0.5
        let vp=clip*camera.projection*simd_inverse(camera.transform)
        let rightVP=clip*rightProjection*simd_inverse(rightTransform)
        frame+=1
        visibilityLock.lock()
        let occlusionValid=lastVisibilityFrame>=0 && simd_equal(vp,visibleVP) && simd_equal(rightVP,visibleRightVP) && frame%30 != 0
        for i in batches.indices {visibleNow[i]=visibleLast[i]}
        let epoch=visibilityEpoch,submittedFrame=frame,queryCount=batches.count
        visibilityLock.unlock()
        let completion=completionSlots[slot]
        completion.retained=batches
        completion.query=query;completion.epoch=epoch;completion.frame=submittedFrame;completion.count=queryCount
        completion.left=vp;completion.right=rightVP
        command.addCompletedHandler(completion.handler)
        var uniforms=Uniforms(left:vp,right:rightVP,leftEye:SIMD4<Float>(eye,camera.orthographic ? 0:1),rightEye:rightTransform.columns.3)
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
        for pass in 0..<2 {
            let shadow=pass==1
            encoder.setDepthStencilState(shadow ? shadowDepth:depth)
            for (batchIndex,batch) in batches.enumerated() where batch.shadow==shadow {
                if occlusionValid && !visibleNow[batchIndex] {occlusionSkipped+=1;continue}
                encoder.setRenderPipelineState((batch.primitive == .line ? linePipelines:pipelines)[Int(batch.mode)])
                encoder.setDepthStencilState(batch.depthDisabled ? overlayDepth:shadow ? shadowDepth:depth)
                encoder.setVisibilityResultMode(.counting,offset:batchIndex*8)
                var count=1,farCount=0
                if !batch.terrain {
                    count=0
                    let data=batch.instances[slot].contents().bindMemory(to:Instance.self,capacity:batch.drawNodes.count*2)
                    for (index,node) in batch.drawNodes.enumerated() {
                        if node.hidden {continue}
                        var model=node.transform
                        if node.billboard {
                            let translation=model.columns.3
                            model=simd_float4x4(camera.orientation);model.columns.3=translation
                            model.columns.0 *= node.scale.x;model.columns.1 *= node.scale.y;model.columns.2 *= node.scale.z
                        }
                        let stretch=max(simd_length(model.columns.0),simd_length(model.columns.1),simd_length(model.columns.2))
                        let center=model*SIMD4<Float>(batch.bounds[index].0,1),radius=batch.bounds[index].1*stretch+simd_length(SIMD3<Float>(rightTransform.columns.3.x,rightTransform.columns.3.y,rightTransform.columns.3.z)-eye)
                        let p=vp*center
                        // Conservative sphere support against all six homogeneous planes.
                        var outside=false
                        for plane in planes {
                            if simd_dot(plane,center) < -radius*simd_length(SIMD3<Float>(plane.x,plane.y,plane.z)) {outside=true;break}
                        }
                        if outside || p.w<=0 {culled+=1;continue}
                        let instance=Instance(model:model,parameters:SIMD4<Float>(batch.mode,node.opacity,0,0))
                        let pixels=batch.bounds[index].1*camera.projection.columns.1.y*Float(output.height)/max(p.w,0.001)
                        if batch.flatVertices != nil && pixels<48 {
                            data[batch.drawNodes.count+farCount]=instance;farCount+=1
                        } else {data[count]=instance;count+=1}
                    }
                    visible+=count+farCount;lodCount+=farCount
                }
                guard count+farCount>0 else {continue}
                encoder.setVertexBuffer(batch.vertices,offset:0,index:0);encoder.setVertexBuffer(batch.instances[slot],offset:0,index:1)
                if batch.terrain {encoder.setFragmentTexture(batch.textures[0],index:6);encoder.setFragmentTexture(batch.textures[1],index:7)}
                else {for (index,texture) in batch.textures.enumerated(){encoder.setFragmentTexture(texture,index:index)}}
                if count>0 {encoder.drawIndexedPrimitives(type:batch.primitive,indexCount:batch.count,indexType:.uint32,indexBuffer:batch.indices,indexBufferOffset:0,instanceCount:count);draws+=1}
                if farCount>0,let flat=batch.flatVertices {
                    encoder.setVertexBuffer(flat,offset:0,index:0)
                    encoder.setVertexBuffer(batch.instances[slot],offset:batch.drawNodes.count*MemoryLayout<Instance>.stride,index:1)
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
    fragment float4 comfortFragment(CompositeRaster r [[stage_in]],constant float2 &comfort [[buffer(0)]]) {
        float edge=smoothstep(0.35,1.0,length((r.uv-0.5)*2));
        return float4(0,0,0,max(comfort.x,edge*comfort.y));
    }
    constant uint batchMode [[function_constant(0)]];
    struct Vertex {float4 position;float4 uv;};
    struct Instance {float4x4 model;float4 parameters;};
    struct Uniforms {float4x4 vp[2];float4 eye[2];};
    struct Raster {float4 position [[position]];uint slice [[render_target_array_index]];float4 uv;float3 light;float3 view;float4 parameters [[flat]];};
    vertex Raster batchVertex(uint v [[vertex_id]],uint i [[instance_id]],uint eye [[amplification_id]],device const Vertex* vertices [[buffer(0)]],device const Instance* instances [[buffer(1)]],constant Uniforms& u [[buffer(2)]]) {
        Vertex a=vertices[v];Instance b=instances[i];Raster r;
        float4 world=b.model*a.position;r.position=u.vp[eye]*world;r.slice=eye;r.uv=a.uv;r.parameters=b.parameters;
        float3x3 rotation=float3x3(b.model[0].xyz,b.model[1].xyz,b.model[2].xyz);
        r.light=u.eye[0].w==0 ? float3(-0.22033581,0.54387457,0.80972377):transpose(rotation)*float3(-0.22033581,0.89032002,0.39847509);
        r.view=u.eye[0].w==0 ? float3(0,0,1):transpose(rotation)*(u.eye[eye].xyz-world.xyz);return r;
    }
    fragment float4 batchFragment(Raster r [[stage_in]],texture2d<float> diffuse [[texture(0)]],texture2d<float> normal [[texture(1)]],texture2d<float> bright [[texture(2)]],texture2d<float> specular [[texture(3)]],texture2d<float> emissive [[texture(4)]],texture2d<float> ao [[texture(5)]],texture2d_array<float> terrain [[texture(6)]],texture2d_array<float> fog [[texture(7)]]) {
        constexpr sampler s(coord::normalized,address::clamp_to_edge,filter::linear,mip_filter::none);
        constexpr sampler terrainSampler(coord::normalized,address::clamp_to_edge,filter::linear,mip_filter::linear,max_anisotropy(16));
        if(batchMode==2) {
            float2 size=r.parameters.zw;float2 original=(r.uv.xy*size-8.0)/(size-16.0);
            return float4(terrain.sample(terrainSampler,r.uv.xy,uint(r.uv.z)).rgb*fog.sample(s,float2(original.x,1.0-original.y),uint(r.uv.w)).rgb,1);
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
