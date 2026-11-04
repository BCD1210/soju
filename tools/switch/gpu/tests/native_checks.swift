@main struct NativeChecks {
    static func main() throws {
        guard let device = MTLCreateSystemDefaultDevice() else { throw Failure("no Metal device") }
        let compiler = RenderCompiler(device)
        let vertex = "#include <metal_stdlib>\nusing namespace metal; vertex float4 soju_vertex(uint i [[vertex_id]]) { return float4(float(i),0,0,1); }"
        let fragment = "#include <metal_stdlib>\nusing namespace metal; fragment float4 soju_fragment() { return float4(1); }"
        func descriptor() -> MTLRenderPipelineDescriptor {
            let d = MTLRenderPipelineDescriptor(); d.colorAttachments[0].pixelFormat = .rgba8Unorm; return d
        }
        _ = try compiler.compile("corruption", descriptor(), vertex, fragment)
        let files = try FileManager.default.contentsOfDirectory(at: compiler.root, includingPropertiesForKeys: nil).filter { $0.pathExtension == "archive" }
        try need(files.count == 1, "archive not published")
        let archive = files[0]
        var bytes = try Data(contentsOf: archive)
        bytes[bytes.count - 1] ^= 255
        try bytes.write(to: archive, options: .atomic)
        _ = try compiler.compile("corruption", descriptor(), vertex, fragment)
        bytes = try Data(contentsOf: archive)
        try need(String(decoding: bytes.prefix(64), as: UTF8.self) == compiler.digest(Data(bytes.dropFirst(64))), "corrupt archive not replaced")
        try Data([1,2,3]).write(to: archive, options: .atomic)
        _ = try compiler.compile("corruption", descriptor(), vertex, fragment)
        try need((try Data(contentsOf: archive)).count > 64, "truncated archive not repaired")
        let invalid = Data(repeating: 127, count: 512)
        var envelope = Data(compiler.digest(invalid).utf8); envelope.append(invalid)
        try envelope.write(to: archive, options: .atomic)
        _ = try compiler.compile("corruption", descriptor(), vertex, fragment)
        try need((try Data(contentsOf: archive)).count > envelope.count, "invalid Metal archive not repaired")
        for i in 0..<40 { _ = try compiler.compile("disk-\(i)", descriptor(), vertex, fragment) }
        let archives = try FileManager.default.contentsOfDirectory(at: compiler.root, includingPropertiesForKeys: [.fileSizeKey]).filter { $0.pathExtension == "archive" }
        let size = try archives.reduce(0) { try $0 + $1.resourceValues(forKeys: [.fileSizeKey]).fileSize! }
        try need(size <= compiler.diskLimit, "disk budget exceeded")
        setenv("SOJU_GPU_DISK_CACHE_BYTES", "1", 1)
        let tiny = RenderCompiler(device)
        _ = try tiny.compile("too-large-to-store", descriptor(), vertex, fragment)
        try need((try FileManager.default.contentsOfDirectory(at: tiny.root, includingPropertiesForKeys: nil)).filter { $0.pathExtension == "archive" }.isEmpty, "lowered disk budget retained oversized archives")
        setenv("SOJU_GPU_DISK_CACHE_BYTES", "65536", 1)
        // Queue deduplication and saturation must recover after completed results
        // belong to variants which are never drawn again.
        for i in 0..<80 { _ = try compiler.request("async-\(i)", descriptor(), vertex, fragment) }
        compiler.queue.sync {}
        try need(compiler.completed.count <= 32 && compiler.pending.isEmpty, "compiler queue not bounded")
        for i in 80..<160 {
            _ = try compiler.request("async-\(i)", descriptor(), vertex, fragment)
            if i % 2 == 1 { compiler.queue.sync {} }
        }
        compiler.queue.sync {}
        try need(compiler.completed.count <= 32, "completed queue exceeded cap during interleaved publication")
        _ = try compiler.request("after-saturation", descriptor(), vertex, fragment)
        compiler.queue.sync {}
        try need(try compiler.request("after-saturation", descriptor(), vertex, fragment) != nil, "compiler starved after saturation")
        _ = try compiler.request("broken", descriptor(), "#error deliberate", fragment)
        compiler.queue.sync {}
        var rejected = false
        do { _ = try compiler.request("broken", descriptor(), "#error deliberate", fragment) } catch { rejected = true }
        try need(rejected, "compiler failure did not surface")

        let td = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba8Unorm, width: 64, height: 64, mipmapped: false)
        td.storageMode = .shared; td.usage = .shaderRead
        let a = device.makeTexture(descriptor: td)!, b = device.makeTexture(descriptor: td)!, c = device.makeTexture(descriptor: td)!
        setenv("SOJU_GPU_CACHE_BYTES", String(a.allocatedSize * 2), 1)
        let worker = try Worker(device: device)
        for (key, texture) in [("a", a), ("b", b), ("c", c)] { worker.textureCache[key] = texture; worker.touch("t", key) }
        worker.touch("t", "a")
        _ = try worker.trimCaches()
        try need(Set(worker.textureCache.keys) == Set(["a", "c"]), "native LRU evicted the recently used resource")
        worker.pressure.lock.lock(); worker.pressure.pressured = true; worker.pressure.lock.unlock()
        _ = try worker.trimCaches()
        try need(worker.cacheBytes() <= a.allocatedSize, "pressure did not halve budget")
        setenv("SOJU_GPU_CACHE_BYTES", "2048", 1)
        let passes = try Worker(device: device)
        for key in ["a", "b", "c"] { passes.renderPasses[key] = MTLRenderPassDescriptor(); passes.touch("r", key) }
        passes.touch("r", "a")
        _ = try passes.trimCaches()
        try need(Set(passes.renderPasses.keys) == Set(["a", "c"]), "render-pass metadata escaped LRU budget")
        passes.invalidatePasses()
        try need(passes.lastUse.isEmpty && passes.cacheBytes() == 0, "invalidated passes retained metadata")
        passes.queryValues["old-query"] = 7
        passes.computeBuffers["old-buffer"] = device.makeBuffer(length: 4096, options: .storageModeShared)!
        passes.computeGenerations["old-buffer"] = 1
        passes.finishReadback()
        try need(passes.queryValues.isEmpty && passes.computeBuffers.isEmpty && passes.computeGenerations.isEmpty, "readback retained delivered resources")
        setenv("SOJU_GPU_CACHE_BYTES", "67108864", 1)
        let staging = try Worker(device: device)
        let depthDescriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .depth32Float, width: 17, height: 9, mipmapped: false)
        depthDescriptor.storageMode = .private; depthDescriptor.usage = .renderTarget
        let depth = device.makeTexture(descriptor: depthDescriptor)!
        staging.depths["probe"] = depth; staging.dirtyAttachments.insert("probe")
        func captureDepth(_ value: Double) throws -> Data {
            let pass = MTLRenderPassDescriptor()
            pass.depthAttachment.texture = depth; pass.depthAttachment.loadAction = .clear
            pass.depthAttachment.storeAction = .store; pass.depthAttachment.clearDepth = value
            let command = staging.queue.makeCommandBuffer()!
            command.makeRenderCommandEncoder(descriptor: pass)!.endEncoding()
            command.commit(); command.waitUntilCompleted()
            var result = Data()
            _ = try staging.readback(staging.spillRoot.path) { name, bytes in
                if name == "probe.depth" { result = bytes }
            }
            try need(result.count == 17*9*4, "padded depth rows escaped readback")
            try result.withUnsafeBytes { bytes in
                for offset in stride(from: 0, to: bytes.count, by: 4) {
                    try need(bytes.loadUnaligned(fromByteOffset: offset, as: Float.self) == Float(value), "depth capture contents")
                }
            }
            return result
        }
        let first = try captureDepth(0.25)
        try need(staging.readbackBuffers.count == 1, "missing reusable depth scratch")
        let identity = ObjectIdentifier(staging.readbackBuffers[0])
        let second = try captureDepth(0.75)
        try need(ObjectIdentifier(staging.readbackBuffers[0]) == identity, "depth scratch was reallocated")
        try need(first != second && first.withUnsafeBytes { $0.loadUnaligned(as: Float.self) } == 0.25, "capture aliases reused scratch")
        staging.readbackBuffers.append(device.makeBuffer(length: 67108864, options: .storageModeShared)!)
        _ = try staging.trimCaches()
        try need(staging.readbackBuffers.isEmpty, "scratch escaped cache budget")
        try plannerChecks()
        print("native_checks archive_corruption=passed archive_truncation=passed disk_bytes=\(size) async_saturation=passed compile_error=passed lru=passed pressure=passed")
    }
}

