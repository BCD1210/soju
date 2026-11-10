import Foundation

/// Finds the newest app release in GitHub's release list. scripts/update.sh applies the same rule:
/// a vX.Y.Z tag that is not a prerelease and carries Soju-X.Y.Z-macos-arm64.zip.
enum UpdateCheck {
    static let feed = URL(string: "https://api.github.com/repos/BCD1210/soju/releases?per_page=30")!

    static func numbers(_ version: String) -> [Int]? {
        let parts = version.split(separator: ".", omittingEmptySubsequences: false).map { Int($0) }
        guard parts.count == 3, !parts.contains(where: { $0 == nil || $0! < 0 }) else { return nil }
        return parts.map { $0! }
    }

    /// False when either side is not three dotted numbers, so a dev build never shows the notice.
    static func isNewer(_ candidate: String, than current: String) -> Bool {
        guard let a = numbers(candidate), let b = numbers(current) else { return false }
        return b.lexicographicallyPrecedes(a)
    }

    /// GitHub lists releases by creation date, not by version, so take the highest version.
    static func latest(from data: Data) -> String? {
        guard data.count <= 2 * 1024 * 1024,
              let releases = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] else { return nil }
        var best: String?
        for release in releases {
            guard let tag = release["tag_name"] as? String, tag.hasPrefix("v"),
                  release["prerelease"] as? Bool == false, release["draft"] as? Bool != true else { continue }
            let version = String(tag.dropFirst())
            guard numbers(version) != nil, let assets = release["assets"] as? [[String: Any]],
                  assets.contains(where: { $0["name"] as? String == "Soju-\(version)-macos-arm64.zip" }) else { continue }
            if best == nil || isNewer(version, than: best!) { best = version }
        }
        return best
    }

    static func page(for version: String) -> URL {
        URL(string: "https://github.com/BCD1210/soju/releases/tag/v" + version)!
    }
}
