import Foundation

/// When to ask for a GitHub star: only players who clearly have Soju working for them.
/// The sign is the same game started on three different days (a game that fails is not
/// started again on later days), or games started on five different days in total.
/// Asked at most twice, a week apart; "Star on GitHub" or "Don't ask again" ends it.
struct StarAsk: Codable, Equatable {
    var days: [String: Set<String>] = [:]   // game id -> calendar days it was started (yyyy-MM-dd)
    var asked = 0
    var lastAsked: Date?
    var done = false

    static let repo = URL(string: "https://github.com/BCD1210/soju")!
    static let key = "soju.starask.v1"

    static func day(_ date: Date) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.timeZone = .current
        f.dateFormat = "yyyy-MM-dd"
        return f.string(from: date)
    }

    mutating func launched(_ game: String, at date: Date = Date()) {
        var set = days[game] ?? []
        set.insert(StarAsk.day(date))
        // Keep the file small: the last 30 days per game are plenty for the rule.
        if set.count > 30 { set = Set(set.sorted().suffix(30)) }
        days[game] = set
    }

    /// The game that shows Soju works best (most days), if the rule is met.
    func favorite() -> String? {
        guard let best = days.max(by: { $0.value.count < $1.value.count || ($0.value.count == $1.value.count && $0.key > $1.key) }) else { return nil }
        let total = days.values.reduce(into: Set<String>()) { $0.formUnion($1) }.count
        return best.value.count >= 3 || total >= 5 ? best.key : nil
    }

    func shouldAsk(now: Date = Date()) -> Bool {
        guard !done, asked < 2, favorite() != nil else { return false }
        if let last = lastAsked, now.timeIntervalSince(last) < 7 * 24 * 3600 { return false }
        return true
    }

    mutating func later(now: Date = Date()) { asked += 1; lastAsked = now; if asked >= 2 { done = true } }
    mutating func finish() { done = true }

    static func load() -> StarAsk {
        guard let data = UserDefaults.standard.data(forKey: key),
              let value = try? JSONDecoder().decode(StarAsk.self, from: data) else { return StarAsk() }
        return value
    }
    func save() {
        if let data = try? JSONEncoder().encode(self) { UserDefaults.standard.set(data, forKey: StarAsk.key) }
    }
}
