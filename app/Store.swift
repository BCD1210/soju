import SwiftUI
import AppKit

/// Didim Labs games sold through Polar and installed natively (scripts/didim_games.py).
/// The catalog comes from didimlabs.com/store/catalog.json, with the copy shipped in the app as fallback.
struct StoreGame: Decodable, Identifiable {
    let id: String
    let title: String
    let publisher: String
    let price: String
    let checkout: String
    let cover: String?
    let summary: String
    let details: String
    let requires: String
    let steam: String?
}

struct StoreCatalog: Decodable {
    let schema: Int
    let games: [StoreGame]

    static func bundled(root: URL) -> [StoreGame] {
        guard let data = try? Data(contentsOf: root.appendingPathComponent("resources/didim-store.json")) else { return [] }
        return decode(data) ?? []
    }
    static func decode(_ data: Data) -> [StoreGame]? {
        guard data.count <= 512 * 1024, let catalog = try? JSONDecoder().decode(StoreCatalog.self, from: data),
              catalog.schema == 1 else { return nil }
        // Only https links to the store's own checkout and images are used.
        return catalog.games.filter { game in
            game.id.range(of: "^[a-z0-9][a-z0-9-]{0,63}$", options: .regularExpression) != nil
                && safe(game.checkout, hosts: ["buy.polar.sh", "polar.sh"]) != nil
                && (game.cover == nil || safe(game.cover!, hosts: ["didimlabs.com"]) != nil)
                && (game.steam == nil || safe(game.steam!, hosts: ["store.steampowered.com"]) != nil)
        }
    }
    static func safe(_ value: String, hosts: Set<String>) -> URL? {
        guard let url = URL(string: value), url.scheme == "https", let host = url.host, hosts.contains(host),
              url.user == nil, url.password == nil else { return nil }
        return url
    }
}

@MainActor final class StoreModel: ObservableObject {
    @Published var games: [StoreGame] = []
    @Published var offline = false

    func load(root: URL) {
        if games.isEmpty { games = StoreCatalog.bundled(root: root) }
        var request = URLRequest(url: URL(string: "https://didimlabs.com/store/catalog.json")!)
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.timeoutInterval = 15
        URLSession.shared.dataTask(with: request) { data, response, _ in
            let fresh = (response as? HTTPURLResponse)?.statusCode == 200 ? data.flatMap(StoreCatalog.decode) : nil
            DispatchQueue.main.async {
                if let fresh, !fresh.isEmpty { self.games = fresh; self.offline = false } else { self.offline = true }
            }
        }.resume()
    }
}

