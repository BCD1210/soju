// Explicit per-sample transfers. Expanded rows use a 2x2 sample grid for 4x storage.
// No resolve occurs here: every color, depth and stencil sample survives a round trip.
final class MultisampleTransfer {
    let device: MTLDevice
    let queue: MTLCommandQueue
    let library: MTLLibrary
    var imports = [UInt: MTLRenderPipelineState]()
    var exports = [String: MTLComputePipelineState]()
    let depthState: MTLDepthStencilState
    let stencilState: MTLDepthStencilState
    init(_ device: MTLDevice) throws {
        self.device = device
        guard let queue = device.makeCommandQueue() else { throw Failure("sample transfer queue") }
        self.queue = queue
        let source = """
        #include <metal_stdlib>
        using namespace metal;
        vertex float4 sample_vertex(uint i [[vertex_id]]) {
            float2 p[3] = {float2(-1,-1),float2(3,-1),float2(-1,3)};
            return float4(p[i],0,1);
        }
        uint sample_index(float4 p, uint s, uint width) {
            return (uint(p.y)*2+s/2)*width*2+uint(p.x)*2+s%2;
        }
        fragment float4 import_color(float4 p [[position]], uint s [[sample_id]],
            device const uchar4* data [[buffer(0)]], constant uint2& args [[buffer(1)]]) {
            float4 color = float4(data[sample_index(p,s,args.x)])/255.0f;
            return args.y ? color.bgra : color;
        }
        struct Depth { float z [[depth(any)]]; };
        fragment Depth import_depth(float4 p [[position]], uint s [[sample_id]],
            device const float* data [[buffer(0)]], constant uint2& args [[buffer(1)]]) {
            return {data[sample_index(p,s,args.x)]};
        }
        struct DepthStencil { float z [[depth(any)]]; uint stencil [[stencil]]; };
        fragment DepthStencil import_depth_stencil(float4 p [[position]], uint s [[sample_id]],
            device const float* data [[buffer(0)]], constant uint2& args [[buffer(1)]],
            device const uchar* stencil [[buffer(2)]]) {
            uint i = sample_index(p,s,args.x);
            return {data[i], uint(stencil[i])};
        }
        kernel void export_color(texture2d_ms<float,access::read> image [[texture(0)]],
            device uchar4* data [[buffer(0)]], constant uint2& args [[buffer(1)]], uint2 p [[thread_position_in_grid]]) {
            if (p.x>=image.get_width()*2 || p.y>=image.get_height()*2) return;
            float4 color = image.read(p/2,(p.x%2)+2*(p.y%2));
            data[p.y*image.get_width()*2+p.x] = uchar4(round(clamp(args.y ? color.bgra : color,0.0f,1.0f)*255.0f));
        }
        kernel void export_depth(depth2d_ms<float,access::read> image [[texture(0)]],
            device float* data [[buffer(0)]], uint2 p [[thread_position_in_grid]]) {
            if (p.x>=image.get_width()*2 || p.y>=image.get_height()*2) return;
            data[p.y*image.get_width()*2+p.x] = image.read(p/2,(p.x%2)+2*(p.y%2));
        }
        kernel void export_stencil(texture2d_ms<uint,access::read> image [[texture(0)]],
            device uchar* data [[buffer(0)]], uint2 p [[thread_position_in_grid]]) {
            if (p.x>=image.get_width()*2 || p.y>=image.get_height()*2) return;
            data[p.y*image.get_width()*2+p.x] = uchar(image.read(p/2,(p.x%2)+2*(p.y%2)).x);
        }
        """
        library = try device.makeLibrary(source: source, options: nil)
        let d = MTLDepthStencilDescriptor()
        d.depthCompareFunction = .always; d.isDepthWriteEnabled = true
        guard let depth = device.makeDepthStencilState(descriptor: d) else { throw Failure("sample depth state") }
        depthState = depth
        let stencil = MTLStencilDescriptor()
        stencil.stencilCompareFunction = .always; stencil.depthStencilPassOperation = .replace
        d.frontFaceStencil = stencil; d.backFaceStencil = stencil
        guard let state = device.makeDepthStencilState(descriptor: d) else { throw Failure("sample stencil state") }
        stencilState = state
    }
    func check(_ texture: MTLTexture) throws {
        try need(texture.textureType == .type2DMultisample && texture.sampleCount == 4,
            "sample transfer requires 4x multisample texture")
        try need(texture.width > 0 && texture.height > 0 && texture.width*texture.height <= 4194304,
            "sample transfer storage budget")
        try need([MTLPixelFormat.rgba8Unorm,.bgra8Unorm,.depth32Float,.depth32Float_stencil8].contains(texture.pixelFormat),
            "sample transfer format")
    }
    func storageView(_ texture: MTLTexture) throws -> MTLTexture {
        let format: MTLPixelFormat
        switch texture.pixelFormat {
        case .rgba8Unorm_srgb: format = .rgba8Unorm
        case .bgra8Unorm_srgb: format = .bgra8Unorm
        default: return texture
        }
        guard let view = texture.makeTextureView(pixelFormat: format) else { throw Failure("multisample storage view") }
        return view
    }
    func upload(_ texture: MTLTexture, bytes: Data, stencil: Data = Data()) throws {
        let texture = try storageView(texture)
        try check(texture)
        let depth = texture.pixelFormat == .depth32Float || texture.pixelFormat == .depth32Float_stencil8
        let hasStencil = texture.pixelFormat == .depth32Float_stencil8
        let count = texture.width*texture.height*4
        try need(bytes.count == count*4 && stencil.count == (hasStencil ? count : 0), "sample upload byte count")
        let key = texture.pixelFormat.rawValue
        if imports[key] == nil {
            let d = MTLRenderPipelineDescriptor()
            d.vertexFunction = library.makeFunction(name: "sample_vertex")
            d.fragmentFunction = library.makeFunction(name: depth ? (hasStencil ? "import_depth_stencil" : "import_depth") : "import_color")
            d.rasterSampleCount = 4
            if depth { d.depthAttachmentPixelFormat = texture.pixelFormat }
            else { d.colorAttachments[0].pixelFormat = texture.pixelFormat }
            if hasStencil { d.stencilAttachmentPixelFormat = texture.pixelFormat }
            imports[key] = try device.makeRenderPipelineState(descriptor: d)
        }
        let pass = MTLRenderPassDescriptor()
        if depth {
            pass.depthAttachment.texture = texture; pass.depthAttachment.loadAction = .dontCare; pass.depthAttachment.storeAction = .store
            if hasStencil {
                pass.stencilAttachment.texture = texture; pass.stencilAttachment.loadAction = .dontCare; pass.stencilAttachment.storeAction = .store
            }
        } else {
            pass.colorAttachments[0].texture = texture; pass.colorAttachments[0].loadAction = .dontCare; pass.colorAttachments[0].storeAction = .store
        }
        let data = bytes.withUnsafeBytes { device.makeBuffer(bytes: $0.baseAddress!, length: bytes.count, options: .storageModeShared) }
        guard let data, let cb = queue.makeCommandBuffer(), let encoder = cb.makeRenderCommandEncoder(descriptor: pass) else { throw Failure("sample upload allocation") }
        encoder.setRenderPipelineState(imports[key]!)
        if depth { encoder.setDepthStencilState(hasStencil ? stencilState : depthState) }
        encoder.setFragmentBuffer(data, offset: 0, index: 0)
        var args = SIMD2<UInt32>(UInt32(texture.width), texture.pixelFormat == .bgra8Unorm ? 1 : 0)
        encoder.setFragmentBytes(&args, length: 8, index: 1)
        if hasStencil {
            let data = stencil.withUnsafeBytes { device.makeBuffer(bytes: $0.baseAddress!, length: stencil.count, options: .storageModeShared) }
            guard let data else { throw Failure("sample stencil upload allocation") }
            encoder.setFragmentBuffer(data, offset: 0, index: 2)
        }
        encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
        encoder.endEncoding(); cb.commit(); cb.waitUntilCompleted()
        try need(cb.status == .completed, "sample upload failed: \(String(describing: cb.error))")
    }
    func download(_ texture: MTLTexture, stencil: Bool = false) throws -> Data {
        let texture = try storageView(texture)
        try check(texture)
        let depth = texture.pixelFormat == .depth32Float || texture.pixelFormat == .depth32Float_stencil8
        try need(!stencil || texture.pixelFormat == .depth32Float_stencil8, "sample stencil format")
        let name = stencil ? "export_stencil" : (depth ? "export_depth" : "export_color")
        if exports[name] == nil {
            guard let function = library.makeFunction(name: name) else { throw Failure("sample export function") }
            exports[name] = try device.makeComputePipelineState(function: function)
        }
        let view: MTLTexture
        if stencil {
            guard let stencilView = texture.makeTextureView(pixelFormat: .x32_stencil8) else { throw Failure("sample stencil view") }
            view = stencilView
        } else { view = texture }
        let size = texture.width*texture.height*4*(stencil ? 1 : 4)
        guard let buffer = device.makeBuffer(length: size, options: .storageModeShared),
              let cb = queue.makeCommandBuffer(), let encoder = cb.makeComputeCommandEncoder() else { throw Failure("sample download allocation") }
        encoder.setComputePipelineState(exports[name]!)
        encoder.setTexture(view, index: 0); encoder.setBuffer(buffer, offset: 0, index: 0)
        var args = SIMD2<UInt32>(UInt32(texture.width), texture.pixelFormat == .bgra8Unorm ? 1 : 0)
        encoder.setBytes(&args, length: 8, index: 1)
        encoder.dispatchThreads(MTLSize(width: texture.width*2, height: texture.height*2, depth: 1), threadsPerThreadgroup: MTLSize(width: 8, height: 8, depth: 1))
        encoder.endEncoding(); cb.commit(); cb.waitUntilCompleted()
        try need(cb.status == .completed, "sample download failed: \(String(describing: cb.error))")
        return Data(bytes: buffer.contents(), count: size)
    }
}
