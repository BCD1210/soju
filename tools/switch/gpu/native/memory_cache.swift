// Eviction runs only after run/readback has released its temporary bindings and
// every submitted command buffer has completed. Dirty attachments are spilled
// before releasing their Metal allocation; their guest generation stays attached.
extension Worker {
    func cacheStamp(_ category: String, _ key: String) -> CacheStamp {
        let id = category + key
        if let stamp = lastUse[id] { return stamp }
        let stamp = CacheStamp(); lastUse[id] = stamp; return stamp
    }
    func use(_ stamp: CacheStamp) { cacheClock &+= 1; stamp.tick = cacheClock }
    func touch(_ category: String, _ key: String) { use(cacheStamp(category, key)) }
    func cacheBytes() -> Int {
        var seen = Set<ObjectIdentifier>(), bytes = 0
        func resource(_ r: MTLResource) {
            if seen.insert(ObjectIdentifier(r)).inserted { bytes += r.allocatedSize }
        }
        for buffer in readbackBuffers { resource(buffer) }
        for t in targets.values { resource(t) }
        for t in depths.values { resource(t) }
        for t in textureCache.values { resource(t.parent ?? t) }
        for t in argumentTables.values {
            resource(t.buffer)
            for texture in t.textures { resource(texture.parent ?? texture) }
        }
        for h in argumentHeaps.values { resource(h.0) }
        // Metal does not expose pipeline/function allocation sizes. Bound their
        // count with a conservative accounting charge, reported separately from
        // device.currentAllocatedSize (which includes driver allocations).
        bytes += (pipelines.count + computePipelines.count) * 65536
        bytes += renderPasses.count * 1024
        return bytes
    }
    func trimCaches() throws -> String {
        let limit = pressure.budget(cacheBudget)
        var evicted = 0
        if cacheBytes() > limit { readbackBuffers.removeAll() }
        if cacheBytes() > limit {
            let candidates = lastUse.sorted { $0.value.tick < $1.value.tick }
            var spilled = false
            for (id, _) in candidates where cacheBytes() > limit {
                guard lastUse[id] != nil else { continue }
                let key = String(id.dropFirst())
                switch id.first {
                case "c", "d":
                    if dirtyAttachments.contains(key) && !spilled {
                        try FileManager.default.createDirectory(at: spillRoot, withIntermediateDirectories: true)
                        _ = try readback(spillRoot.path)
                        for k in dirtyAttachments {
                            if let g = generations[k] { spilledColors[k] = g }
                            if let g = depthGenerations[k] { spilledDepths[k] = g }
                        }
                        spilled = true
                    }
                    invalidatePasses()
                    if id.first == "c" { targets.removeValue(forKey: key); generations.removeValue(forKey: key); targetBytes.removeValue(forKey: key) }
                    else { depths.removeValue(forKey: key); depthGenerations.removeValue(forKey: key) }
                case "t":
                    textureCache.removeValue(forKey: key)
                    // Argument tables retain texture views. Drop dependent tables
                    // and their heap pages together, otherwise eviction frees nothing.
                    argumentTables.removeAll(); argumentHeaps.removeAll()
                    lastUse = lastUse.filter { !$0.key.hasPrefix("a") }
                case "a":
                    argumentTables.removeValue(forKey: key)
                    let used = Set(argumentTables.values.map { ObjectIdentifier($0.buffer) })
                    argumentHeaps = argumentHeaps.filter { used.contains(ObjectIdentifier($0.value.0)) }
                case "r": renderPasses.removeValue(forKey: key)
                case "k": computePipelines.removeValue(forKey: key); functions.removeValue(forKey: key)
                case "p":
                    pipelines.removeValue(forKey: key); renderFunctions.removeValue(forKey: key); depthStates.removeValue(forKey: key)
                default: break
                }
                lastUse.removeValue(forKey: id); evicted += 1
            }
        }
        // Current upload storage is transient, not part of the reusable cache.
        // Release oversized historical ring slots at a completion boundary.
        for i in uploadRing.indices where (uploadRing[i]?.length ?? 0) > max(65536, limit / 8) { uploadRing[i] = nil }
        return "cache_bytes=\(cacheBytes()) cache_budget=\(limit) cache_evictions=\(evicted) metal_bytes=\(device.currentAllocatedSize)"
    }
    func restore(_ key: String, _ generation: Int, _ suffix: String, _ fallback: () throws -> Data) throws -> Data {
        let stored = suffix == ".rgba" ? spilledColors[key] : spilledDepths[key]
        if stored == generation { return try Data(contentsOf: spillRoot.appendingPathComponent(key + suffix)) }
        return try fallback()
    }
    func invalidatePasses() {
        renderPasses.removeAll()
        lastUse = lastUse.filter { !$0.key.hasPrefix("r") }
    }
    func finishReadback() {
        queryValues.removeAll()
        computeBuffers.removeAll(); computeGenerations.removeAll()
        dirtyAttachments.removeAll()
        spilledColors.removeAll(); spilledDepths.removeAll()
        try? FileManager.default.removeItem(at: spillRoot)
    }
}

final class MemoryPressure {
    let lock = NSLock()
    var pressured = false
    let source = DispatchSource.makeMemoryPressureSource(eventMask: [.normal, .warning, .critical], queue: DispatchQueue.global(qos: .utility))
    init() {
        source.setEventHandler { [weak self] in
            guard let self else { return }
            lock.lock(); pressured = source.data.contains(.warning) || source.data.contains(.critical); lock.unlock()
        }
        source.resume()
    }
    deinit { source.cancel() }
    func budget(_ configured: Int) -> Int {
        lock.lock(); defer { lock.unlock() }
        return pressured ? configured / 2 : configured
    }
}

final class CacheStamp { var tick: UInt64 = 0 }
