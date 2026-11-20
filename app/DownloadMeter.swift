import Foundation

/// The last line of curl's progress meter in a chunk of log, e.g.
/// "  42  512M   42  215M    0     0  12.3M      0  0:00:41  0:00:17  0:00:24 12.5M"
/// (% total, total, % received, received, % sent, sent, average down, up, time total, spent, left, speed).
/// Soju's scripts download with plain curl, so its meter is the only progress they report.
struct DownloadMeter: Equatable {
    var fraction: Double
    var speed: String
    var left: String

    var done: Bool { fraction >= 1 }
    /// "12.5 MB/s · 0:24 left", leaving out what curl does not know yet ("--:--:--").
    var detail: String {
        var parts: [String] = []
        if let rate = Self.bytes(speed), rate > 0 { parts.append(Self.rate(rate)) }
        if !left.contains("-") { parts.append(left + " left") }
        return parts.joined(separator: " · ")
    }

    /// curl's sizes: "53562", "298k", "12.3M" (1024-based).
    static func bytes(_ text: String) -> Double? {
        let units: [Character: Double] = ["k": 1024, "M": 1_048_576, "G": 1_073_741_824, "T": 1_099_511_627_776]
        if let last = text.last, let unit = units[last] { return Double(text.dropLast()).map { $0 * unit } }
        return Double(text)
    }
    /// "52 KB/s", "12.3 MB/s".
    static func rate(_ bytes: Double) -> String {
        if bytes >= 1_048_576 { return String(format: "%.1f MB/s", bytes / 1_048_576) }
        return String(format: "%.0f KB/s", max(1, bytes / 1024))
    }

    private static let line = try! NSRegularExpression(pattern:
        #"^\s*(\d{1,3})\s+[\d.]+[kMGT]?\s+\d{1,3}\s+[\d.]+[kMGT]?\s+\d+\s+\d+\s+([\d.]+[kMGT]?)\s+\S+\s+\S+\s+\S+\s+(\S+)\s+\S+\s*$"#)

    static func last(in text: String) -> DownloadMeter? {
        for raw in text.split(whereSeparator: { $0 == "\n" || $0 == "\r" }).reversed() {
            let s = String(raw)
            guard let m = line.firstMatch(in: s, range: NSRange(s.startIndex..., in: s)),
                  let p = Range(m.range(at: 1), in: s), let sp = Range(m.range(at: 2), in: s),
                  let l = Range(m.range(at: 3), in: s), let pct = Double(s[p]), pct <= 100 else { continue }
            var left = String(s[l])
            // "0:00:24" reads as "0:24"; hours stay when there are some.
            if left.hasPrefix("0:") { left.removeFirst(2) }
            if left.hasPrefix("0") && left.count == 5 { left.removeFirst() }
            return DownloadMeter(fraction: pct / 100, speed: String(s[sp]), left: left)
        }
        return nil
    }
}
