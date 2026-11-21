import SwiftUI
import AppKit

/// News from Didim Labs, the first shelf of the Didim Labs page: a short list, newest first.
/// It comes from didimlabs.com/store/news.json (docs/store/news.json in this repository), with the copy shipped in the app as fallback.
struct NewsItem: Decodable, Identifiable {
    let id: String
    /// yyyy-mm-dd
    let date: String
    let tag: String?
    let title: String
    let body: String
    let link: String?
}

struct NewsFeed: Decodable {
    let schema: Int
    let items: [NewsItem]

    static let address = "https://didimlabs.com/store/news.json"

    /// A link goes to our own pages or the project's GitHub, over https, and nowhere else.
    static func linkURL(_ value: String?) -> URL? {
        guard let value, let url = URL(string: value), url.scheme == "https", let host = url.host, url.user == nil, url.password == nil, url.port == nil,
              ["didimlabs.com", "goat.didimlabs.com", "github.com"].contains(host) else { return nil }
        if host == "github.com", !url.path.hasPrefix("/BCD1210/") { return nil }
        return url
    }
    static func decode(_ data: Data) -> [NewsItem]? {
        guard data.count <= 256 * 1024, let feed = try? JSONDecoder().decode(NewsFeed.self, from: data), feed.schema == 1 else { return nil }
        return feed.items.filter { item in
            item.id.range(of: "^[a-z0-9][a-z0-9-]{0,63}$", options: .regularExpression) != nil
                && item.date.range(of: "^20[0-9]{2}-[01][0-9]-[0-3][0-9]$", options: .regularExpression) != nil
                && !item.title.isEmpty && item.title.count <= 120 && item.body.count <= 600 && (item.link == nil || linkURL(item.link) != nil)
        }.sorted { $0.date > $1.date }.prefix(30).map { $0 }
    }
    static func bundled(root: URL) -> [NewsItem] {
        guard let data = try? Data(contentsOf: root.appendingPathComponent("resources/didim-news.json")) else { return [] }
        return decode(data) ?? []
    }
}

@MainActor final class NewsModel: ObservableObject {
    @Published var items: [NewsItem] = []

    func load(root: URL) {
        if items.isEmpty { items = NewsFeed.bundled(root: root) }
        var request = URLRequest(url: URL(string: NewsFeed.address)!)
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.timeoutInterval = 15
        URLSession.shared.dataTask(with: request) { data, response, _ in
            let fresh = (response as? HTTPURLResponse)?.statusCode == 200 ? data.flatMap(NewsFeed.decode) : nil
            DispatchQueue.main.async { if let fresh, !fresh.isEmpty { self.items = fresh } }
        }.resume()
    }
}

struct NewsShelfView: View {
    @ObservedObject var news: NewsModel
    let accent: Color

    private static let read: DateFormatter = { let f = DateFormatter(); f.dateFormat = "yyyy-MM-dd"; f.locale = Locale(identifier: "en_US_POSIX"); return f }()
    private static let show: DateFormatter = { let f = DateFormatter(); f.dateStyle = .medium; f.timeStyle = .none; return f }()
    private func day(_ value: String) -> String { Self.read.date(from: value).map(Self.show.string(from:)) ?? value }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                ForEach(news.items) { row($0) }
                if news.items.isEmpty { Text("No news yet.").foregroundStyle(.secondary).padding(.top, 40) }
            }.frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func row(_ item: NewsItem) -> some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack(spacing: 8) {
                if let tag = item.tag, !tag.isEmpty {
                    Text(tag.uppercased()).font(.system(size: 10, weight: .bold)).tracking(0.8).foregroundStyle(accent)
                }
                Text(day(item.date)).font(.system(size: 11)).foregroundStyle(.secondary)
            }
            Text(item.title).font(.system(size: 18, weight: .bold, design: .rounded))
            if !item.body.isEmpty {
                Text(item.body).font(.system(size: 12)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            if let url = NewsFeed.linkURL(item.link) { Link("Read more", destination: url).font(.system(size: 12)) }
        }.padding(18).frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.primary.opacity(0.04), in: RoundedRectangle(cornerRadius: 16))
    }
}
