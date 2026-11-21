import Foundation

/// A game made with GOAT (goat.didimlabs.com) that its maker asked to list and Didim Labs said yes to.
/// These are web builds: Play opens the game's own address, nothing is installed.
struct GoatGame: Decodable, Identifiable {
    let id: String
    let title: String
    /// The maker's handle on GOAT, shown as @handle.
    let maker: String
    let summary: String
    let engine: String?
    let cover: String?
    let play: String
}

/// The list at goat.didimlabs.com/store/goat.json.
struct GoatCatalog: Decodable {
    let schema: Int
    let games: [GoatGame]

    static let address = "https://goat.didimlabs.com/store/goat.json"

    /// A game is played at its own address under didimlabs.com (<key>-play.didimlabs.com), over https, and nowhere else.
    static func playURL(_ value: String) -> URL? {
        guard let url = URL(string: value), url.scheme == "https", let host = url.host, url.user == nil, url.password == nil, url.port == nil,
              host == "play.didimlabs.com" || host.range(of: "^[a-z0-9]{8,64}-play\\.didimlabs\\.com$", options: .regularExpression) != nil else { return nil }
        return url
    }
    static func coverURL(_ value: String?) -> URL? {
        guard let value, let url = URL(string: value), url.scheme == "https", url.host == "goat.didimlabs.com", url.user == nil, url.password == nil, url.port == nil,
              url.path.hasPrefix("/store/cover/") else { return nil }
        return url
    }
    /// Entries that are not what the list promises (another host to play at, a maker that is not a handle) are left out.
    static func decode(_ data: Data) -> [GoatGame]? {
        guard data.count <= 1024 * 1024, let catalog = try? JSONDecoder().decode(GoatCatalog.self, from: data), catalog.schema == 1 else { return nil }
        return catalog.games.filter { game in
            game.id.range(of: "^[0-9a-f]{12}$", options: .regularExpression) != nil
                && game.maker.range(of: "^[a-z][a-z0-9_]{2,19}$", options: .regularExpression) != nil
                && !game.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && game.title.count <= 120 && game.summary.count <= 600
                && playURL(game.play) != nil && (game.cover == nil || coverURL(game.cover) != nil)
        }
    }
    /// Whether a page the game asks to go to stays in the game's own window: only its own address does.
    static func staysInGame(_ target: URL, game: URL) -> Bool {
        target.scheme == "https" && target.host == game.host && target.port == nil
    }
}
