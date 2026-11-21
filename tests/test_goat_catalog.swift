import Foundation

@main struct GoatCatalogTests {
    static func game(_ over: [String: Any] = [:]) -> [String: Any] {
        var g: [String: Any] = ["id": "aaaaaaaaaaaa", "title": "Rooftop Cat Duel", "maker": "nyang_maker", "summary": "Two ninja cats fight on a rooftop.",
                                "engine": "web", "cover": "https://goat.didimlabs.com/store/cover/aaaaaaaaaaaa.jpg?v=5",
                                "play": "https://0123456789abcdef0123456789abcdef-play.didimlabs.com/", "listed": 1]
        for (k, v) in over { g[k] = v }
        return g
    }
    static func list(_ games: [[String: Any]], schema: Int = 1) -> [GoatGame]? {
        GoatCatalog.decode(try! JSONSerialization.data(withJSONObject: ["schema": schema, "games": games]))
    }
    static func main() {
        assert(list([game()])?.count == 1)
        assert(list([game()])?.first?.maker == "nyang_maker")
        assert(list([game()], schema: 2) == nil)                                  // a list this app does not know
        assert(GoatCatalog.decode(Data("not json".utf8)) == nil)
        assert(list([])?.isEmpty == true)                                         // an empty shelf is a shelf
        var none = game(); none["cover"] = NSNull(); assert(list([none])?.count == 1)   // no picture is fine
        // Only the game's own address under didimlabs.com, over https.
        for bad in ["http://0123456789abcdef-play.didimlabs.com/", "https://evil.example.com/", "https://0123456789abcdef-play.didimlabs.com.evil.com/",
                    "https://didimlabs.com/", "https://goat.didimlabs.com/", "https://user@0123456789abcdef-play.didimlabs.com/",
                    "https://0123456789abcdef-play.didimlabs.com:8443/", "javascript:alert(1)", "file:///etc/passwd", ""] {
            assert(list([game(["play": bad])])?.isEmpty == true, bad)
        }
        assert(list([game(["play": "https://play.didimlabs.com/0123456789abcdef/"])])?.count == 1)
        // A picture from anywhere else, a maker that is not a handle, an id that is not a game's: left out, the rest kept.
        assert(list([game(["cover": "https://evil.example.com/x.jpg"])])?.isEmpty == true)
        assert(list([game(["cover": "https://goat.didimlabs.com/api/state"])])?.isEmpty == true)
        for bad in ["Mike Kim", "a@x.com", "ab", "1abc", "DIDIM", ""] { assert(list([game(["maker": bad])])?.isEmpty == true, bad) }
        assert(list([game(["id": "../../etc"])])?.isEmpty == true)
        assert(list([game(["title": "  "])])?.isEmpty == true)
        assert(list([game(), game(["id": "bbbbbbbbbbbb", "play": "https://evil.example.com/"]), game(["id": "cccccccccccc"])])?.map(\.id) == ["aaaaaaaaaaaa", "cccccccccccc"])
        // Leaving the game: only its own address stays in the window.
        let play = URL(string: "https://0123456789abcdef-play.didimlabs.com/")!
        assert(GoatCatalog.staysInGame(URL(string: "https://0123456789abcdef-play.didimlabs.com/level2.html")!, game: play))
        assert(!GoatCatalog.staysInGame(URL(string: "https://fedcba9876543210-play.didimlabs.com/")!, game: play))
        assert(!GoatCatalog.staysInGame(URL(string: "https://example.com/")!, game: play))
        assert(!GoatCatalog.staysInGame(URL(string: "http://0123456789abcdef-play.didimlabs.com/")!, game: play))
        print("goat catalog: ok")
    }
}
