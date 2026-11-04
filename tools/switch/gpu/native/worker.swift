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
struct StateTransition: Hashable { let previous: Int; let current: Int }
struct ArgumentTable {
    let key: String
    let buffer: MTLBuffer
    let offset: Int
    let textures: [MTLTexture]
    let usage: [CacheStamp]
    let samplers: [MTLSamplerState]
}
// Graphics shaders have no writable storage. Within a span without clear/query/depth
// operations, disjoint color sets commute; preserve order within every color set.
func schedulePasses(_ commands: UnsafeBufferPointer<Command>) throws -> [Int] {
    var order = [Int](), groups = [[Int]](), groupIDs = [String: Int]()
    var owners = [String: (String, Int64)]()
    var descriptors = [Int: (String, [(String, Int64)])]()
    func drain() {
        for group in groups { order.append(contentsOf: group) }
        groups.removeAll(keepingCapacity: true);groupIDs.removeAll(keepingCapacity: true);owners.removeAll(keepingCapacity: true)
    }
    for (index, input) in commands.enumerated() {
        let source = (input.kind == 5 || input.kind == 6) ? Int(input.reference) : index
        try need(source >= 0 && source <= index, "invalid scheduling reference")
        let command = commands[source]
        if command.kind != 0 || command.depth.length != 0 || items(command.registers).contains(where: { $0.address == 0x1514 && $0.value != 0 }) {
            drain();order.append(index);continue
        }
        let descriptor: (String, [(String, Int64)])
        if let cached = descriptors[source] { descriptor = cached } else {
            let targets = items(command.targets).map { (text($0.key), $0.generation) }
            descriptor = (targets.map { $0.0 }.joined(separator: "|"), targets)
            descriptors[source] = descriptor
        }
        let (key, targets) = descriptor
        if targets.contains(where: { target in
            if let owner = owners[target.0] { return owner.0 != key || owner.1 != target.1 }
            return false
        }) { drain() }
        let group: Int
        if let existing = groupIDs[key] { group = existing } else {
            group = groups.count;groupIDs[key] = group;groups.append([])
        }
        for target in targets { owners[target.0] = (key, target.1) }
        groups[group].append(index)
    }
    drain()
    return order
}
struct RenderPassState {
    let descriptor: MTLRenderPassDescriptor
    let key: String
    let colors: [MTLTexture]
    let depthFormat: MTLPixelFormat
    let usage: [CacheStamp]
}
final class Worker {
    let device: MTLDevice
    let queue: MTLCommandQueue
    var targets = [String: MTLTexture](), generations = [String: Int](), depths = [String: MTLTexture](), depthGenerations = [String: Int]()
    var clearPipelines = [UInt: MTLRenderPipelineState]()
    var occlusion: UInt64 = 0
    var queryValues = [String: UInt32]()
    var targetBytes = [String: Int]()
    let pressure = MemoryPressure()
    var cacheClock: UInt64 = 0
    var lastUse = [String: CacheStamp]()
    let cacheBudget = max(0, Int(ProcessInfo.processInfo.environment["SOJU_GPU_CACHE_BYTES"] ?? "") ?? 268435456)
    let spillRoot = URL(fileURLWithPath: FileManager.default.currentDirectoryPath + "/target/soju-spill-" + UUID().uuidString, isDirectory: true)
    var dirtyAttachments = Set<String>()
    var spilledColors = [String: Int](), spilledDepths = [String: Int]()
    deinit { try? FileManager.default.removeItem(at: spillRoot) }
    let compiler: RenderCompiler
    var renderFunctions = [String: (MTLFunction, MTLFunction)]()
    var pipelines = [String: MTLRenderPipelineState](), depthStates = [String: MTLDepthStencilState]()
    var functions = [String: MTLFunction](), textureCache = [String: MTLTexture]()
    var computePipelines = [String: MTLComputePipelineState]()
    var computeBuffers = [String: MTLBuffer](), computeGenerations = [String: Int]()
    var readbackBuffers = [MTLBuffer]()
    var uploadRing: [MTLBuffer?] = [nil, nil, nil]
    var uploadSlot = 0
    var argumentTables = [String: ArgumentTable]()
    var renderPasses = [String: MTLRenderPassDescriptor]()
    var argumentHeaps = [String: (MTLBuffer, Int)]()
    init(device shared: MTLDevice? = nil, compiler sharedCompiler: RenderCompiler? = nil) throws {
        guard let d = shared ?? MTLCreateSystemDefaultDevice(), let q = d.makeCommandQueue() else { throw Failure("no default Metal device/queue") }
        device = d; queue = q; compiler = sharedCompiler ?? RenderCompiler(d)
    }
    func run(_ commands: UnsafeBufferPointer<Command>, _ resources: UnsafeBufferPointer<SpanByte>, _ constants: SpanByte) throws -> String {
    let encoding = NativeSpan("native_run")
    defer { encoding.end() }
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
    var boundPipeline = ""
    var boundRegisters = [Int: Int]()
    var boundCommand = -1
    var groupChanges = [StateTransition: UInt8]()
    var viewportRegisters = [0xd7c]
    var scissorRegisters = [Int]()
    for index in 0..<16 {
        for offset in stride(from: 0, through: 20, by: 4) { viewportRegisters.append(0xa00 + index*32 + offset) }
        for offset in [0,4,8] { scissorRegisters.append(0xe00 + index*16 + offset) }
    }
    let registerGroups = [viewportRegisters, [0xdac,0xdc0,0xdc4,0xdc8,0x1110,0x15bc,0x156c,0x187c], scissorRegisters, [0x1918,0x1920,0x191c,0xdac,0xdb0], [0x131c,0x1320,0x1324,0x1328], [0x1394,0x1594,0xf54]]
    var boundBuffers = [Int: (ObjectIdentifier, Int)]()
    var resident = Set<ObjectIdentifier>()
    var stateBinds = 0, offsetWrites = 0, passHits = 0
    var gpuWait = 0.0
    var skippedDraws = 0
    func endPass() {
        activeEncoder?.endEncoding();activeEncoder=nil;activeKey=""
        boundCommand = -1
        boundPipeline="";boundRegisters.removeAll(keepingCapacity:true);boundBuffers.removeAll(keepingCapacity:true);resident.removeAll(keepingCapacity:true)
    }
    func bind(_ encoder: MTLRenderCommandEncoder, _ stage: Int, _ buffer: MTLBuffer, _ offset: Int, _ slot: Int) {
        let key = stage * 32 + slot, identity = ObjectIdentifier(buffer)
        if let old = boundBuffers[key], old.0 == identity {
            if old.1 == offset { return }
            if stage == 0 { encoder.setVertexBufferOffset(offset, index: slot) } else { encoder.setFragmentBufferOffset(offset, index: slot) }
            offsetWrites += 1
        } else {
            if stage == 0 { encoder.setVertexBuffer(buffer, offset: offset, index: slot) } else { encoder.setFragmentBuffer(buffer, offset: offset, index: slot) }
            stateBinds += 1
        }
        boundBuffers[key] = (identity, offset)
    }
    func bindTable(_ encoder: MTLRenderCommandEncoder, _ stage: Int, _ table: ArgumentTable) {
        for stamp in table.usage { use(stamp) }
        for texture in table.textures {
            if resident.insert(ObjectIdentifier(texture)).inserted { encoder.useResource(texture, usage: .read, stages: [.vertex, .fragment]) }
        }
        bind(encoder, stage, table.buffer, table.offset, 30)
    }
    func flush() throws {
        endPass()
        if let pending=cb {
            let completion = NativeSpan("gpu_completion")
            defer { completion.end() }
            let waitStarted = ProcessInfo.processInfo.systemUptime
            pending.commit();pending.waitUntilCompleted()
            gpuWait += ProcessInfo.processInfo.systemUptime - waitStarted
            try need(pending.status == .completed,"GPU command failed: \(String(describing:pending.error))")
            cb=nil;retainedSamplers.removeAll()
        }
    }
    func commandBuffer() throws -> MTLCommandBuffer {
        if cb==nil {cb=queue.makeCommandBuffer();bufferCount += 1}
        guard let buffer=cb else {throw Failure("command buffer")};return buffer
    }
    func resource(_ name: String) throws -> SpanByte {
        guard name.hasPrefix("resource-"), name.hasSuffix(".bin"), let index = Int(name.dropFirst(9).dropLast(4)), resources.indices.contains(index) else { throw Failure("invalid resource reference") }
        return resources[index]
    }
    func file(_ name: String) throws -> Data {
        let resource = try resource(name)
        return Data(bytesNoCopy: UnsafeMutableRawPointer(mutating: resource.pointer!), count: resource.length, deallocator: .none)
    }
    var buffers = [String: MTLBuffer](), fileFunctions = [String: MTLFunction]()
    func buffer(_ name: String) throws -> MTLBuffer {
        if let b = buffers[name] { return b }
        let upload = NativeSpan("buffer_upload")
        defer { upload.end() }
        let span = try resource(name)
        // Rust captures are page-aligned, padded, immutable, and owned until GPU completion.
        guard span.length > 0, let pointer = span.pointer,
              let b = device.makeBuffer(bytesNoCopy: UnsafeMutableRawPointer(mutating: pointer), length: ((span.length + 16383) / 16384) * 16384, options: .storageModeShared, deallocator: nil)
        else { throw Failure("shared capture buffer allocation") }
        buffers[name] = b; return b
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
    var vertexBindings = [Int: [(Int, MTLBuffer, Int)]]()
    var passStates = [Int: RenderPassState]()
    var registerStates = [Int: [Int: Int]]()
    var resolvedTables = [Int: [Int: ArgumentTable]]()
    let scheduled = try schedulePasses(commands)
    let depthPlans = sharedDepthPlans(commands, scheduled)
    var mergedKeys = [Int: String]()
    var pipelineUsage = [Int: CacheStamp]()
    for inputIndex in scheduled {
        let input = commands[inputIndex]
        let commandIndex = (input.kind == 5 || input.kind == 6) ? Int(input.reference) : inputIndex
        try need(commandIndex >= 0 && commandIndex <= inputIndex, "invalid command reference")
        var command = commands[commandIndex]
        if input.kind == 6 { command.first = input.first; command.count = input.count }
        if command.kind == 4 {
            endPass()
            let key=text(command.key), source=text(command.source)
            let f=try function(source,"soju_compute")
            let pipelineKey=String(data:try file(source),encoding:.utf8)!
            if computePipelines[pipelineKey] == nil {computePipelines[pipelineKey]=try device.makeComputePipelineState(function:f)}
            let pipeline=computePipelines[pipelineKey]!
            touch("k", pipelineKey)
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
        let originalAttachments = items(command.targets)
        let merged = depthPlans[inputIndex]
        let passStateIndex = merged == nil ? commandIndex : commands.count + inputIndex
        guard let target = originalAttachments.first else { throw Failure("missing state") }
        if registerStates[commandIndex] == nil { registerStates[commandIndex] = Dictionary(uniqueKeysWithValues: items(command.registers).map { (Int($0.address), Int($0.value)) }) }
        let raw = registerStates[commandIndex]!
        func r(_ a: Int) -> Int { raw[a] ?? 0 }
        func f(_ a: Int) -> Double { Double(Float(bitPattern:UInt32(r(a)))) }
        let w = Int(target.width), h = Int(target.height)
        let mask = Int(command.clear), isClear = command.kind == 1
        try need(r(0x1514)<=1,"invalid occlusion enable")
        let visibility = r(0x1514)==1 && !isClear ? device.makeBuffer(length:8,options:.storageModeShared) : nil
        var colors = [MTLTexture]()
        var depthFormat = MTLPixelFormat.invalid
        let pass: MTLRenderPassDescriptor
        let passKey: String
        if let resolved = passStates[passStateIndex] {
            for stamp in resolved.usage { use(stamp) }
            pass = resolved.descriptor; passKey = resolved.key; colors = resolved.colors; depthFormat = resolved.depthFormat; passHits += 1
        } else {

        let attachments = merged?.attachments ?? Array(originalAttachments)
        for target in attachments {
            let key = text(target.key)
        touch("c", key); dirtyAttachments.insert(key)
        let newTarget = targets[key] == nil
        if newTarget {
            let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat:MTLPixelFormat(rawValue:UInt(Int(target.metal)))!,width:w,height:h,mipmapped:false)
            descriptor.storageMode = .shared; descriptor.usage = [.renderTarget,.shaderRead]
            guard let texture = device.makeTexture(descriptor:descriptor) else { throw Failure("color allocation") }; targets[key] = texture; targetBytes[key] = Int(target.bytes)
        }
        let color = targets[key]!
        if newTarget || generations[key] != Int(target.generation) {
            if generations[key] != nil {try flush()}
            let textureUpload = NativeSpan("texture_upload")
            defer { textureUpload.end() }
            let initial = try restore(key, Int(target.generation), ".rgba") { try file(text(target.file)) };try need(initial.count == w*h*Int(target.bytes),"target initial bytes")
            initial.withUnsafeBytes { p in color.replace(region:MTLRegionMake2D(0,0,w,h),mipmapLevel:0,withBytes:p.baseAddress!,bytesPerRow:w*Int(target.bytes)) }
            generations[key] = Int(target.generation)
        }
            colors.append(color)
        }
        let depthKey = items(command.depth).first.map { text($0.key) + ":" + String($0.generation) } ?? "none"
        passKey = attachments.map{ text($0.key) }.joined(separator:"|")+"|"+depthKey
        let cachedPass = !isClear && visibility == nil ? renderPasses[passKey] : nil
        pass = cachedPass ?? MTLRenderPassDescriptor()
        if cachedPass != nil { passHits += 1 }
        if cachedPass == nil {
        for (slot, color) in colors.enumerated() {
            let a = pass.colorAttachments[slot]!
            a.texture = color; a.loadAction = isClear && mask & 60 != 0 && (mask >> 6) == slot ? .clear : .load; a.storeAction = .store
            a.clearColor = color.pixelFormat == .r32Uint
                ? MTLClearColor(red:Double(r(0xd80)),green:Double(r(0xd84)),blue:Double(r(0xd88)),alpha:Double(r(0xd8c)))
                : MTLClearColor(red:f(0xd80),green:f(0xd84),blue:f(0xd88),alpha:f(0xd8c))
        }
        }
        if let visibility {visibility.contents().storeBytes(of:UInt64(0),as:UInt64.self);pass.visibilityResultBuffer=visibility}

        if let depth = items(command.depth).first {
            let dk = text(depth.key)
            touch("d", dk); dirtyAttachments.insert(dk)
            depthFormat = [0x19,0x14].contains(Int(depth.format)) ? .depth32Float_stencil8 : (Int(depth.format)==0x13 ? .depth16Unorm : .depth32Float)
            if depthGenerations[dk] != Int(depth.generation) {
                if depthGenerations[dk] != nil {try flush()}
                depths.removeValue(forKey:dk)
                invalidatePasses()
            }
            if depths[dk] == nil {
                let textureUpload = NativeSpan("texture_upload")
                defer { textureUpload.end() }
                let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat:depthFormat,width:w,height:h,mipmapped:false)
                descriptor.storageMode = .private;descriptor.usage = .renderTarget
                guard let t = device.makeTexture(descriptor:descriptor) else { throw Failure("depth allocation") };depths[dk] = t; depthGenerations[dk] = Int(depth.generation)
                endPass()
                let upload=try commandBuffer()
                guard let blit=upload.makeBlitCommandEncoder() else {throw Failure("depth upload encoder")}
                func plane(_ name:String,_ bytesPerPixel:Int,_ options:MTLBlitOption) throws {
                    let bytes=try restore(dk, Int(depth.generation), name == "zfile" ? ".depth" : ".stencil") { try file((name == "zfile" ? text(depth.zfile) : text(depth.sfile))) }, row=((w*bytesPerPixel+255)/256)*256
                    try need(bytes.count==w*h*bytesPerPixel,"depth initial bytes")
                    guard let staging=device.makeBuffer(length:row*h,options:.storageModeShared) else {throw Failure("depth staging")}
                    bytes.withUnsafeBytes {p in for y in 0..<h {memcpy(staging.contents().advanced(by:y*row),p.baseAddress!.advanced(by:y*w*bytesPerPixel),w*bytesPerPixel)} }
                    blit.copy(from:staging,sourceOffset:0,sourceBytesPerRow:row,sourceBytesPerImage:row*h,sourceSize:MTLSize(width:w,height:h,depth:1),to:t,destinationSlice:0,destinationLevel:0,destinationOrigin:MTLOrigin(x:0,y:0,z:0),options:options)
                }
                try plane("zfile",depthFormat == .depth16Unorm ? 2 : 4,depthFormat == .depth32Float_stencil8 ? .depthFromDepthStencil : [])
                if depthFormat == .depth32Float_stencil8 {try plane("sfile",1,.stencilFromDepthStencil)}
                blit.endEncoding()
            }
            if cachedPass == nil {
            pass.depthAttachment.texture = depths[dk];pass.depthAttachment.loadAction = isClear && mask & 1 != 0 ? .clear : .load;pass.depthAttachment.storeAction = .store
            pass.depthAttachment.clearDepth = f(0xd90)
            if depthFormat == .depth32Float_stencil8 {
                pass.stencilAttachment.texture = depths[dk];pass.stencilAttachment.loadAction = isClear && mask & 2 != 0 ? .clear : .load;pass.stencilAttachment.storeAction = .store
                pass.stencilAttachment.clearStencil = UInt32(r(0xda0))
            }
            }
        }
        if !isClear && visibility == nil {
            touch("r", passKey)
            renderPasses[passKey] = pass
            passStates[passStateIndex] = RenderPassState(descriptor: pass, key: passKey, colors: colors, depthFormat: depthFormat, usage: [cacheStamp("r", passKey)] + attachments.map { cacheStamp("c", text($0.key)) } + items(command.depth).map { cacheStamp("d", text($0.key)) })
        }
        }
        if isClear || visibility != nil || activeKey != passKey {endPass()}
        let current=try commandBuffer()
        if isClear {
            if r(0xe00) != 0 {
                try need(mask & 3 == 0 && mask & 60 == 60 && colors.count == 1 && mask >> 6 == 0,
                         "scissored clear requires one color target and no depth/stencil")
                let color = colors[0]
                try need(color.pixelFormat == .rgba8Unorm || color.pixelFormat == .bgra8Unorm,
                         "scissored clear color format")
                let x = r(0xe04) & 65535, y = r(0xe08) & 65535
                let endX = min(r(0xe04) >> 16, w), endY = min(r(0xe08) >> 16, h)
                if endX > x && endY > y {
                    let key = color.pixelFormat.rawValue
                    if clearPipelines[key] == nil {
                        let source = """
                        #include <metal_stdlib>
                        using namespace metal;
                        vertex float4 clear_vertex(uint i [[vertex_id]]) {
                            float2 p[3] = {float2(-1,-1),float2(3,-1),float2(-1,3)};
                            return float4(p[i],0,1);
                        }
                        fragment float4 clear_fragment(constant float4& c [[buffer(0)]]) { return c; }
                        """
                        let lib = try device.makeLibrary(source: source, options: nil)
                        let d = MTLRenderPipelineDescriptor()
                        d.vertexFunction = lib.makeFunction(name: "clear_vertex")
                        d.fragmentFunction = lib.makeFunction(name: "clear_fragment")
                        d.colorAttachments[0].pixelFormat = color.pixelFormat
                        clearPipelines[key] = try device.makeRenderPipelineState(descriptor: d)
                    }
                    pass.colorAttachments[0].loadAction = .load
                    guard let encoder = current.makeRenderCommandEncoder(descriptor: pass) else { throw Failure("scissor clear encoder") }
                    encoderCount += 1
                    encoder.setRenderPipelineState(clearPipelines[key]!)
                    encoder.setScissorRect(MTLScissorRect(x:x,y:y,width:endX-x,height:endY-y))
                    var rgba = SIMD4<Float>(Float(f(0xd80)),Float(f(0xd84)),Float(f(0xd88)),Float(f(0xd8c)))
                    encoder.setFragmentBytes(&rgba,length:16,index:0)
                    encoder.drawPrimitives(type:.triangle,vertexStart:0,vertexCount:3)
                    encoder.endEncoding()
                }
            } else {
                guard let encoder = current.makeRenderCommandEncoder(descriptor:pass) else { throw Failure("clear encoder") }
                encoderCount += 1; encoder.endEncoding()
            }
            clears += 1
        } else {
            let pk: String
            if let merged {
                if let cached = mergedKeys[passStateIndex] { pk = cached }
                else { pk = text(command.pipeline) + merged.suffix(command); mergedKeys[passStateIndex] = pk }
            } else { pk = text(command.pipeline) }
            if pipelines[pk] == nil {
                let descriptor = MTLRenderPipelineDescriptor()
                for slot in colors.indices { descriptor.colorAttachments[slot].pixelFormat = colors[slot].pixelFormat }
                descriptor.depthAttachmentPixelFormat = depthFormat
                if depthFormat == .depth32Float_stencil8 { descriptor.stencilAttachmentPixelFormat = depthFormat }
                try need(r(0x12e4)<=1,"invalid per-target blend enable")
                let outputSlots = merged?.mapping(command) ?? Array(originalAttachments.indices)
                for slot in colors.indices { descriptor.colorAttachments[slot].writeMask = [] }
                for (originalSlot, slot) in outputSlots.enumerated() {
                    let physical = (r(0x121c) >> (4 + originalSlot * 3)) & 7
                    let blend = descriptor.colorAttachments[slot]!
                    blend.writeMask = .all
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
                let vertexSource = String(decoding: try file(text(command.vertex)), as: UTF8.self)
                var fragmentSource = String(decoding: try file(text(command.fragment)), as: UTF8.self)
                if let merged { fragmentSource = merged.fragment(fragmentSource, command) }
                guard let ready = try compiler.request(pk, descriptor, vertexSource, fragmentSource) else {
                    skippedDraws += max(1, Int(input.repeats)); continue
                }
                pipelines[pk] = ready.pipeline
                renderFunctions[pk] = (ready.vertex, ready.fragment)
            } else {pipelineHits += 1}
            if pipelineUsage[passStateIndex] == nil { pipelineUsage[passStateIndex] = cacheStamp("p", pk) }
            use(pipelineUsage[passStateIndex]!)
            let (vs, fs) = renderFunctions[pk]!
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
            if boundPipeline != pk {
                encoder.setRenderPipelineState(pipelines[pk]!);encoder.setDepthStencilState(depthStates[pk]!);boundPipeline=pk;stateBinds += 2
            }
            let transition = StateTransition(previous: boundCommand, current: commandIndex)
            let changedGroups: UInt8
            if boundCommand < 0 { changedGroups = 63 }
            else if let cached = groupChanges[transition] { changedGroups = cached }
            else {
                var mask: UInt8 = 0
                for (group, offsets) in registerGroups.enumerated() {
                    if offsets.contains(where: { boundRegisters[$0] != r($0) }) { mask |= 1 << group }
                }
                groupChanges[transition] = mask;changedGroups = mask
            }
            if changedGroups & 1 != 0 {
            var viewports = [MTLViewport]()
            for index in 0..<16 {
                let base = 0xa00 + index*32
                let sx=f(base),sy=f(base+4),sz=f(base+8),ox=f(base+12),oy=f(base+16),oz=f(base+20)
                let near = r(0xd7c)==0 ? oz-sz : oz, far=oz+sz
                try need([sx,sy,sz,ox,oy,oz].allSatisfy{$0.isFinite} && near>=0 && near<=1 && far>=0 && far<=1,"invalid viewport/depth range")
                viewports.append(MTLViewport(originX:ox-abs(sx),originY:oy-abs(sy),width:2*abs(sx),height:2*abs(sy),znear:near,zfar:far))
            }
            encoder.setViewports(viewports)
            }
            if changedGroups & 2 != 0 {
            let offset = r(0xdac)==0x1b01 ? r(0xdc4) : r(0xdc8)
            try need([r(0xdc0),r(0xdc4),r(0xdc8)].allSatisfy{$0<=1},"invalid polygon offset enable")
            if offset != 0 {
                try need(r(0x1110)==1,"depth bias requires format-dependent units")
                encoder.setDepthBias(Float(f(0x15bc)),slopeScale:Float(f(0x156c)),clamp:Float(f(0x187c)))
            } else {encoder.setDepthBias(0,slopeScale:0,clamp:0)}
            }
            if changedGroups & 4 != 0 {
            var scissors = [MTLScissorRect]()
            for index in 0..<16 {
                let base = 0xe00 + index*16
                if r(base) != 0 {
                    let x=r(base+4)&65535,y=r(base+8)&65535,endX=r(base+4)>>16,endY=r(base+8)>>16
                    try need(x<=endX && y<=endY && endX<=w && endY<=h,"invalid scissor")
                    scissors.append(MTLScissorRect(x:x,y:y,width:endX-x,height:endY-y))
                } else {scissors.append(MTLScissorRect(x:0,y:0,width:w,height:h))}
            }
            encoder.setScissorRects(scissors)
            }
            if changedGroups & 8 != 0 {
            if r(0x1918) != 0 {
                switch r(0x1920) {case 0x404:encoder.setCullMode(.front);case 0x405:encoder.setCullMode(.back);default:throw Failure("cull face unsupported")}
            } else {encoder.setCullMode(.none)}
            switch r(0x191c) {case 0,0x900:encoder.setFrontFacing(.clockwise);case 0x901:encoder.setFrontFacing(.counterClockwise);default:throw Failure("front winding")}
            try need(r(0xdac)==r(0xdb0) && [0,0x1b01,0x1b02].contains(r(0xdac)),"polygon mode unsupported")
            encoder.setTriangleFillMode(r(0xdac)==0x1b01 ? .lines : .fill)
            }
            if changedGroups & 16 != 0 {
            encoder.setBlendColor(red:Float(f(0x131c)),green:Float(f(0x1320)),blue:Float(f(0x1324)),alpha:Float(f(0x1328)))
            }
            if changedGroups & 32 != 0 {
            encoder.setStencilReferenceValues(front:UInt32(r(0x1394)),back:UInt32(r(r(0x1594)==1 ? 0xf54 : 0x1394)))
            }
            boundRegisters = raw;boundCommand = commandIndex
            if vertexBindings[commandIndex] == nil {
                vertexBindings[commandIndex] = try items(command.buffers).map { b in
                    let slot = Int(b.slot)
                    let a = items(command.attributes).first { Int($0.stream) == slot }!
                    let offset = a.instanced == 1 ? (Int(command.instance) / Int(a.rate)) * Int(a.stride) : 0
                    return (slot, try buffer(text(b.file)), offset)
                }
            }
            for (slot, resource, offset) in vertexBindings[commandIndex]! { bind(encoder, 0, resource, offset, slot) }
            for c in items(command.constants) {
                guard let b=uploadRing[uploadSlot] else {throw Failure("missing constant upload")}
                let offset = (input.kind == 5 || input.kind == 6) ? uniformOffset(input, Int(c.stage), Int(c.slot)) : Int(c.offset)
                bind(encoder, Int(c.stage), b, offset, Int(c.slot))
            }
            for stage in 0...1 {
                if let table = resolvedTables[commandIndex]?[stage] {
                    bindTable(encoder, stage, table); textureHits += table.textures.count
                    continue
                }
                let stageItems=items(command.textures).filter{Int($0.stage)==stage}
                if stageItems.isEmpty {continue}
                let tableKey = pk + ":" + String(stage) + ":" + stageItems.map { item in
                    "\(text(item.key)):\(item.image):\(item.sampler):\(item.count):\(item.u):\(item.v):\(item.min):\(item.mag):\(item.anisotropy)"
                }.joined(separator: "|")
                touch("a", tableKey)
                if let resolved = argumentTables[tableKey] {
                    resolvedTables[commandIndex, default: [:]][stage] = resolved
                    bindTable(encoder, stage, resolved); textureHits += resolved.textures.count
                    continue
                }
                var tableTextures=[MTLTexture](), tableSamplers=[MTLSamplerState]()
                let argument=(stage==0 ? vs : fs).makeArgumentEncoder(bufferIndex:30)
                try need(device.argumentBuffersSupport == .tier2, "persistent argument heaps require tier 2")
                let heapKey = pk + ":" + String(stage)
                let stride = ((argument.encodedLength + argument.alignment - 1) / argument.alignment) * argument.alignment
                var heap = argumentHeaps[heapKey]
                if heap == nil || heap!.1 + stride > heap!.0.length {
                    guard let storage = device.makeBuffer(length: max(65536, stride * 64), options: .storageModeShared) else { throw Failure("argument heap allocation") }
                    heap = (storage, 0)
                }
                let table = heap!.0, tableOffset = heap!.1
                argumentHeaps[heapKey] = (table, tableOffset + stride)
                argument.setArgumentBuffer(table, offset: tableOffset)
                for item in stageItems {
                    if item.image >= 0 {
                        let tk=text(item.key)
                        touch("t", tk)
                        if textureCache[tk]==nil {
                            let textureUpload = NativeSpan("texture_upload")
                            defer { textureUpload.end() }
                            let tw=Int(item.width),th=Int(item.height),layers=Int(item.layers)
                            let d=MTLTextureDescriptor.texture2DDescriptor(pixelFormat:MTLPixelFormat(rawValue:UInt(Int(item.metal)))!,width:tw,height:th,mipmapped:false)
                            d.textureType = item.array == 1 ? .type2DArray : .type2D
                            d.arrayLength=layers
                            d.storageMode = .shared;d.usage = [.shaderRead,.pixelFormatView]
                            guard let t=device.makeTexture(descriptor:d) else {throw Failure("sample texture allocation (BC1 device support required)")}
                            let bytes=try file(text(item.file));let row=Int(item.row)
                            try need(bytes.count == row*Int(item.rows)*layers,"sample texture size")
                            bytes.withUnsafeBytes{p in
                                for layer in 0..<layers {
                                    t.replace(region:MTLRegionMake2D(0,0,tw,th),mipmapLevel:0,slice:layer,withBytes:p.baseAddress!.advanced(by:layer*row*Int(item.rows)),bytesPerRow:row,bytesPerImage:row*Int(item.rows))
                                }
                            }
                            let components = [item.swizzle0,item.swizzle1,item.swizzle2,item.swizzle3].map { MTLTextureSwizzle(rawValue:UInt8($0))! }
                            let swizzle = MTLTextureSwizzleChannels(red:components[0],green:components[1],blue:components[2],alpha:components[3])
                            guard let view=t.makeTextureView(pixelFormat:t.pixelFormat,textureType:d.textureType,levels:0..<1,slices:0..<layers,swizzle:swizzle) else {throw Failure("texture view")}
                            textureCache[tk]=view
                        }else{textureHits += 1}
                        let t=textureCache[tk]!;tableTextures.append(t);argument.setTexture(t,index:Int(item.image))
                    }else{
                        let d=MTLSamplerDescriptor();let modes:[MTLSamplerAddressMode]=[.repeat,.mirrorRepeat,.clampToEdge]
                        d.sAddressMode=modes[Int(item.u)];d.tAddressMode=modes[Int(item.v)];d.normalizedCoordinates=true
                        d.minFilter=Int(item.min)==1 ? .nearest : .linear;d.magFilter=Int(item.mag)==1 ? .nearest : .linear;d.mipFilter = .notMipmapped;d.supportArgumentBuffers=true
                        try need((1...16).contains(Int(item.anisotropy)), "sampler anisotropy range")
                        d.maxAnisotropy=Int(item.anisotropy)
                        guard let sampler=device.makeSamplerState(descriptor:d) else {throw Failure("sampler allocation")};tableSamplers.append(sampler);argument.setSamplerState(sampler,index:9*Int(item.count)+Int(item.sampler))
                    }
                }
                let resolved = ArgumentTable(key: tableKey, buffer: table, offset: tableOffset, textures: tableTextures, usage: [cacheStamp("a", tableKey)] + stageItems.filter { $0.image >= 0 }.map { cacheStamp("t", text($0.key)) }, samplers: tableSamplers)
                argumentTables[tableKey] = resolved
                resolvedTables[commandIndex, default: [:]][stage] = resolved
                bindTable(encoder, stage, resolved)
            }
            let topology:MTLPrimitiveType
            switch Int(command.topology) {case 0:topology = .point;case 1:topology = .line;case 3:topology = .lineStrip;case 4:topology = .triangle;case 5:topology = .triangleStrip;default:throw Failure("primitive")}
            let repeats = max(1, Int(input.repeats))
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
    return "merged_depth_draws=\(depthPlans.count) skipped_draws=\(skippedDraws) pass_hits=\(passHits) encoding_seconds=\(elapsed-gpuWait) gpu_wait_seconds=\(gpuWait) state_binds=\(stateBinds) offset_writes=\(offsetWrites) encoders=\(encoderCount) command_buffers=\(bufferCount) depth_hits=\(depthHits) parse_seconds=\(started-parseStarted) seconds=\(elapsed) draws_per_second=\(Double(draws)/max(elapsed,0.000001)) device=\(device.name) dispatches=\(dispatches) draws=\(draws) clears=\(clears) pipelines=\(pipelines.count) pipeline_hits=\(pipelineHits) textures=\(textureCache.count) texture_hits=\(textureHits)"
}
    func readback(_ directory: String, receive: ((String, Data) -> Void)? = nil) throws -> String {
        let readback = NativeSpan("native_readback")
        defer { readback.end() }
        let root = URL(fileURLWithPath:directory)
        func output(_ name: String, _ bytes: Data) throws {
            if let receive { receive(name, bytes) }
            else { try bytes.write(to: root.appendingPathComponent(name)) }
        }
        if root != spillRoot {
            for key in spilledColors.keys where targets[key] == nil {
                try output(key + ".rgba", Data(contentsOf: spillRoot.appendingPathComponent(key + ".rgba")))
            }
            for key in spilledDepths.keys where depths[key] == nil {
                for suffix in [".depth", ".stencil"] {
                    try output(key + suffix, Data(contentsOf: spillRoot.appendingPathComponent(key + suffix)))
                }
            }
        }
    for (key,t) in targets where dirtyAttachments.contains(key) {
        var bytes=Data(count:t.width*t.height*targetBytes[key]!)
        bytes.withUnsafeMutableBytes { storage in
            t.getBytes(storage.baseAddress!,bytesPerRow:t.width*targetBytes[key]!,from:MTLRegionMake2D(0,0,t.width,t.height),mipmapLevel:0)
        }
        try output(key + ".rgba", bytes)
    }
        if depths.keys.contains(where: { dirtyAttachments.contains($0) }) {
            guard let cb=queue.makeCommandBuffer(),let blit=cb.makeBlitCommandEncoder() else {throw Failure("depth readback encoder")}
            var planes=[(String,MTLBuffer,Int,Int,Int)]()
            for (key,t) in depths where dirtyAttachments.contains(key) {
                for stencil in [false,true] {
                    if stencil && t.pixelFormat != .depth32Float_stencil8 {try output(key + ".stencil", Data());continue}
                    let bpp=stencil ? 1 : (t.pixelFormat == .depth16Unorm ? 2 : 4), row=((t.width*bpp+255)/256)*256
                    let buffer: MTLBuffer
                    if let index=readbackBuffers.firstIndex(where: { $0.length >= row*t.height }) {
                        buffer=readbackBuffers.remove(at:index)
                    } else {
                        guard let allocation=device.makeBuffer(length:row*t.height,options:.storageModeShared) else {throw Failure("depth readback buffer")}
                        buffer=allocation
                    }
                    let options:MTLBlitOption=t.pixelFormat == .depth32Float_stencil8 ? (stencil ? .stencilFromDepthStencil : .depthFromDepthStencil) : []
                    blit.copy(from:t,sourceSlice:0,sourceLevel:0,sourceOrigin:MTLOrigin(x:0,y:0,z:0),sourceSize:MTLSize(width:t.width,height:t.height,depth:1),to:buffer,destinationOffset:0,destinationBytesPerRow:row,destinationBytesPerImage:row*t.height,options:options)
                    planes.append((key+(stencil ? ".stencil" : ".depth"),buffer,row,t.width*bpp,t.height))
                }
            }
            blit.endEncoding();cb.commit();cb.waitUntilCompleted();try need(cb.status == .completed,"depth readback failed")
            for (name,buffer,row,width,height) in planes {
                var bytes=Data(count:width*height)
                bytes.withUnsafeMutableBytes { storage in
                    for y in 0..<height {
                        storage.baseAddress!.advanced(by:y*width).copyMemory(from:buffer.contents().advanced(by:y*row),byteCount:width)
                    }
                }
                try output(name, bytes)
                // Completion above and the owned Data copy make the scratch buffer reusable.
                let budget=min(32*1024*1024,pressure.budget(cacheBudget)/8)
                if readbackBuffers.reduce(0, { $0+$1.length })+buffer.length <= budget {
                    readbackBuffers.append(buffer)
                }
            }
        }
        for (key,value) in queryValues {
            var little=value.littleEndian
            try withUnsafeBytes(of:&little) {try output(key + ".query", Data($0))}
        }
        for (key,buffer) in computeBuffers {
            try output(key + ".buffer", Data(bytes:buffer.contents(),count:buffer.length))
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
        do {
            let worker = Unmanaged<Worker>.fromOpaque(pointer).takeUnretainedValue()
            let result = try autoreleasepool { try worker.runSubmission(UnsafeBufferPointer(start: commands, count: count), UnsafeBufferPointer(start: resources, count: resourceCount), constants.pointee) }
            return strdup("OK " + result + " " + (try worker.trimCaches()))
        }
        catch { return strdup("ERROR \(error)") }
    }
}
@_cdecl("soju_readback")
public func sojuReadback(_ pointer: UnsafeMutableRawPointer, _ directory: UnsafePointer<CChar>, _ context: UnsafeMutableRawPointer, _ receive: @convention(c) (UnsafeMutableRawPointer, UnsafePointer<CChar>, UnsafePointer<UInt8>?, Int) -> Void) -> UnsafeMutablePointer<CChar>? {
    autoreleasepool {
        do {
            let worker = Unmanaged<Worker>.fromOpaque(pointer).takeUnretainedValue()
            let result = try autoreleasepool {
                try worker.readback(String(cString: directory)) { name, data in
                    name.withCString { key in
                        data.withUnsafeBytes { bytes in
                            receive(context, key, bytes.baseAddress?.assumingMemoryBound(to: UInt8.self), bytes.count)
                        }
                    }
                }
            }
            worker.finishReadback()
            if NativeSpan.enabled { FileHandle.standardError.write(Data(("profile-native|" + NativeSpan.report() + "\n").utf8)) }
            return strdup("OK " + result + " " + (try worker.trimCaches()))
        }
        catch { return strdup("ERROR \(error)") }
    }
}

@_cdecl("soju_layout")
public func sojuLayout(_ index: Int) -> Int {
    let sizes = [MemoryLayout<Command>.stride, MemoryLayout<SpanByte>.stride,
                 MemoryLayout<Reg>.stride, MemoryLayout<TargetCommand>.stride,
                 MemoryLayout<DepthCommand>.stride, MemoryLayout<ConstantCommand>.stride,
                 MemoryLayout<AttributeCommand>.stride, MemoryLayout<BufferCommand>.stride,
                 MemoryLayout<SegmentCommand>.stride, MemoryLayout<TextureCommand>.stride]
    return sizes.indices.contains(index) ? sizes[index] : -1
}
