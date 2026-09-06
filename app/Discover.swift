import SwiftUI
import AppKit

struct StoreItem: Decodable, Identifiable {
    let id: String
    let platform: Launcher
    let title: String
    let url: String
    let artwork: String?
    let price: String
    let mac: Bool
    let windows: Bool
}
struct StoreResponse: Decodable {
    let games: [StoreItem]
    let warnings: [String]
    let country: String
}

@MainActor final class DiscoverModel: ObservableObject {
    @Published var results: [StoreItem] = []
    @Published var warnings: [String] = []
    @Published var searching = false
    @Published var searchedQuery = ""
    @Published var searchedCountry = ""
    private var task: Process?
    private var generation = UUID()
    func search(_ query: String, country: String, app: SojuModel) {
        let query = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard query.count >= 2, query.count <= 100 else { warnings = ["Enter 2–100 characters to search."]; return }
        generation = UUID()
        let current = generation
        task?.terminate()
        searching = true; results = []; warnings = []
        searchedQuery = query; searchedCountry = country
        task = ServiceBridge.start(root: app.root, base: app.base, args: ["search", query, "--country", country]) { [weak self] result in
            guard let self, self.generation == current else { return }
            self.searching = false; self.task = nil
            switch result {
            case .success(let data):
                do {
                    let value = try JSONDecoder().decode(StoreResponse.self, from: data)
                    self.results = value.games; self.warnings = value.warnings
                } catch { self.warnings = ["The stores returned an unreadable response. Please retry."] }
            case .failure(let error): self.warnings = [error.localizedDescription]
            }
        }
    }
    func cancel() {
        generation = UUID(); task?.terminate(); task = nil; searching = false
    }
}

struct DiscoverView: View {
    @EnvironmentObject var model: SojuModel
    @StateObject private var store = DiscoverModel()
    @State private var query = ""
    @State private var country = "US"
    @State private var platform = "all"
    @State private var purchaseLinks = PurchaseLinks.empty
    private var shown: [StoreItem] { store.results.filter { platform == "all" || $0.platform.rawValue == platform } }
    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack {
                VStack(alignment: .leading, spacing: 7) {
                    Text("Find your next game").font(.system(size: 30, weight: .bold, design: .rounded))
                    Text("Search Steam and GOG. Compare games and prices.").font(.callout).foregroundStyle(.secondary)
                }
                Spacer()
            }
            HStack(spacing: 10) {
                TextField("Search games and add-ons", text: $query).textFieldStyle(.roundedBorder).accessibilityLabel("Search stores")
                    .onSubmit { store.search(query, country: country, app: model) }
                Picker("Region", selection: $country) {
                    Text("US · USD").tag("US"); Text("Korea · KRW").tag("KR"); Text("UK · GBP").tag("GB")
                    Text("Germany · EUR").tag("DE"); Text("Japan · JPY").tag("JP")
                }.labelsHidden().frame(width: 145).accessibilityLabel("Store region")
                Button("Search") { store.search(query, country: country, app: model) }
                    .buttonStyle(.borderedProminent).disabled(query.trimmingCharacters(in: .whitespacesAndNewlines).count < 2)
            }
            HStack {
                Picker("Store", selection: $platform) {
                    Text("Steam & GOG").tag("all"); Text("Steam").tag("steam"); Text("GOG").tag("gog")
                }.pickerStyle(.segmented).frame(width: 270).accessibilityLabel("Store results filter")
                Spacer()
                if store.searching {
                    ProgressView().controlSize(.small)
                    Button("Cancel") { store.cancel() }
                } else if !store.searchedQuery.isEmpty {
                    Text(String(shown.count) + " results · " + store.searchedCountry).font(.caption).foregroundStyle(.secondary)
                }
            }
            if !store.searchedQuery.isEmpty {
                Text("Results for “" + store.searchedQuery + "”").font(.callout.bold())
            }
            if !store.warnings.isEmpty {
                Text(store.warnings.joined(separator: "\n")).font(.caption).foregroundStyle(.orange).textSelection(.enabled)
            }
            if shown.isEmpty {
                VStack(spacing: 14) {
                    Image(systemName: "sparkle.magnifyingglass").font(.system(size: 42, weight: .light))
                    Text(store.searching ? "Searching the stores…" : store.searchedQuery.isEmpty ? "What would you like to play?" : "No matching listings")
                        .font(.title3.bold())
                    Text("Purchases and downloads stay with the store.").foregroundStyle(.secondary)
                }.frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 285), spacing: 16)], spacing: 16) {
                        ForEach(shown) { item in listing(item) }
                    }.padding(.vertical, 3)
                }
            }
            if purchaseLinks.hasAffiliateLinks {
                Text("Links marked ‘Supports Soju’ may earn us a commission if you buy, at no extra cost to you.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Divider()
            Text("Prices are for the selected region and may change at checkout. Listings can include add-ons and bundles. Store availability does not confirm compatibility with Soju.")
                .font(.caption).foregroundStyle(.secondary)
        }.padding(28).onAppear { purchaseLinks = PurchaseLinks.load(root: model.root) }.onDisappear { store.cancel() }
    }
    private func listing(_ item: StoreItem) -> some View {
        let owned = model.games.first { $0.id == item.id }
        let purchase = purchaseLinks.resolve(item.url)
        return VStack(alignment: .leading, spacing: 12) {
            ZStack {
                Color.primary.opacity(0.04)
                AsyncImage(url: item.artwork.flatMap(URL.init(string:))) { image in
                    image.resizable().scaledToFit()
                } placeholder: { Image(systemName: item.platform.symbol).font(.largeTitle).foregroundStyle(.secondary) }
            }.frame(height: 120).clipShape(RoundedRectangle(cornerRadius: 9))
            HStack {
                Text(item.platform == .steam ? "STEAM" : "GOG").font(.caption.bold()).foregroundStyle(.secondary)
                Spacer()
                if let owned { Text(owned.isInstalled ? "Installed" : "In your library").font(.caption).foregroundStyle(.green) }
            }
            Text(item.title).font(.headline).lineLimit(2).frame(height: 38, alignment: .topLeading)
            HStack {
                Text(item.price).font(.system(size: 17, weight: .semibold))
                Spacer()
                if item.mac { Text("macOS listed").font(.caption).foregroundStyle(.secondary) }
            }
            HStack {
                if let owned {
                    Button(owned.isInstalled ? "Play" : owned.platform == .steam ? "Install in Steam" : "Open Galaxy") {
                        if owned.isInstalled { model.play(owned) } else { model.installOwnedGame(owned) }
                    }.disabled(model.busy)
                }
                Spacer()
                if let purchase {
                    VStack(alignment: .trailing, spacing: 4) {
                        Button("View in store") { NSWorkspace.shared.open(purchase.url) }
                            .buttonStyle(.borderedProminent)
                        if purchase.isAffiliate {
                            Text("Supports Soju").font(.caption2).foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }.padding(16).frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 13))
    }
}
