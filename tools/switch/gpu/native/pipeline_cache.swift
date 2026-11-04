import CryptoKit

struct CompiledRender {
    let pipeline: MTLRenderPipelineState
    let vertex: MTLFunction
    let fragment: MTLFunction
}

// Only this queue touches disk and compiler state. Published results are immutable.
final class RenderCompiler {
    let device: MTLDevice
    let queue = DispatchQueue(label: "soju.metal.compile", qos: .utility)
    let lock = NSLock()
    var pending = Set<String>()
    var completed = [String: Result<CompiledRender, Error>]()
    var completionOrder = [String]()
    let asynchronous = ProcessInfo.processInfo.environment["SOJU_GPU_ASYNC_PIPELINES"] == "1"
    let diskLimit = max(0, Int(ProcessInfo.processInfo.environment["SOJU_GPU_DISK_CACHE_BYTES"] ?? "") ?? 134217728)
    let root: URL
    init(_ device: MTLDevice) {
        self.device = device
        root = URL(fileURLWithPath: ProcessInfo.processInfo.environment["SOJU_GPU_CACHE_DIR"] ?? NSHomeDirectory() + "/Library/Caches/Soju/Metal/v1", isDirectory: true)
    }
    func digest(_ bytes: Data) -> String { SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined() }
    func compile(_ key: String, _ descriptor: MTLRenderPipelineDescriptor, _ vertex: String, _ fragment: String) throws -> CompiledRender {
        let compilation = NativeSpan("shader_compile")
        defer { compilation.end() }
        let options = MTLCompileOptions(); options.mathMode = .safe
        let vl = try device.makeLibrary(source: vertex, options: options)
        let fl = try device.makeLibrary(source: fragment, options: options)
        guard let vs = vl.makeFunction(name: "soju_vertex"), let fs = fl.makeFunction(name: "soju_fragment") else { throw Failure("render entry missing") }
        compilation.end()
        descriptor.vertexFunction = vs; descriptor.fragmentFunction = fs
        // The stable guest key plus exact translated sources, options, OS and GPU
        // names scope archives across translator/compiler/device changes.
        let identity = digest(Data("v1\0safe\0\(device.registryID)\0\(ProcessInfo.processInfo.operatingSystemVersionString)\0\(key)\0\(vertex)\0\(fragment)".utf8))
        let destination = root.appendingPathComponent(identity + ".archive")
        let temporary = root.appendingPathComponent(UUID().uuidString + ".tmp")
        defer { try? FileManager.default.removeItem(at: temporary) }
        var archive: MTLBinaryArchive?
        if diskLimit > 0 {
            try? FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            let ad = MTLBinaryArchiveDescriptor()
            if let size = try? destination.resourceValues(forKeys: [.fileSizeKey]).fileSize,
               size <= diskLimit, let bytes = try? Data(contentsOf: destination), bytes.count > 64 {
                let payload = Data(bytes.dropFirst(64))
                if String(decoding: bytes.prefix(64), as: UTF8.self) == digest(payload),
                   (try? payload.write(to: temporary, options: .atomic)) != nil { ad.url = temporary }
            }
            archive = try? device.makeBinaryArchive(descriptor: ad)
            if archive == nil { archive = try? device.makeBinaryArchive(descriptor: MTLBinaryArchiveDescriptor()) }
            if let archive { descriptor.binaryArchives = [archive] }
        }
        let creation = NativeSpan("pipeline_creation")
        defer { creation.end() }
        let pipeline: MTLRenderPipelineState
        do { pipeline = try device.makeRenderPipelineState(descriptor: descriptor) }
        catch {
            guard archive != nil else { throw error }
            descriptor.binaryArchives = []
            pipeline = try device.makeRenderPipelineState(descriptor: descriptor)
            archive = try? device.makeBinaryArchive(descriptor: MTLBinaryArchiveDescriptor())
        }
        if let archive {
            do {
                try archive.addRenderPipelineFunctions(descriptor: descriptor)
                try archive.serialize(to: temporary)
                let payload = try Data(contentsOf: temporary)
                var bytes = Data(digest(payload).utf8); bytes.append(payload)
                if bytes.count <= diskLimit {
                    try bytes.write(to: destination, options: .atomic)
                }
            } catch { /* Disk caching is optional; compiled pipelines remain usable. */ }
        }
        if diskLimit > 0 { prune() }
        return CompiledRender(pipeline: pipeline, vertex: vs, fragment: fs)
    }
    func prune() {
        let fm = FileManager.default
        let files = (try? fm.contentsOfDirectory(at: root, includingPropertiesForKeys: [.fileSizeKey, .contentModificationDateKey])) ?? []
        let entries = files.filter { $0.pathExtension == "archive" }.compactMap { url -> (URL, Int, Date)? in
            guard let v = try? url.resourceValues(forKeys: [.fileSizeKey, .contentModificationDateKey]) else { return nil }
            return (url, v.fileSize ?? 0, v.contentModificationDate ?? .distantPast)
        }.sorted { $0.2 < $1.2 }
        var total = entries.reduce(0) { $0 + $1.1 }
        for (url, size, _) in entries where total > diskLimit {
            if (try? fm.removeItem(at: url)) != nil { total -= size }
        }
    }
    func request(_ key: String, _ descriptor: MTLRenderPipelineDescriptor, _ vertex: String, _ fragment: String) throws -> CompiledRender? {
        if !asynchronous { return try queue.sync { try compile(key, descriptor, vertex, fragment) } }
        lock.lock()
        if let result = completed.removeValue(forKey: key) { completionOrder.removeAll { $0 == key }; lock.unlock(); return try result.get() }
        if pending.contains(key) || pending.count >= 32 { lock.unlock(); return nil }
        if completed.count >= 32 { completed.removeValue(forKey: completionOrder.removeFirst()) }
        pending.insert(key); lock.unlock()
        // descriptor and source strings are owned copies; no Command/Span survives run.
        queue.async { [self] in
            let result = autoreleasepool { Result { try compile(key, descriptor, vertex, fragment) } }
            lock.lock(); pending.remove(key)
            if completed.count >= 32 { completed.removeValue(forKey: completionOrder.removeFirst()) }
            completed[key] = result; completionOrder.append(key); lock.unlock()
        }
        return nil
    }
}
