// Only color-only draw batches with disjoint exact attachment sets qualify.
// Guest texture inputs are immutable captures. Shared/depth/query/compute/clear
// work stays on the ordered path; no inter-queue hazard is inferred away.
extension Worker {
    func runSubmission(_ commands: UnsafeBufferPointer<Command>, _ resources: UnsafeBufferPointer<SpanByte>, _ constants: SpanByte) throws -> String {
        guard ProcessInfo.processInfo.environment["SOJU_GPU_PARALLEL_PASSES"] == "1",
              let groups = try independentGroups(commands), groups.count > 1, groups.count <= 8 else {
            return try run(commands, resources, constants)
        }
        let workers = try groups.map { group -> Worker in
            let child = try Worker(device: device, compiler: compiler)
            let keys = Set(items(commands[group[0]].targets).map { text($0.key) })
            child.targets = targets.filter { keys.contains($0.key) }
            child.generations = generations.filter { keys.contains($0.key) }
            child.targetBytes = targetBytes.filter { keys.contains($0.key) }
            child.pipelines = pipelines; child.renderFunctions = renderFunctions
            child.depthStates = depthStates; child.textureCache = textureCache
            // Mutable heap cursors, uniform uploads and descriptors are not shared.
            return child
        }
        var batches = [[Command]]()
        for group in groups {
            var local = [Int: Int](), batch = [Command]()
            for index in group {
                var command = commands[index]
                if command.kind == 5 {
                    guard let reference = local[Int(command.reference)] else { throw Failure("parallel reference escaped attachment group") }
                    command.reference = Int64(reference)
                }
                local[index] = batch.count; batch.append(command)
            }
            batches.append(batch)
        }
        let results = ParallelResults(workers.count)
        DispatchQueue.concurrentPerform(iterations: workers.count) { index in
            let result = autoreleasepool {
                Result { try batches[index].withUnsafeBufferPointer { try workers[index].run($0, resources, constants) } }
            }
            results.set(index, result)
        }
        // concurrentPerform joins every encoder/GPU completion even on error.
        // Parent state is never mutated concurrently with child cache access.
        let diagnostics = try results.values.map { try $0!.get() }
        for child in workers {
            targets.merge(child.targets) { _, new in new }
            generations.merge(child.generations) { _, new in new }
            targetBytes.merge(child.targetBytes) { _, new in new }
            pipelines.merge(child.pipelines) { _, new in new }
            renderFunctions.merge(child.renderFunctions) { _, new in new }
            depthStates.merge(child.depthStates) { _, new in new }
            textureCache.merge(child.textureCache) { _, new in new }
            dirtyAttachments.formUnion(child.dirtyAttachments)
            for (key, _) in child.lastUse.sorted(by: { $0.value.tick < $1.value.tick }) where !key.hasPrefix("a") { use(cacheStamp(String(key.prefix(1)), String(key.dropFirst()))) }
        }
        let names = ["skipped_draws", "draws", "encoders", "command_buffers", "pipeline_hits", "depth_hits", "texture_hits"]
        let totals = names.map { name -> String in
            let sum = diagnostics.reduce(0) { value, diagnostic in
                value + (diagnostic.split(separator: " ").first { $0.hasPrefix(name + "=") }.flatMap { Int($0.dropFirst(name.count + 1)) } ?? 0)
            }
            return "\(name)=\(sum)"
        }.joined(separator: " ")
        return "parallel_passes=\(groups.count) " + totals
    }
    func independentGroups(_ commands: UnsafeBufferPointer<Command>) throws -> [[Int]]? {
        var groups = [[Int]](), ids = [String: Int](), owners = [String: String](), generations = [String: Int64]()
        for (index, input) in commands.enumerated() {
            let source = input.kind == 5 ? Int(input.reference) : index
            try need(source >= 0 && source <= index, "invalid parallel reference")
            let command = commands[source]
            guard command.kind == 0, command.depth.length == 0,
                  !items(command.registers).contains(where: { $0.address == 0x1514 && $0.value != 0 }) else { return nil }
            let attachments = items(command.targets)
            let key = attachments.map { text($0.key) }.joined(separator: "|")
            for target in attachments {
                let name = text(target.key)
                if spilledColors[name] != nil { return nil }
                if let owner = owners[name], owner != key { return nil }
                if let generation = generations[name], generation != target.generation { return nil }
                owners[name] = key; generations[name] = target.generation
            }
            let group: Int
            if let existing = ids[key] { group = existing } else { group = groups.count; ids[key] = group; groups.append([]) }
            groups[group].append(index)
        }
        return groups
    }
}
final class ParallelResults {
    let lock = NSLock()
    var values: [Result<String, Error>?]
    init(_ count: Int) { values = Array(repeating: nil, count: count) }
    func set(_ index: Int, _ value: Result<String, Error>) { lock.lock(); values[index] = value; lock.unlock() }
}
