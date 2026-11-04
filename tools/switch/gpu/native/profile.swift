import os.signpost

final class NativeSpan {
    static let enabled = ProcessInfo.processInfo.environment["SOJU_PROFILE"] != nil
    static let log = OSLog(subsystem: "org.soju.switch", category: .pointsOfInterest)
    static let lock = NSLock()
    static var totals = [String: (Int, Double)]()
    let name: String
    let start: Double
    let id: OSSignpostID
    var finished = false
    init(_ name: String) {
        self.name = name
        start = NativeSpan.enabled ? ProcessInfo.processInfo.systemUptime : 0
        id = NativeSpan.enabled ? OSSignpostID(log: NativeSpan.log) : .invalid
        if NativeSpan.enabled { os_signpost(.begin, log: NativeSpan.log, name: "Native stage", signpostID: id, "%{public}s", name) }
    }
    func end() {
        if finished || !NativeSpan.enabled { return }
        finished = true
        let elapsed = ProcessInfo.processInfo.systemUptime - start
        os_signpost(.end, log: NativeSpan.log, name: "Native stage", signpostID: id)
        NativeSpan.lock.lock()
        let old = NativeSpan.totals[name] ?? (0, 0)
        NativeSpan.totals[name] = (old.0 + 1, old.1 + elapsed)
        NativeSpan.lock.unlock()
    }
    deinit { end() }
    static func report() -> String {
        guard enabled else { return "" }
        lock.lock(); defer { lock.unlock() }
        return totals.keys.sorted().map { key in
            let value = totals[key]!
            return "\(key)_calls=\(value.0) \(key)_ms=\(value.1 * 1000)"
        }.joined(separator: " ")
    }
}
