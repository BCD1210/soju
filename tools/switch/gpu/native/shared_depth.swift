struct SharedDepthPlan {
    let attachments: [TargetCommand]
    let slots: [String: Int]
    func mapping(_ command: Command) -> [Int] { items(command.targets).map { slots[text($0.key)]! } }
    func suffix(_ command: Command) -> String {
        ":mrt-v1:" + attachments.map { String($0.metal) }.joined(separator: ",") + ":" + mapping(command).map(String.init).joined(separator: ",")
    }
    func fragment(_ source: String, _ command: Command) -> String {
        var result = source
        // Two-phase replacement avoids mapping 0->1 followed by 1->2 twice.
        for (from, to) in mapping(command).enumerated() {
            result = result.replacingOccurrences(of: "[[color(\(from))]]", with: "[[soju_color_slot_\(to)]]")
        }
        for slot in attachments.indices {
            result = result.replacingOccurrences(of: "[[soju_color_slot_\(slot)]]", with: "[[color(\(slot))]]")
        }
        return result
    }
}
func sharedDepthPlans(_ commands: UnsafeBufferPointer<Command>, _ order: [Int]) -> [Int: SharedDepthPlan] {
    if ProcessInfo.processInfo.environment["SOJU_GPU_MERGE_DEPTH"] == "0" { return [:] }
    var plans = [Int: SharedDepthPlan](), span = [Int](), union = [TargetCommand]()
    var slots = [String: Int](), identity = "", sets = Set<String>()
    func drain() {
        if sets.count > 1 {
            let plan = SharedDepthPlan(attachments: union, slots: slots)
            for index in span { plans[index] = plan }
        }
        span.removeAll(); union.removeAll(); slots.removeAll(); sets.removeAll(); identity = ""
    }
    for index in order {
        let input = commands[index], source = input.kind == 5 ? Int(input.reference) : index
        let command = commands[source]
        guard command.kind == 0, let depth = items(command.depth).first,
              let first = items(command.targets).first,
              !items(command.registers).contains(where: { $0.address == 0x1514 && $0.value != 0 }) else { drain(); continue }
        let id = "\(text(depth.key)):\(depth.generation):\(first.width):\(first.height)"
        let targets = items(command.targets)
        let extra = targets.filter { slots[text($0.key)] == nil }.count
        let changed = targets.contains { target in
            if let slot = slots[text(target.key)] { return union[slot].generation != target.generation }
            return false
        }
        if id != identity || union.count + extra > 8 || changed { drain() }
        identity = id
        for target in targets where slots[text(target.key)] == nil {
            slots[text(target.key)] = union.count; union.append(target)
        }
        sets.insert(targets.map { text($0.key) }.joined(separator: "|")); span.append(index)
    }
    drain(); return plans
}
