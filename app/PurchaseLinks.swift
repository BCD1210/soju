import Foundation

/// Reviewed links shipped with the app, never supplied by search responses.
struct PurchaseLinks: Decodable {
    struct Entry: Decodable {
        let destination: String
        let affiliate: String
    }
    struct Route {
        let url: URL
        let isAffiliate: Bool
    }
    let schema: Int
    let enabled: Bool
    let links: [Entry]
    static let empty = PurchaseLinks(schema: 1, enabled: false, links: [])

    static func load(root: URL) -> PurchaseLinks {
        let file = root.appendingPathComponent("resources/purchase-links.json")
        guard let data = try? Data(contentsOf: file), data.count <= 256 * 1024,
              let config = try? JSONDecoder().decode(Self.self, from: data),
              config.schema == 1, config.links.count <= 500 else { return .empty }
        var seen = Set<String>()
        for link in config.links {
            guard let destination = safeURL(link.destination, hosts: ["www.gog.com"]),
                  destination.path.hasPrefix("/en/game/") || destination.path.hasPrefix("/game/"),
                  destination.query == nil, destination.fragment == nil,
                  safeURL(link.affiliate, hosts: ["www.gog.com", "af.gog.com"]) != nil,
                  seen.insert(link.destination).inserted else { return .empty }
        }
        return config
    }

    var hasAffiliateLinks: Bool { enabled && !links.isEmpty }

    func resolve(_ destination: String, useAffiliateLinks: Bool) -> Route? {
        guard let direct = Self.safeURL(destination, hosts: [
            "store.steampowered.com", "www.gog.com", "store.epicgames.com", "us.shop.battle.net"
        ]) else { return nil }
        if schema == 1, enabled, useAffiliateLinks,
           let entry = links.first(where: { $0.destination == destination }),
           direct.host == "www.gog.com",
           let affiliate = Self.safeURL(entry.affiliate, hosts: ["www.gog.com", "af.gog.com"]) {
            return Route(url: affiliate, isAffiliate: true)
        }
        return Route(url: direct, isAffiliate: false)
    }

    private static func safeURL(_ value: String, hosts: Set<String>) -> URL? {
        guard value.count <= 8192, !value.contains("\\"),
              !value.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }),
              let url = URL(string: value), url.scheme == "https",
              url.user == nil, url.password == nil, url.port == nil || url.port == 443,
              let host = url.host, hosts.contains(host) else { return nil }
        return url
    }
}