final class PlannerFixture {
    var cleanup = [() -> Void]()
    deinit { for free in cleanup { free() } }
    func bytes(_ string: String) -> SpanByte {
        let data = Array(string.utf8), p = UnsafeMutablePointer<UInt8>.allocate(capacity: string.utf8.count)
        data.withUnsafeBufferPointer { p.initialize(from: $0.baseAddress!, count: $0.count) }
        cleanup.append { p.deallocate() }; return SpanByte(pointer: p, length: data.count, capacity: data.count)
    }
    func command(_ names: [String], depth: Bool = false, generation: Int64 = 1, depthGeneration: Int64 = 1, query: Bool = false, width: Int64 = 32) -> Command {
        var command = Command()
        let targets = UnsafeMutablePointer<TargetCommand>.allocate(capacity: names.count)
        for (index, name) in names.enumerated() {
            var target = TargetCommand(); target.width = width; target.height = 32; target.generation = generation; target.metal = 70; target.key = bytes(name)
            targets.advanced(by: index).initialize(to: target)
        }
        cleanup.append { targets.deallocate() }; command.targets = SpanTargetCommand(pointer: targets, length: names.count, capacity: names.count)
        if depth {
            var d = DepthCommand(); d.key = bytes("shared-depth"); d.generation = depthGeneration; d.format = 10
            let p = UnsafeMutablePointer<DepthCommand>.allocate(capacity: 1); p.initialize(to: d)
            cleanup.append { p.deallocate() }; command.depth = SpanDepthCommand(pointer: p, length: 1, capacity: 1)
        }
        if query {
            let p = UnsafeMutablePointer<Reg>.allocate(capacity: 1); p.initialize(to: Reg(address: 0x1514, value: 1))
            cleanup.append { p.deallocate() }; command.registers = SpanReg(pointer: p, length: 1, capacity: 1)
        }
        return command
    }
}
func plannerChecks() throws {
    let f = PlannerFixture(), worker = try Worker()
    func independent(_ commands: [Command]) throws -> [[Int]]? { try commands.withUnsafeBufferPointer { try worker.independentGroups($0) } }
    func merged(_ commands: [Command]) -> [Int: SharedDepthPlan] { commands.withUnsafeBufferPointer { sharedDepthPlans($0, Array(commands.indices)) } }
    let a = f.command(["a"]), b = f.command(["b"])
    try need(try independent([a,b]) == [[0],[1]], "independent colors rejected")
    try need(try independent([f.command(["a","b"]), f.command(["b","c"])]) == nil, "overlapping color sets parallelized")
    try need(try independent([a,f.command(["a"], generation: 2)]) == nil, "generation boundary parallelized")
    var clear = a; clear.kind = 1
    try need(try independent([a,clear,b]) == nil, "clear boundary parallelized")
    try need(try independent([a,f.command(["b"], query: true)]) == nil, "query boundary parallelized")
    try need(try independent([a,f.command(["b"], depth: true)]) == nil, "shared depth parallelized")
    var reference = Command(); reference.kind = 5; reference.reference = 0
    try need(try independent([a,b,reference]) == [[0,2],[1]], "reference lost its target ordering")
    let da = f.command(["a"], depth: true), db = f.command(["b"], depth: true)
    try need(merged([da,db,da]).count == 3, "shared depth not merged")
    try need(merged([da,clear,db]).isEmpty, "clear crossed by depth merge")
    try need(merged([da,f.command(["b"], depth: true, query: true),db]).isEmpty, "query crossed by depth merge")
    try need(merged([da,f.command(["b"], depth: true, depthGeneration: 2)]).isEmpty, "depth generation crossed by merge")
    try need(merged([da,f.command(["b"], depth: true, width: 64)]).isEmpty, "dimension boundary crossed by merge")
    try need(merged([da,db,f.command(["a"], depth: true, generation: 2)]).count == 2, "color generation crossed by merge")
    let nine = (0..<9).map { f.command(["target-\($0)"], depth: true) }
    let plan = merged(nine)
    try need(plan.count == 8 && plan[8] == nil && plan[0]!.attachments.count == 8, "attachment limit not respected")
    print("planner_checks overlap=passed generations=passed clear_query=passed depth=passed references=passed mrt_limit=passed")
}
