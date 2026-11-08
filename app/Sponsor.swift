import SwiftUI

/// One paid sidebar slot, listed in didimlabs.com/sponsors.json.
struct SponsorSlot: Decodable, Equatable {
    let name: String
    let line: String
    let url: String
    let starts: String
    let ends: String

    var link: URL? {
        guard var parts = URLComponents(string: url), parts.scheme == "https", let host = parts.host,
              !host.isEmpty, parts.user == nil, parts.password == nil else { return nil }
        var query = parts.queryItems ?? []
        if !query.contains(where: { $0.name == "utm_source" }) {
            query.append(URLQueryItem(name: "utm_source", value: "soju"))
            parts.queryItems = query
        }
        return parts.url
    }

    /// Dates are inclusive calendar days (YYYY-MM-DD, UTC).
    func isActive(on day: String) -> Bool { starts <= day && day <= ends }
}

struct SponsorFeed: Decodable {
    let schema: Int
    let slots: [SponsorSlot]

    static func current(from data: Data, now: Date = Date()) -> SponsorSlot? {
        guard data.count <= 64 * 1024, let feed = try? JSONDecoder().decode(SponsorFeed.self, from: data),
              feed.schema == 1 else { return nil }
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(identifier: "UTC")
        formatter.dateFormat = "yyyy-MM-dd"
        let today = formatter.string(from: now)
        return feed.slots.first { slot in
            slot.isActive(on: today) && slot.link != nil
                && (1...40).contains(slot.name.count) && (1...90).contains(slot.line.count)
        }
    }
}

@MainActor final class SponsorModel: ObservableObject {
    @Published var slot: SponsorSlot?
    private var loaded = false

    func load() {
        guard !loaded else { return }
        loaded = true
        // SOJU_SPONSORS_FILE points at a local feed for checking the card before publishing it.
        if let path = ProcessInfo.processInfo.environment["SOJU_SPONSORS_FILE"],
           let data = FileManager.default.contents(atPath: path) {
            slot = SponsorFeed.current(from: data)
            return
        }
        var request = URLRequest(url: URL(string: "https://didimlabs.com/sponsors.json")!)
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.timeoutInterval = 15
        URLSession.shared.dataTask(with: request) { data, response, _ in
            let slot = (response as? HTTPURLResponse)?.statusCode == 200 ? data.flatMap { SponsorFeed.current(from: $0) } : nil
            DispatchQueue.main.async { self.slot = slot }
        }.resume()
    }
}

/// Bottom-left of the sidebar: the current sponsor, or an offer to buy the slot.
struct SponsorCard: View {
    @StateObject private var sponsors = SponsorModel()
    static let page = URL(string: "https://didimlabs.com/sponsor.html")!

    var body: some View {
        Group {
            if let slot = sponsors.slot, let link = slot.link {
                VStack(alignment: .leading, spacing: 5) {
                    Text("SPONSOR").font(.system(size: 9, weight: .semibold)).foregroundStyle(.tertiary)
                    Link(destination: link) {
                        VStack(alignment: .leading, spacing: 3) {
                            Text(slot.name).font(.system(size: 12, weight: .semibold)).foregroundStyle(.primary)
                            Text(slot.line).font(.system(size: 11)).foregroundStyle(.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }.buttonStyle(.plain).help(link.host ?? "")
                    Link(destination: Self.page) {
                        Text("Advertise here").font(.system(size: 10)).foregroundStyle(.tertiary)
                    }.buttonStyle(.plain)
                }
            } else {
                Link(destination: Self.page) {
                    VStack(alignment: .leading, spacing: 4) {
                        Label("Advertise in Soju", systemImage: "megaphone")
                            .font(.system(size: 12, weight: .medium)).foregroundStyle(.primary)
                        Text("This spot is for sale. One sponsor at a time.")
                            .font(.system(size: 11)).foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }.buttonStyle(.plain).help("Buy this spot or ask about a partnership")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 13).padding(.vertical, 11)
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Color.primary.opacity(0.1)))
        .padding(.top, 10)
        .onAppear { sponsors.load() }
    }
}
