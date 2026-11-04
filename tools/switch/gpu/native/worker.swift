import Foundation
import Metal

struct Failure: Error, CustomStringConvertible { let description: String; init(_ s: String) { description = s } }
func need(_ b: Bool, _ s: String) throws { if !b { throw Failure(s) } }
func compare(_ v: Int) throws -> MTLCompareFunction {
    let n = (0x200...0x207).contains(v) ? v - 0x200 : v - 1
    guard (0...7).contains(n), let c = MTLCompareFunction(rawValue: UInt(n)) else { throw Failure("compare function \(v)") }; return c
}
func blendOp(_ v: Int) throws -> MTLBlendOperation {
    switch v { case 1,0x8006:return .add;case 2,0x800a:return .subtract;case 3,0x800b:return .reverseSubtract;case 4,0x8007:return .min;case 5,0x8008:return .max;default:throw Failure("blend op \(v)") }
}
func blendFactor(_ v: Int) throws -> MTLBlendFactor {
    switch v {case 1,0x4000:return .zero;case 2,0x4001:return .one;case 3,0x4300:return .sourceColor;case 4,0x4301:return .oneMinusSourceColor;case 5,0x4302:return .sourceAlpha;case 6,0x4303:return .oneMinusSourceAlpha;case 7,0x4304:return .destinationAlpha;case 8,0x4305:return .oneMinusDestinationAlpha;case 9,0x4306:return .destinationColor;case 10,0x4307:return .oneMinusDestinationColor;case 11,0x4308:return .sourceAlphaSaturated;case 14,0xc001:return .blendColor;case 15,0xc002:return .oneMinusBlendColor;case 0xc003:return .blendAlpha;case 0xc004:return .oneMinusBlendAlpha;default:throw Failure("blend factor \(v)")}
}
func stencilOp(_ v: Int) throws -> MTLStencilOperation {
    switch v {case 1,0x1e00:return .keep;case 2,0:return .zero;case 3,0x1e01:return .replace;case 4,0x1e02:return .incrementClamp;case 5,0x1e03:return .decrementClamp;case 6,0x150a:return .invert;case 7,0x8507:return .incrementWrap;case 8,0x8508:return .decrementWrap;default:throw Failure("stencil op \(v)")}
}
final class Worker {
    let device: MTLDevice
    let queue: MTLCommandQueue
    var targets = [String: MTLTexture](), generations = [String: Int](), depths = [String: MTLTexture](), depthGenerations = [String: Int]()
    var occlusion: UInt64 = 0
    var queryValues = [String: UInt32]()
    var targetBytes = [String: Int]()
    var pipelines = [String: MTLRenderPipelineState](), depthStates = [String: MTLDepthStencilState]()
    var functions = [String: MTLFunction](), textureCache = [String: MTLTexture]()
    var computePipelines = [String: MTLComputePipelineState]()
    var computeBuffers = [String: MTLBuffer](), computeGenerations = [String: Int]()
    var uploadRing: [MTLBuffer?] = [nil, nil, nil]
    var uploadSlot = 0
    var argumentTables = [String: (MTLBuffer, [MTLTexture], [MTLSamplerState])]()
    init() throws {
        guard let d = MTLCreateSystemDefaultDevice(), let q = d.makeCommandQueue() else { throw Failure("no default Metal device/queue") }
        device = d; queue = q
    }
    func run(_ commands: UnsafeBufferPointer<Command>, _ resources: UnsafeBufferPointer<SpanByte>, _ constants: SpanByte) throws -> String {
    let parseStarted=ProcessInfo.processInfo.systemUptime
    var pipelineHits = 0, textureHits = 0, draws = 0, clears = 0, encoderCount = 0, bufferCount = 0, depthHits = 0
    let started=ProcessInfo.processInfo.systemUptime
    var cb: MTLCommandBuffer? = nil
    var activeEncoder: MTLRenderCommandEncoder? = nil
    var activeKey=""
    var retainedSamplers=[MTLSamplerState]()
    // run returns only after completion; a ring slot cannot still be in flight.
    uploadSlot = (uploadSlot + 1) % 3
    let constantData = UnsafeBufferPointer(start: constants.pointer, count: constants.length)
    if !constantData.isEmpty {
        if (uploadRing[uploadSlot]?.length ?? 0) < constantData.count {
            uploadRing[uploadSlot] = device.makeBuffer(length: max(65536, constantData.count), options: .storageModeShared)
        }
        guard let upload = uploadRing[uploadSlot] else { throw Failure("constant ring allocation") }
        upload.contents().copyMemory(from: constants.pointer!, byteCount: constants.length)
    }
    func endPass() {activeEncoder?.endEncoding();activeEncoder=nil;activeKey=""}
    func flush() throws {
        endPass()
        if let pending=cb {
            pending.commit();pending.waitUntilCompleted()
            try need(pending.status == .completed,"GPU command failed: \(String(describing:pending.error))")
            cb=nil;retainedSamplers.removeAll()
        }
    }
    func commandBuffer() throws -> MTLCommandBuffer {
        if cb==nil {cb=queue.makeCommandBuffer();bufferCount += 1}
        guard let buffer=cb else {throw Failure("command buffer")};return buffer
    }
    func file(_ name: String) throws -> Data {
        guard name.hasPrefix("resource-"), name.hasSuffix(".bin"), let index = Int(name.dropFirst(9).dropLast(4)), resources.indices.contains(index) else { throw Failure("invalid resource reference") }
        let resource = resources[index]
        return Data(bytesNoCopy: UnsafeMutableRawPointer(mutating: resource.pointer!), count: resource.length, deallocator: .none)
    }
    var buffers = [String: MTLBuffer](), fileFunctions = [String: MTLFunction]()
    func buffer(_ name: String) throws -> MTLBuffer {
        if let b = buffers[name] { return b }
        let bytes = try file(name)
        guard !bytes.isEmpty, let b = bytes.withUnsafeBytes({ p in device.makeBuffer(bytes:p.baseAddress!,length:p.count,options:.storageModeShared) }) else { throw Failure("buffer allocation") }; buffers[name] = b; return b
    }
    func function(_ path: String, _ entry: String) throws -> MTLFunction {
        if let f = fileFunctions[path] { return f }
        let source = String(decoding: try file(path), as: UTF8.self)
        if let f = functions[source] { fileFunctions[path] = f; return f }
        let options = MTLCompileOptions(); options.mathMode = .safe
        let library = try device.makeLibrary(source:source, options:options)
        guard let f = library.makeFunction(name:entry) else { throw Failure("entry \(entry)") }; functions[source] = f; fileFunctions[path] = f; return f
    }
    var dispatches=0
    for command in commands {
        if command.kind == 4 {
            endPass()
            let key=text(command.key), source=text(command.source)
            let f=try function(source,"soju_compute")
            let pipelineKey=String(data:try file(source),encoding:.utf8)!
            if computePipelines[pipelineKey] == nil {computePipelines[pipelineKey]=try device.makeComputePipelineState(function:f)}
            let pipeline=computePipelines[pipelineKey]!
            if computeGenerations[key] != Int(command.generation) {
                if computeBuffers[key] != nil {try flush()}
                let bytes=try file(text(command.file))
                guard let storage=bytes.withUnsafeBytes({ p in device.makeBuffer(bytes:p.baseAddress!,length:p.count,options:.storageModeShared) }) else {throw Failure("compute allocation")}
                computeBuffers[key]=storage
                computeGenerations[key]=Int(command.generation)
            }
            let local=[Int(command.local0),Int(command.local1),Int(command.local2)]
            let groups=[Int(command.groups0),Int(command.groups1),Int(command.groups2)]
            try need(local.reduce(1,*) <= pipeline.maxTotalThreadsPerThreadgroup,"compute threadgroup device limit")
            try need(pipeline.staticThreadgroupMemoryLength <= device.maxThreadgroupMemoryLength,"compute shared device limit")
            for axis in 0..<3 {
                let limit=[device.maxThreadsPerThreadgroup.width,device.maxThreadsPerThreadgroup.height,device.maxThreadsPerThreadgroup.depth][axis]
                try need(local[axis] <= limit,"compute local dimension limit")
            }
            guard let encoder=try commandBuffer().makeComputeCommandEncoder() else {throw Failure("compute encoder")}
            encoder.setComputePipelineState(pipeline)
            encoder.setBuffer(computeBuffers[key],offset:0,index:Int(command.slot))
            for c in items(command.constants) {encoder.setBuffer(uploadRing[uploadSlot],offset:Int(c.offset),index:Int(c.slot))}
            encoder.dispatchThreadgroups(MTLSize(width:groups[0],height:groups[1],depth:groups[2]),threadsPerThreadgroup:MTLSize(width:local[0],height:local[1],depth:local[2]))
            encoder.endEncoding();dispatches += 1
            continue
        }
        if command.kind == 2 {try flush();occlusion=0;continue}
        if command.kind == 3 { let key = text(command.key);try flush();queryValues[key]=UInt32(truncatingIfNeeded:occlusion);continue}
        let attachments = items(command.targets)
        guard let target = attachments.first else { throw Failure("missing state") }
        let raw = Dictionary(uniqueKeysWithValues: items(command.registers).map { (Int($0.address), Int($0.value)) })
        func r(_ a: Int) -> Int { raw[a] ?? 0 }
        func f(_ a: Int) -> Double { Double(Float(bitPattern:UInt32(r(a)))) }
        let w = Int(target.width), h = Int(target.height)
        var colors = [MTLTexture]()
        for target in attachments {
            let key = text(target.key)
        if targets[key] == nil {
            let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat:MTLPixelFormat(rawValue:UInt(Int(target.metal)))!,width:w,height:h,mipmapped:false)
            descriptor.storageMode = .shared; descriptor.usage = [.renderTarget,.shaderRead]
            guard let texture = device.makeTexture(descriptor:descriptor) else { throw Failure("color allocation") }; targets[key] = texture; targetBytes[key] = Int(target.bytes)
        }
        let color = targets[key]!
        if generations[key] != Int(target.generation) {
            if generations[key] != nil {try flush()}
            let initial = try file(text(target.file));try need(initial.count == w*h*Int(target.bytes),"target initial bytes")
            initial.withUnsafeBytes { p in color.replace(region:MTLRegionMake2D(0,0,w,h),mipmapLevel:0,withBytes:p.baseAddress!,bytesPerRow:w*Int(target.bytes)) }
            generations[key] = Int(target.generation)
        }
            colors.append(color)
        }
        let mask = Int(command.clear), isClear = command.kind == 1
        let pass = MTLRenderPassDescriptor()
        for (slot, color) in colors.enumerated() {
            let a = pass.colorAttachments[slot]!
            a.texture = color; a.loadAction = isClear && mask & 60 != 0 && (mask >> 6) == slot ? .clear : .load; a.storeAction = .store
            a.clearColor = color.pixelFormat == .r32Uint
                ? MTLClearColor(red:Double(r(0xd80)),green:Double(r(0xd84)),blue:Double(r(0xd88)),alpha:Double(r(0xd8c)))
                : MTLClearColor(red:f(0xd80),green:f(0xd84),blue:f(0xd88),alpha:f(0xd8c))
        }
        try need(r(0x1514)<=1,"invalid occlusion enable")
        let visibility = r(0x1514)==1 && !isClear ? device.makeBuffer(length:8,options:.storageModeShared) : nil
        if let visibility {visibility.contents().storeBytes(of:UInt64(0),as:UInt64.self);pass.visibilityResultBuffer=visibility}
        var depthFormat = MTLPixelFormat.invalid
        if let depth = items(command.depth).first {
            let dk = text(depth.key)
            depthFormat = [0x19,0x14].contains(Int(depth.format)) ? .depth32Float_stencil8 : (Int(depth.format)==0x13 ? .depth16Unorm : .depth32Float)
            if depthGenerations[dk] != Int(depth.generation) {
                if depthGenerations[dk] != nil {try flush()}
                depths.removeValue(forKey:dk)
            }
            if depths[dk] == nil {
                let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat:depthFormat,width:w,height:h,mipmapped:false)
                descriptor.storageMode = .private;descriptor.usage = .renderTarget
                guard let t = device.makeTexture(descriptor:descriptor) else { throw Failure("depth allocation") };depths[dk] = t; depthGenerations[dk] = Int(depth.generation)
                endPass()
                let upload=try commandBuffer()
                guard let blit=upload.makeBlitCommandEncoder() else {throw Failure("depth upload encoder")}
                func plane(_ name:String,_ bytesPerPixel:Int,_ options:MTLBlitOption) throws {
                    let bytes=try file((name == "zfile" ? text(depth.zfile) : text(depth.sfile))), row=((w*bytesPerPixel+255)/256)*256
                    try need(bytes.count==w*h*bytesPerPixel,"depth initial bytes")
                    guard let staging=device.makeBuffer(length:row*h,options:.storageModeShared) else {throw Failure("depth staging")}
                    bytes.withUnsafeBytes {p in for y in 0..<h {memcpy(staging.contents().advanced(by:y*row),p.baseAddress!.advanced(by:y*w*bytesPerPixel),w*bytesPerPixel)} }
                    blit.copy(from:staging,sourceOffset:0,sourceBytesPerRow:row,sourceBytesPerImage:row*h,sourceSize:MTLSize(width:w,height:h,depth:1),to:t,destinationSlice:0,destinationLevel:0,destinationOrigin:MTLOrigin(x:0,y:0,z:0),options:options)
                }
                try plane("zfile",depthFormat == .depth16Unorm ? 2 : 4,depthFormat == .depth32Float_stencil8 ? .depthFromDepthStencil : [])
                if depthFormat == .depth32Float_stencil8 {try plane("sfile",1,.stencilFromDepthStencil)}
                blit.endEncoding()
            }
            pass.depthAttachment.texture = depths[dk];pass.depthAttachment.loadAction = isClear && mask & 1 != 0 ? .clear : .load;pass.depthAttachment.storeAction = .store
            pass.depthAttachment.clearDepth = f(0xd90)
            if depthFormat == .depth32Float_stencil8 {
                pass.stencilAttachment.texture = depths[dk];pass.stencilAttachment.loadAction = isClear && mask & 2 != 0 ? .clear : .load;pass.stencilAttachment.storeAction = .store
                pass.stencilAttachment.clearStencil = UInt32(r(0xda0))
            }
        }
        let depthKey=items(command.depth).first.map { text($0.key) } ?? "none"
        let passKey=attachments.map{text($0.key)}.joined(separator:"|")+"|"+depthKey
        if isClear || visibility != nil || activeKey != passKey {endPass()}
        let current=try commandBuffer()
        if isClear {
            try need(r(0xe00)==0,"scissored clear unsupported")
            guard let encoder = current.makeRenderCommandEncoder(descriptor:pass) else { throw Failure("clear encoder") }; encoderCount += 1; encoder.endEncoding();clears += 1
        } else {
            let pk = text(command.pipeline)
            let vs = try function(text(command.vertex),"soju_vertex"), fs = try function(text(command.fragment),"soju_fragment")
            if pipelines[pk] == nil {
                let descriptor = MTLRenderPipelineDescriptor();descriptor.vertexFunction = vs;descriptor.fragmentFunction = fs
                for slot in colors.indices { descriptor.colorAttachments[slot].pixelFormat = colors[slot].pixelFormat }
                descriptor.depthAttachmentPixelFormat = depthFormat
                if depthFormat == .depth32Float_stencil8 { descriptor.stencilAttachmentPixelFormat = depthFormat }
                try need(r(0x12e4)<=1,"invalid per-target blend enable")
                for slot in colors.indices {
                    let physical = (r(0x121c) >> (4 + slot * 3)) & 7
                    let blend = descriptor.colorAttachments[slot]!
                    try need(r(0x1360 + physical*4)<=1,"invalid blend enable")
                    blend.isBlendingEnabled = r(0x1360 + physical*4)==1
                    if blend.isBlendingEnabled {
                        let per = r(0x12e4)==1, base = 0x1e00 + physical*32
                        let separate = r(per ? base : 0x133c) != 0
                        blend.rgbBlendOperation = try blendOp(r(per ? base+4 : 0x1340))
                        blend.sourceRGBBlendFactor = try blendFactor(r(per ? base+8 : 0x1344))
                        blend.destinationRGBBlendFactor = try blendFactor(r(per ? base+12 : 0x1348))
                        blend.alphaBlendOperation = try blendOp(r(per ? base+(separate ? 16 : 4) : (separate ? 0x134c : 0x1340)))
                        blend.sourceAlphaBlendFactor = try blendFactor(r(per ? base+(separate ? 20 : 8) : (separate ? 0x1350 : 0x1344)))
                        blend.destinationAlphaBlendFactor = try blendFactor(r(per ? base+(separate ? 24 : 12) : (separate ? 0x1358 : 0x1348)))
                    }
                }
                let vertex = MTLVertexDescriptor()
                for a in items(command.attributes) {
                    let slot = Int(a.slot), stream = Int(a.stream)
                    guard let format = MTLVertexFormat(rawValue:UInt(Int(a.format))) else {throw Failure("vertex format")}
                    vertex.attributes[slot].format = format;vertex.attributes[slot].offset = Int(a.offset);vertex.attributes[slot].bufferIndex = stream
                    vertex.layouts[stream].stride = Int(a.stride);vertex.layouts[stream].stepFunction = Int(a.instanced)==1 ? .constant : .perVertex;vertex.layouts[stream].stepRate = Int(a.instanced)==1 ? 0 : 1
                }
                descriptor.vertexDescriptor = vertex
                pipelines[pk] = try device.makeRenderPipelineState(descriptor:descriptor)
            } else {pipelineHits += 1}
            if depthStates[pk] == nil {
                let d = MTLDepthStencilDescriptor()
                try need(r(0x12cc)<=1 && r(0x12e8)<=1 && r(0x1380)<=1,"invalid depth/stencil enable")
                d.depthCompareFunction = r(0x12cc)==0 ? .always : try compare(r(0x130c));d.isDepthWriteEnabled = r(0x12e8)==1 && r(0x12cc)==1
                if r(0x1380)==1 {
                    try need(depthFormat == .depth32Float_stencil8,"stencil format")
                    func stencil(_ back: Bool) throws -> MTLStencilDescriptor {
                        let s = MTLStencilDescriptor();s.stencilCompareFunction = try compare(r(back ? 0x15a4 : 0x1390))
                        s.stencilFailureOperation = try stencilOp(r(back ? 0x1598 : 0x1384));s.depthFailureOperation = try stencilOp(r(back ? 0x159c : 0x1388));s.depthStencilPassOperation = try stencilOp(r(back ? 0x15a0 : 0x138c))
                        s.readMask = UInt32(r(back ? 0xf5c : 0x1398));s.writeMask = UInt32(r(back ? 0xf58 : 0x139c));return s
                    }
                    d.frontFaceStencil = try stencil(false);d.backFaceStencil = try stencil(r(0x1594)==1)
                }
                guard let state = device.makeDepthStencilState(descriptor:d) else {throw Failure("depth/stencil state")};depthStates[pk] = state
            } else {depthHits += 1}
            if activeEncoder==nil {
                activeEncoder=current.makeRenderCommandEncoder(descriptor:pass);activeKey=passKey;encoderCount += 1
            }
            guard let encoder=activeEncoder else {throw Failure("render encoder")}
            if visibility != nil {encoder.setVisibilityResultMode(.counting,offset:0)}
            encoder.setRenderPipelineState(pipelines[pk]!);encoder.setDepthStencilState(depthStates[pk]!)
            let sx=f(0xa00),sy=f(0xa04),sz=f(0xa08),ox=f(0xa0c),oy=f(0xa10),oz=f(0xa14)
            let near = r(0xd7c)==0 ? oz-sz : oz, far=oz+sz
            try need([sx,sy,sz,ox,oy,oz].allSatisfy{$0.isFinite} && sx != 0 && sy != 0 && near>=0 && near<=1 && far>=0 && far<=1,"invalid viewport/depth range")
            encoder.setViewport(MTLViewport(originX:ox-abs(sx),originY:oy-abs(sy),width:2*abs(sx),height:2*abs(sy),znear:near,zfar:far))
            let offset = r(0xdac)==0x1b01 ? r(0xdc4) : r(0xdc8)
            try need([r(0xdc0),r(0xdc4),r(0xdc8)].allSatisfy{$0<=1},"invalid polygon offset enable")
            if offset != 0 {
                try need(r(0x1110)==1,"depth bias requires format-dependent units")
                encoder.setDepthBias(Float(f(0x15bc)),slopeScale:Float(f(0x156c)),clamp:Float(f(0x187c)))
            } else {encoder.setDepthBias(0,slopeScale:0,clamp:0)}
            if r(0xe00) != 0 {
                let x=r(0xe04)&65535,y=r(0xe08)&65535,endX=r(0xe04)>>16,endY=r(0xe08)>>16
                try need(x<endX && y<endY && endX<=w && endY<=h,"invalid scissor")
                encoder.setScissorRect(MTLScissorRect(x:x,y:y,width:endX-x,height:endY-y))
            } else {encoder.setScissorRect(MTLScissorRect(x:0,y:0,width:w,height:h))}
            if r(0x1918) != 0 {
                switch r(0x1920) {case 0x404:encoder.setCullMode(.front);case 0x405:encoder.setCullMode(.back);default:throw Failure("cull face unsupported")}
            } else {encoder.setCullMode(.none)}
            switch r(0x191c) {case 0,0x900:encoder.setFrontFacing(.clockwise);case 0x901:encoder.setFrontFacing(.counterClockwise);default:throw Failure("front winding")}
            try need(r(0xdac)==r(0xdb0) && [0,0x1b01,0x1b02].contains(r(0xdac)),"polygon mode unsupported")
            encoder.setTriangleFillMode(r(0xdac)==0x1b01 ? .lines : .fill)
            encoder.setBlendColor(red:Float(f(0x131c)),green:Float(f(0x1320)),blue:Float(f(0x1324)),alpha:Float(f(0x1328)))
            encoder.setStencilReferenceValues(front:UInt32(r(0x1394)),back:UInt32(r(r(0x1594)==1 ? 0xf54 : 0x1394)))
            for b in items(command.buffers) {
                let slot=Int(b.slot)
                let a=items(command.attributes).first{Int($0.stream)==slot}!
                let offset=Int(a.instanced)==1 ? (Int(command.instance)/Int(a.rate))*Int(a.stride) : 0
                encoder.setVertexBuffer(try buffer(text(b.file)),offset:offset,index:slot)
            }
            for c in items(command.constants) {
                guard let b=uploadRing[uploadSlot] else {throw Failure("missing constant upload")}
                let offset=Int(c.offset)
                if Int(c.stage)==0 {encoder.setVertexBuffer(b,offset:offset,index:Int(c.slot))}else{encoder.setFragmentBuffer(b,offset:offset,index:Int(c.slot))}
            }
            for stage in 0...1 {
                let stageItems=items(command.textures).filter{Int($0.stage)==stage}
                if stageItems.isEmpty {continue}
                let tableKey = pk + ":" + String(stage) + ":" + stageItems.map { item in
                    "\(text(item.key)):\(item.image):\(item.sampler):\(item.count):\(item.u):\(item.v):\(item.min):\(item.mag)"
                }.joined(separator: "|")
                if let (table, resources, _) = argumentTables[tableKey] {
                    for t in resources {encoder.useResource(t,usage:.read,stages:stage==0 ? .vertex : .fragment);textureHits += 1}
                    if stage==0 {encoder.setVertexBuffer(table,offset:0,index:30)}else{encoder.setFragmentBuffer(table,offset:0,index:30)}
                    continue
                }
                var tableTextures=[MTLTexture](), tableSamplers=[MTLSamplerState]()
                let argument=(stage==0 ? vs : fs).makeArgumentEncoder(bufferIndex:30)
                guard let table=device.makeBuffer(length:argument.encodedLength,options:.storageModeShared) else {throw Failure("argument allocation")};argument.setArgumentBuffer(table,offset:0)
                for item in stageItems {
                    if item.image >= 0 {
                        let tk=text(item.key)
                        if textureCache[tk]==nil {
                            let tw=Int(item.width),th=Int(item.height)
                            let d=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:MTLPixelFormat(rawValue:UInt(Int(item.metal)))!,width:tw,height:th,mipmapped:false)
                            d.storageMode = .shared;d.usage = [.shaderRead,.pixelFormatView]
                            guard let t=device.makeTexture(descriptor:d) else {throw Failure("sample texture allocation (BC1 device support required)")}
                            let bytes=try file(text(item.file));let row=Int(item.row)
                            try need(bytes.count == row*Int(item.rows),"sample texture size")
                            bytes.withUnsafeBytes{p in t.replace(region:MTLRegionMake2D(0,0,tw,th),mipmapLevel:0,withBytes:p.baseAddress!,bytesPerRow:row)}
                            let components = [item.swizzle0,item.swizzle1,item.swizzle2,item.swizzle3].map { MTLTextureSwizzle(rawValue:UInt8($0))! }
                            let swizzle = MTLTextureSwizzleChannels(red:components[0],green:components[1],blue:components[2],alpha:components[3])
                            guard let view=t.makeTextureView(pixelFormat:t.pixelFormat,textureType:.type2D,levels:0..<1,slices:0..<1,swizzle:swizzle) else {throw Failure("texture view")}
                            textureCache[tk]=view
                        }else{textureHits += 1}
                        let t=textureCache[tk]!;tableTextures.append(t);argument.setTexture(t,index:Int(item.image));encoder.useResource(t,usage:.read,stages:stage==0 ? .vertex : .fragment)
                    }else{
                        let d=MTLSamplerDescriptor();let modes:[MTLSamplerAddressMode]=[.repeat,.mirrorRepeat,.clampToEdge]
                        d.sAddressMode=modes[Int(item.u)];d.tAddressMode=modes[Int(item.v)];d.normalizedCoordinates=true
                        d.minFilter=Int(item.min)==1 ? .nearest : .linear;d.magFilter=Int(item.mag)==1 ? .nearest : .linear;d.mipFilter = .notMipmapped;d.supportArgumentBuffers=true
                        guard let sampler=device.makeSamplerState(descriptor:d) else {throw Failure("sampler allocation")};tableSamplers.append(sampler);argument.setSamplerState(sampler,index:9*Int(item.count)+Int(item.sampler))
                    }
                }
                argumentTables[tableKey] = (table, tableTextures, tableSamplers)
                if stage==0 {encoder.setVertexBuffer(table,offset:0,index:30)}else{encoder.setFragmentBuffer(table,offset:0,index:30)}
            }
            let topology:MTLPrimitiveType
            switch Int(command.topology) {case 0:topology = .point;case 1:topology = .line;case 3:topology = .lineStrip;case 4:topology = .triangle;case 5:topology = .triangleStrip;default:throw Failure("primitive")}
            let repeats = max(1, Int(command.repeats))
            for _ in 0..<repeats {
            if command.indices.length > 0 { let indexFile=text(command.indices)
                let indices=try buffer(indexFile)
                for segment in items(command.segments) {
                    encoder.drawIndexedPrimitives(type:topology,indexCount:Int(segment.count),indexType:Int(command.index_size)==2 ? .uint16 : .uint32,indexBuffer:indices,indexBufferOffset:Int(segment.start)*Int(command.index_size),instanceCount:1,baseVertex:Int(command.base_vertex),baseInstance:Int(command.instance))
                }
            }else{encoder.drawPrimitives(type:topology,vertexStart:Int(command.first),vertexCount:Int(command.count),instanceCount:1,baseInstance:Int(command.instance))}
            draws += 1
            }
            pipelineHits += repeats - 1; depthHits += repeats - 1
        }
        if let visibility {try flush();occlusion &+= visibility.contents().load(as:UInt64.self)}
    }
    try flush()
    let elapsed=ProcessInfo.processInfo.systemUptime-started
    return "encoders=\(encoderCount) command_buffers=\(bufferCount) depth_hits=\(depthHits) parse_seconds=\(started-parseStarted) seconds=\(elapsed) draws_per_second=\(Double(draws)/max(elapsed,0.000001)) device=\(device.name) dispatches=\(dispatches) draws=\(draws) clears=\(clears) pipelines=\(pipelines.count) pipeline_hits=\(pipelineHits) textures=\(textureCache.count) texture_hits=\(textureHits)"
}
    func readback(_ directory: String) throws -> String {
        let root = URL(fileURLWithPath:directory)
    for (key,t) in targets {
        var bytes=[UInt8](repeating:0,count:t.width*t.height*targetBytes[key]!)
        t.getBytes(&bytes,bytesPerRow:t.width*targetBytes[key]!,from:MTLRegionMake2D(0,0,t.width,t.height),mipmapLevel:0)
        try Data(bytes).write(to:root.appendingPathComponent("\(key).rgba"))
    }
        if !depths.isEmpty {
            guard let cb=queue.makeCommandBuffer(),let blit=cb.makeBlitCommandEncoder() else {throw Failure("depth readback encoder")}
            var planes=[(String,MTLBuffer,Int,Int,Int)]()
            for (key,t) in depths {
                for stencil in [false,true] {
                    if stencil && t.pixelFormat != .depth32Float_stencil8 {try Data().write(to:root.appendingPathComponent("\(key).stencil"));continue}
                    let bpp=stencil ? 1 : (t.pixelFormat == .depth16Unorm ? 2 : 4), row=((t.width*bpp+255)/256)*256
                    guard let buffer=device.makeBuffer(length:row*t.height,options:.storageModeShared) else {throw Failure("depth readback buffer")}
                    let options:MTLBlitOption=t.pixelFormat == .depth32Float_stencil8 ? (stencil ? .stencilFromDepthStencil : .depthFromDepthStencil) : []
                    blit.copy(from:t,sourceSlice:0,sourceLevel:0,sourceOrigin:MTLOrigin(x:0,y:0,z:0),sourceSize:MTLSize(width:t.width,height:t.height,depth:1),to:buffer,destinationOffset:0,destinationBytesPerRow:row,destinationBytesPerImage:row*t.height,options:options)
                    planes.append((key+(stencil ? ".stencil" : ".depth"),buffer,row,t.width*bpp,t.height))
                }
            }
            blit.endEncoding();cb.commit();cb.waitUntilCompleted();try need(cb.status == .completed,"depth readback failed")
            for (name,buffer,row,width,height) in planes {
                var bytes=Data()
                for y in 0..<height {bytes.append(buffer.contents().advanced(by:y*row).assumingMemoryBound(to:UInt8.self),count:width)}
                try bytes.write(to:root.appendingPathComponent(name))
            }
        }
        for (key,value) in queryValues {
            var little=value.littleEndian
            try withUnsafeBytes(of:&little) {try Data($0).write(to:root.appendingPathComponent("\(key).query"))}
        }
        for (key,buffer) in computeBuffers {
            try Data(bytes:buffer.contents(),count:buffer.length).write(to:root.appendingPathComponent("\(key).buffer"))
        }
        return "readback_targets=\(targets.count)"
    }
}
@_cdecl("soju_create")
public func sojuCreate() -> UnsafeMutableRawPointer? {
    guard let worker = try? Worker() else { return nil }
    return Unmanaged.passRetained(worker).toOpaque()
}
@_cdecl("soju_destroy")
public func sojuDestroy(_ pointer: UnsafeMutableRawPointer) { Unmanaged<Worker>.fromOpaque(pointer).release() }
@_cdecl("soju_run")
public func sojuRun(_ pointer: UnsafeMutableRawPointer, _ commands: UnsafePointer<Command>?, _ count: Int, _ resources: UnsafePointer<SpanByte>?, _ resourceCount: Int, _ constants: UnsafePointer<SpanByte>) -> UnsafeMutablePointer<CChar>? {
    autoreleasepool {
        do { return strdup("OK " + (try Unmanaged<Worker>.fromOpaque(pointer).takeUnretainedValue().run(UnsafeBufferPointer(start: commands, count: count), UnsafeBufferPointer(start: resources, count: resourceCount), constants.pointee))) }
        catch { return strdup("ERROR \(error)") }
    }
}
@_cdecl("soju_readback")
public func sojuReadback(_ pointer: UnsafeMutableRawPointer, _ directory: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>? {
    autoreleasepool {
        do { return strdup("OK " + (try Unmanaged<Worker>.fromOpaque(pointer).takeUnretainedValue().readback(String(cString: directory)))) }
        catch { return strdup("ERROR \(error)") }
    }
}
