import Foundation

/// One finished game from play-log.jsonl, which scripts/play-stats.py keeps on this Mac only.
struct PlaySession: Equatable {
    var start: Date
    var minutes: Int
    var game: String
    var title: String
}

/// One square of the Soju Garden: a day and how much was played.
struct GardenDay: Equatable {
    static let minutesPerShot = 30
    var date: Date
    var minutes: Int
    var titles: [String]
    /// Shots poured that day: every started half hour counts.
    var shots: Int { minutes <= 0 ? 0 : (minutes + Self.minutesPerShot - 1) / Self.minutesPerShot }
    /// How full the glass is drawn: 0 none, 1 a sip, 2 one shot, 3 up to two hours, 4 bottoms up.
    var level: Int { minutes <= 0 ? 0 : minutes < 30 ? 1 : minutes < 60 ? 2 : minutes < 120 ? 3 : 4 }
}

/// The year of play the Soju Garden draws: 53 week columns ending with this week, and its numbers.
struct Garden {
    var weeks: [[GardenDay?]]       // columns of seven days; nil after today
    var shotsThisYear = 0
    var shotsToday = 0
    var streak = 0                  // days in a row up to today, or up to yesterday when today has none yet
    var bestStreak = 0
    var playedToday = false
    var topGame: String?
    var totalMinutes = 0
    var lateNight = false           // a game started between 1 and 5 in the morning
    var allShots = 0
    var any: Bool { totalMinutes > 0 }

    /// Badges for fun: play time counted in shots and bottles (a bottle pours 7 shots, a crate holds
    /// 20 bottles), plus streaks and a late-night game.
    struct Badge: Equatable {
        var id: String, en: String, ko: String
        var shots: Int?             // play-time badges: shots needed
        var earned: Bool
    }
    var badges: [Badge] {
        func pour(_ id: String, _ en: String, _ ko: String, _ n: Int) -> Badge {
            Badge(id: id, en: en, ko: ko, shots: n, earned: allShots >= n)
        }
        return [pour("first", "First shot", "첫 잔", 1),
                pour("bottle", "One bottle", "한 병", 7),
                pour("bottles5", "Five bottles", "다섯 병", 35),
                pour("crate", "One crate", "한 짝", 140),
                pour("crates5", "Five crates", "다섯 짝", 700),
                Badge(id: "streak7", en: "7-day streak", ko: "7일 연속", shots: nil, earned: bestStreak >= 7),
                Badge(id: "streak30", en: "30-day streak", ko: "30일 연속", shots: nil, earned: bestStreak >= 30),
                Badge(id: "late", en: "Late-night shot", ko: "새벽 한 잔", shots: nil, earned: lateNight)]
    }
    /// A rank for fun, like a game ladder, from all the shots poured.
    struct Rank: Equatable { var en: String, ko: String, shots: Int, tier: Int }
    static let ranks = [Rank(en: "Iron", ko: "아이언", shots: 0, tier: 0), Rank(en: "Bronze", ko: "브론즈", shots: 7, tier: 1),
                        Rank(en: "Silver", ko: "실버", shots: 35, tier: 2), Rank(en: "Gold", ko: "골드", shots: 70, tier: 3),
                        Rank(en: "Platinum", ko: "플래티넘", shots: 140, tier: 4), Rank(en: "Diamond", ko: "다이아몬드", shots: 280, tier: 5),
                        Rank(en: "Master", ko: "마스터", shots: 500, tier: 6), Rank(en: "Grandmaster", ko: "그랜드마스터", shots: 1000, tier: 7)]
    var rank: Rank { Self.ranks.last { allShots >= $0.shots }! }
    /// The rank above and the shots still to pour for it; nil at Grandmaster.
    var nextRank: (Rank, Int)? { Self.ranks.first { $0.shots > allShots }.map { ($0, $0.shots - allShots) } }

    /// The next play-time badge and the shots still to pour for it.
    var nextBadge: (Badge, Int)? {
        badges.first { $0.shots != nil && !$0.earned }.map { ($0, $0.shots! - allShots) }
    }

    static func parse(_ text: String) -> [PlaySession] {
        let iso = ISO8601DateFormatter()
        return text.split(separator: "\n").compactMap { line in
            guard let data = line.data(using: .utf8),
                  let o = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
                  let s = o["start"] as? String, let start = iso.date(from: s),
                  let m = o["minutes"] as? Int, m > 0 else { return nil }
            return PlaySession(start: start, minutes: m, game: o["game"] as? String ?? "", title: o["title"] as? String ?? "")
        }
    }

    init(sessions: [PlaySession], today: Date = Date(), calendar cal: Calendar = .current) {
        let todayStart = cal.startOfDay(for: today)
        var minutes: [Date: Int] = [:], titles: [Date: [String]] = [:], byGame: [String: Int] = [:]
        for s in sessions {
            let d = cal.startOfDay(for: s.start)
            minutes[d, default: 0] += s.minutes
            let name = s.title.isEmpty ? s.game : s.title
            if !name.isEmpty && !(titles[d] ?? []).contains(name) { titles[d, default: []].append(name) }
            byGame[name, default: 0] += s.minutes
            let hour = cal.component(.hour, from: s.start)
            if hour >= 1 && hour < 5 { lateNight = true }
        }
        func day(_ d: Date) -> GardenDay { GardenDay(date: d, minutes: minutes[d] ?? 0, titles: titles[d] ?? []) }
        let back = (cal.component(.weekday, from: todayStart) - cal.firstWeekday + 7) % 7
        let thisWeek = cal.date(byAdding: .day, value: -back, to: todayStart)!
        let first = cal.date(byAdding: .day, value: -7 * 52, to: thisWeek)!
        weeks = (0..<53).map { w in
            (0..<7).map { r in
                let d = cal.date(byAdding: .day, value: w * 7 + r, to: first)!
                return d > todayStart ? nil : day(d)
            }
        }
        let year = cal.component(.year, from: todayStart)
        allShots = minutes.reduce(0) { $0 + GardenDay(date: $1.key, minutes: $1.value, titles: []).shots }
        shotsThisYear = minutes.filter { cal.component(.year, from: $0.key) == year }
            .reduce(0) { $0 + GardenDay(date: $1.key, minutes: $1.value, titles: []).shots }
        shotsToday = day(todayStart).shots
        playedToday = minutes[todayStart] != nil
        var d = playedToday ? todayStart : cal.date(byAdding: .day, value: -1, to: todayStart)!
        while minutes[d] != nil { streak += 1; d = cal.date(byAdding: .day, value: -1, to: d)! }
        var run = 0, prev: Date?
        for k in minutes.keys.sorted() {
            run = prev.flatMap { cal.date(byAdding: .day, value: 1, to: $0) } == k ? run + 1 : 1
            bestStreak = max(bestStreak, run)
            prev = k
        }
        topGame = byGame.max { $0.value < $1.value || ($0.value == $1.value && $0.key > $1.key) }?.key
        totalMinutes = byGame.values.reduce(0, +)
    }
}
