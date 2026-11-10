import Foundation

@main struct UpdateCheckTests {
    static func release(_ tag: String, zip: String?, prerelease: Bool = false, draft: Bool = false) -> [String: Any] {
        ["tag_name": tag, "prerelease": prerelease, "draft": draft,
         "assets": zip.map { [["name": $0], ["name": $0 + ".sha256"]] } ?? [[String: Any]]()]
    }
    static func main() throws {
        assert(UpdateCheck.isNewer("1.10.12", than: "1.10.11"))
        assert(UpdateCheck.isNewer("1.10.10", than: "1.9.9"))      // numbers, not text
        assert(UpdateCheck.isNewer("2.0.0", than: "1.99.99"))
        assert(!UpdateCheck.isNewer("1.10.11", than: "1.10.11"))
        assert(!UpdateCheck.isNewer("1.10.10", than: "1.10.11"))
        assert(!UpdateCheck.isNewer("1.10.12", than: "dev"))        // source builds never show the card
        assert(!UpdateCheck.isNewer("1.10", than: "1.9.0"))
        assert(!UpdateCheck.isNewer("1.10.x", than: "1.9.0"))

        let list: [[String: Any]] = [
            release("engine-v1.9", zip: "wine-engine-x86_64.tar.xz"),            // not an app release
            release("v1.10.11", zip: "Soju-1.10.11-macos-arm64.zip"),
            release("v1.11.0", zip: "Soju-1.11.0-macos-arm64.zip", prerelease: true),
            release("v1.10.12", zip: nil),                                       // tagged, zip not uploaded yet
            release("v1.10.9", zip: "Soju-1.10.9-macos-arm64.zip"),
            release("v1.12.0", zip: "Soju-1.12.0-macos-arm64.zip", draft: true),
        ]
        let data = try JSONSerialization.data(withJSONObject: list)
        assert(UpdateCheck.latest(from: data) == "1.10.11")
        // Order in the feed does not matter.
        let reversed = try JSONSerialization.data(withJSONObject: Array(list.reversed()))
        assert(UpdateCheck.latest(from: reversed) == "1.10.11")
        assert(UpdateCheck.latest(from: Data("not json".utf8)) == nil)
        assert(UpdateCheck.latest(from: Data("{\"message\":\"rate limited\"}".utf8)) == nil)
        assert(UpdateCheck.latest(from: Data("[]".utf8)) == nil)
        assert(UpdateCheck.page(for: "1.10.12").absoluteString == "https://github.com/BCD1210/soju/releases/tag/v1.10.12")
        print("update check: ok")
    }
}
