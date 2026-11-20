import Foundation

/// GardenStats: the Soju Garden's days, shots and streaks from the local play log.
@main struct GardenStatsTests {
    static func main() {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(identifier: "America/Los_Angeles")!
        cal.firstWeekday = 1
        let log = """
        {"start": "2026-11-17T20:00:00-08:00", "minutes": 25, "game": "steam:1", "title": "A"}
        {"start": "2026-11-18T21:00:00-08:00", "minutes": 95, "game": "steam:1", "title": "A"}
        {"start": "2026-11-19T02:10:00-08:00", "minutes": 40, "game": "steam:2", "title": "B"}
        {"start": "2026-11-19T22:00:00-08:00", "minutes": 130, "game": "steam:1", "title": "A"}
        {"start": "2026-11-10T22:00:00-08:00", "minutes": 30, "game": "gog:9", "title": ""}
        not json
        {"start": "2026-11-20T09:00:00-08:00", "minutes": 0, "game": "x", "title": "zero"}
        """
        let sessions = Garden.parse(log)
        assert(sessions.count == 5, "\(sessions.count)")
        let today = ISO8601DateFormatter().date(from: "2026-11-20T08:00:00-08:00")!
        let g = Garden(sessions: sessions, today: today, calendar: cal)
        assert(g.weeks.count == 53 && g.weeks.allSatisfy { $0.count == 7 })
        let days = g.weeks.joined().compactMap { $0 }
        assert(days.last.map { cal.isDate($0.date, inSameDayAs: today) } == true)    // today is the last square
        assert(g.weeks.last!.compactMap { $0 }.count == 6)                           // Friday: Sun..Fri drawn
        let nov19 = days.last { cal.component(.day, from: $0.date) == 19 }!
        assert(nov19.minutes == 170 && nov19.shots == 6 && nov19.level == 4 && nov19.titles == ["B", "A"])
        assert(GardenDay(date: today, minutes: 25, titles: []).level == 1)
        assert(GardenDay(date: today, minutes: 30, titles: []).shots == 1)
        assert(GardenDay(date: today, minutes: 31, titles: []).shots == 2)
        assert(!g.playedToday && g.shotsToday == 0)
        assert(g.streak == 3, "\(g.streak)")          // 17, 18, 19 (today not played yet)
        assert(g.bestStreak == 3)
        assert(g.topGame == "A" && g.totalMinutes == 320 && g.lateNight)
        assert(g.shotsThisYear == 1 + 4 + 6 + 1, "\(g.shotsThisYear)")
        assert(g.allShots == 12)
        assert(g.badges.filter { $0.earned }.map { $0.id } == ["first", "bottle", "late"], "\(g.badges.filter { $0.earned }.map { $0.id })")
        assert(g.nextBadge.map { [$0.0.id, String($0.1)] } == ["bottles5", "23"])
        assert(g.rank.en == "Bronze" && g.nextRank.map { [$0.0.en, String($0.1)] } == ["Silver", "23"])
        let empty = Garden(sessions: [], today: today, calendar: cal)
        assert(!empty.any && empty.streak == 0 && empty.topGame == nil && empty.badges.allSatisfy { !$0.earned })
        assert(empty.nextBadge.map { [$0.0.id, String($0.1)] } == ["first", "1"])
        assert(empty.rank.en == "Iron" && empty.nextRank?.0.en == "Bronze")
        var lots = g; lots.allShots = 1000
        assert(lots.rank.en == "Grandmaster" && lots.nextRank == nil)
        print("garden stats: ok")
    }
}
