import Foundation

@main struct PurchaseLinkTests {
    static func main() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root.appendingPathComponent("resources"), withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let file = root.appendingPathComponent("resources/purchase-links.json")
        let direct = "https://www.gog.com/en/game/test_game"
        let affiliate = "https://af.gog.com/en/game/test_game?as=TEST_ONLY"
        func config(_ entries: [[String: String]], enabled: Bool = true, schema: Int = 1) throws -> PurchaseLinks {
            let data = try JSONSerialization.data(withJSONObject: ["schema": schema, "enabled": enabled, "links": entries])
            try data.write(to: file)
            return PurchaseLinks.load(root: root)
        }
        let entry = ["destination": direct, "affiliate": affiliate]
        assert(PurchaseLinks.load(root: root).resolve(direct)?.url.absoluteString == direct)
        let active = try config([entry])
        assert(active.hasAffiliateLinks)
        assert(active.resolve(direct)?.url.absoluteString == affiliate)
        assert(active.resolve(direct)?.isAffiliate == true)
        let other = "https://www.gog.com/en/game/another_edition"
        assert(active.resolve(other)?.url.absoluteString == other)
        let steam = "https://store.steampowered.com/app/10/"
        assert(active.resolve(steam)?.url.absoluteString == steam)
        let disabled = try config([entry], enabled: false)
        assert(!disabled.hasAffiliateLinks)
        assert(disabled.resolve(direct)?.url.absoluteString == direct)
        for bad in ["http://af.gog.com/x", "https://af.gog.com.evil.test/x", "https://user@af.gog.com/x",
                    "https://af.gog.com:8080/x", "file:///etc/passwd", "javascript:alert(1)"] {
            let value = try config([["destination": direct, "affiliate": bad]])
            assert(!value.hasAffiliateLinks)
            assert(value.resolve(direct)?.url.absoluteString == direct)
        }
        for bad in ["file:///etc/passwd", "https://www.gog.com.evil.test/x", "https://x@www.gog.com/x"] {
            assert(active.resolve(bad) == nil)
        }
        let duplicate = try config([entry, entry])
        assert(!duplicate.hasAffiliateLinks)
        let wrongSchema = try config([entry], schema: 2)
        assert(!wrongSchema.hasAffiliateLinks)
        try Data("not json".utf8).write(to: file)
        assert(!PurchaseLinks.load(root: root).hasAffiliateLinks)
        print("Purchase links: automatic affiliate preference, exact products, ordinary-link fallback and unsafe URL checks passed")
    }
}