struct StoreView: View {
    @EnvironmentObject var model: SojuModel
    @StateObject private var store = StoreModel()
    @State private var keyFor: StoreGame?
    @State private var key = ""
    private let accent = Color(red: 0.79, green: 0.36, blue: 0.27)

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            VStack(alignment: .leading, spacing: 7) {
                Text("Didim Labs Store").font(.system(size: 30, weight: .bold, design: .rounded))
                Text("Games made by Didim Labs, built natively for Apple Silicon. Buy once, then install and play them here.")
                    .font(.callout).foregroundStyle(.secondary)
            }
            if model.busy || !model.activity.isEmpty && model.activity != "Ready when you are." {
                HStack(spacing: 8) {
                    if model.busy { ProgressView().controlSize(.small) }
                    else { Image(systemName: model.failed ? "exclamationmark.circle" : "checkmark.circle").foregroundStyle(model.failed ? .orange : .green) }
                    Text(model.busy ? lastLine : (model.failed ? lastLine : model.activity)).font(.system(size: 12)).lineLimit(2)
                }
            }
            ScrollView {
                VStack(spacing: 18) {
                    ForEach(store.games) { card($0) }
                    if store.games.isEmpty {
                        Text(store.offline ? "The store is unavailable right now. Check the internet connection." : "Loading the store…")
                            .foregroundStyle(.secondary).padding(.top, 40)
                    }
                }
            }
            Text("Payments are handled by Polar, Didim Labs' reseller. The license key from your receipt email is saved in this Mac's Keychain.")
                .font(.system(size: 11)).foregroundStyle(.secondary)
        }
        .padding(28).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .onAppear { store.load(root: model.root) }
        .sheet(item: $keyFor) { game in keySheet(game) }
    }

    private var lastLine: String {
        model.log.split(separator: "\n").last.map { String($0).replacingOccurrences(of: "soju: ", with: "") } ?? model.activity
    }

    private func installed(_ game: StoreGame) -> InstalledGame? {
        model.games.first { $0.id == "didim:" + game.id && $0.isInstalled }
    }

    private func card(_ game: StoreGame) -> some View {
        HStack(alignment: .top, spacing: 22) {
            ZStack {
                LinearGradient(colors: [accent.opacity(0.5), .black], startPoint: .topLeading, endPoint: .bottomTrailing)
                if let cover = game.cover, let url = URL(string: cover) {
                    AsyncImage(url: url) { image in image.resizable().scaledToFill() } placeholder: { Image(systemName: "cat.fill").font(.largeTitle) }
                }
            }.frame(width: 320, height: 150).clipped().clipShape(RoundedRectangle(cornerRadius: 12))
            VStack(alignment: .leading, spacing: 9) {
                Text(game.publisher.uppercased()).font(.system(size: 10, weight: .bold)).tracking(0.8).foregroundStyle(accent)
                Text(game.title).font(.system(size: 22, weight: .bold, design: .rounded))
                Text(game.summary).font(.system(size: 12)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                Text(game.details).font(.system(size: 11))
                Text(game.requires).font(.system(size: 11)).foregroundStyle(.secondary)
                HStack(spacing: 10) {
                    if let game = installed(game) {
                        Button { model.play(game) } label: { Label("Play", systemImage: "play.fill") }
                            .buttonStyle(.borderedProminent).tint(.green).disabled(model.launchingGames.contains(game.id))
                    } else {
                        Button { if let url = StoreCatalog.safe(game.checkout, hosts: ["buy.polar.sh", "polar.sh"]) { NSWorkspace.shared.open(url) } } label: {
                            Label("Buy · " + game.price, systemImage: "cart")
                        }.buttonStyle(.borderedProminent).tint(accent)
                    }
                    Button(installed(game) == nil ? "Enter license key…" : "Reinstall latest version") {
                        if installed(game) == nil { key = ""; keyFor = game }
                        else { model.run(["didim", "activate", game.id], title: "Updating " + game.title + "…") }
                    }.disabled(model.busy)
                    if let steam = game.steam, let url = URL(string: steam) {
                        Link("Windows on Steam", destination: url).font(.system(size: 12))
                    }
                }.padding(.top, 4)
            }
            Spacer(minLength: 0)
        }.padding(18).background(Color.primary.opacity(0.04), in: RoundedRectangle(cornerRadius: 16))
    }

    private func keySheet(_ game: StoreGame) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Install " + game.title).font(.system(size: 18, weight: .bold))
            Text("Paste the license key from your purchase email (it starts with DSTC-). Soju checks it, downloads the game and installs it.")
                .font(.system(size: 12)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            TextField("License key", text: $key).textFieldStyle(.roundedBorder).font(.system(.body, design: .monospaced))
            HStack {
                Spacer()
                Button("Cancel") { keyFor = nil }
                Button("Install") {
                    let value = key.trimmingCharacters(in: .whitespacesAndNewlines)
                    keyFor = nil
                    model.run(["didim", "activate", game.id], title: "Installing " + game.title + "…", environment: ["SOJU_DIDIM_KEY": value])
                }.buttonStyle(.borderedProminent).keyboardShortcut(.defaultAction)
                    .disabled(key.trimmingCharacters(in: .whitespacesAndNewlines).range(of: "^[A-Za-z0-9-]{8,100}$", options: .regularExpression) == nil)
            }
        }.padding(24).frame(width: 460)
    }
}
