import Foundation

@main struct StarAskTests {
    static func at(_ day: Int) -> Date { Date(timeIntervalSince1970: 1_790_000_000 + Double(day) * 86400) }
    static func main() {
        var s = StarAsk()
        assert(!s.shouldAsk(now: at(0)))
        s.launched("steam:1", at: at(0)); s.launched("steam:1", at: at(0).addingTimeInterval(600))
        s.launched("steam:1", at: at(1))
        assert(s.favorite() == nil && !s.shouldAsk(now: at(1)))          // two days is not enough
        s.launched("steam:2", at: at(1))
        s.launched("steam:1", at: at(2))
        assert(s.favorite() == "steam:1" && s.shouldAsk(now: at(2)))     // same game, third day

        var t = StarAsk()                                                // five days, different games
        for d in 0..<5 { t.launched("game:\(d)", at: at(d)) }
        assert(t.shouldAsk(now: at(5)))

        s.later(now: at(2))
        assert(!s.shouldAsk(now: at(5)))                                 // a week between asks
        assert(s.shouldAsk(now: at(9)))
        s.later(now: at(9))
        assert(s.done && !s.shouldAsk(now: at(30)))                      // asked twice at most

        var u = StarAsk()
        for d in 0..<3 { u.launched("x", at: at(d)) }
        u.finish()
        assert(!u.shouldAsk(now: at(3)))
        for d in 0..<40 { u.launched("x", at: at(d)) }
        assert(u.days["x"]!.count == 30)
        let data = try! JSONEncoder().encode(u)
        assert(try! JSONDecoder().decode(StarAsk.self, from: data) == u)
        print("star ask: ok")
    }
}
