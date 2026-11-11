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
    let demo: StoreDemo?
}

/// A free demo: its own game ID, a zip on the Soju GitHub releases, and its size and SHA-256.
struct StoreDemo: Decodable {
    let id: String
    let url: String
    let size: Int
    let sha256: String
    var megabytes: String { String(max(1, size / 1_000_000)) + " MB" }
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
                && (game.demo == nil || validDemo(game.demo!, of: game.id))
        }
    }
    static func validDemo(_ demo: StoreDemo, of id: String) -> Bool {
        demo.id != id && demo.id.range(of: "^[a-z0-9][a-z0-9-]{0,63}$", options: .regularExpression) != nil
            && demo.url.hasPrefix("https://github.com/BCD1210/soju/releases/download/") && demo.url.hasSuffix(".zip")
            && demo.size > 0 && demo.sha256.range(of: "^[0-9a-f]{64}$", options: .regularExpression) != nil
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
    @StateObject private var purchase = PurchaseWatcher()
    @StateObject private var account = AccountModel()
    @State private var keyFor: StoreGame?
    @State private var key = ""
    private let accent = Color(red: 0.79, green: 0.36, blue: 0.27)

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            VStack(alignment: .leading, spacing: 7) {
                Text("Didim Labs Store").font(.system(size: 30, weight: .bold, design: .rounded))
                Text("Games we make at Didim Labs. Mac builds for M1 and later, no Wine needed.")
                    .font(.callout).foregroundStyle(.secondary)
            }
            accountBar
            if model.busy || !model.activity.isEmpty && model.activity != "Ready." {
                HStack(spacing: 8) {
                    if model.busy { ProgressView().controlSize(.small) }
                    else { Image(systemName: model.failed ? "exclamationmark.circle" : "checkmark.circle").foregroundStyle(model.failed ? .orange : .green) }
                    Text(model.busy ? lastLine : (model.failed ? lastLine : model.activity)).font(.system(size: 12)).lineLimit(2)
                }
            }
            if let pending = purchase.pending {
                HStack(spacing: 10) {
                    ProgressView().controlSize(.small)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(pending.game.isEmpty ? "Getting your purchase from the website…" : "Finish paying for " + pending.title + " in your browser.").font(.system(size: 12, weight: .semibold))
                        Text(purchase.message.isEmpty ? "Soju installs it by itself as soon as the payment goes through." : purchase.message)
                            .font(.system(size: 11)).foregroundStyle(.secondary)
                    }
                    Spacer()
                    Button("Open checkout") { NSWorkspace.shared.open(pending.url) }
                    Button("Cancel") { purchase.cancel() }
                }.padding(12).background(accent.opacity(0.1), in: RoundedRectangle(cornerRadius: 10))
            } else if !purchase.message.isEmpty {
                Text(purchase.message).font(.system(size: 12)).foregroundStyle(.secondary)
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
            NotifyRow()
            HStack(spacing: 6) {
                Text("Press Buy, pay in your browser, and Soju installs the game by itself.")
                if let game = store.games.first(where: { installed($0) == nil }) {
                    Button("Have a license key?") { key = ""; keyFor = game }.buttonStyle(.link)
                }
            }.font(.system(size: 11)).foregroundStyle(.secondary)
        }
        .padding(28).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .onAppear { store.load(root: model.root); account.loadProviders(); buyRequested(); claimRequested() }
        .onChange(of: model.requestedBuy) { buyRequested() }
        .onChange(of: model.requestedClaim) { claimRequested() }
        .onChange(of: model.requestedAuth) {
            guard let handoff = model.requestedAuth, handoff.count == 2 else { return }
            model.requestedAuth = nil
            account.received(state: handoff[0], code: handoff[1])
        }
        .onChange(of: store.games.count) { buyRequested() }
        .sheet(item: $keyFor) { game in keySheet(game) }
    }

    private var lastLine: String {
        model.log.split(separator: "\n").last.map { String($0).replacingOccurrences(of: "soju: ", with: "") } ?? model.activity
    }

    private func installed(_ game: StoreGame) -> InstalledGame? {
        installed(id: game.id)
    }
    private func installed(id: String) -> InstalledGame? {
        model.games.first { $0.id == "didim:" + id && $0.isInstalled }
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
                    } else if account.owned.contains(game.id) {
                        Button { model.run(["didim", "activate", game.id], title: "Installing " + game.title + "…") } label: {
                            Label("Install", systemImage: "arrow.down.circle.fill")
                        }.buttonStyle(.borderedProminent).tint(.green).disabled(model.busy)
                    } else {
                        Button { buy(game) } label: {
                            Label("Buy · " + game.price, systemImage: "cart")
                        }.buttonStyle(.borderedProminent).tint(accent).disabled(purchase.pending != nil)
                        if let demo = game.demo {
                            if let demoGame = installed(id: demo.id) {
                                Button { model.play(demoGame) } label: { Label("Play demo", systemImage: "play") }
                                    .disabled(model.launchingGames.contains(demoGame.id))
                            } else {
                                Button { model.run(["didim", "demo", game.id], title: "Downloading the " + game.title + " demo…") } label: {
                                    Label("Free demo · " + demo.megabytes, systemImage: "arrow.down.circle")
                                }.disabled(model.busy).help("The first nights of the game, free")
                            }
                        }
                    }
                    if installed(game) != nil {
                        Button("Reinstall") { model.run(["didim", "activate", game.id], title: "Updating " + game.title + "…") }
                            .disabled(model.busy)
                    }
                    if installed(game) != nil, let demo = game.demo, installed(id: demo.id) != nil {
                        Button("Remove demo") { model.run(["didim", "remove", demo.id], title: "Removing the demo…") }
                            .disabled(model.busy)
                    }
                    if let steam = game.steam, let url = URL(string: steam) {
                        Link("Windows: Steam", destination: url).font(.system(size: 12))
                    }
                }.padding(.top, 4)
            }
            Spacer(minLength: 0)
        }.padding(18).background(Color.primary.opacity(0.04), in: RoundedRectangle(cornerRadius: 16))
    }

    private var accountBar: some View {
        HStack(spacing: 10) {
            if account.signingIn != nil {
                ProgressView().controlSize(.small)
                Text("Finish signing in in your browser.").font(.system(size: 12))
                Button("Cancel") { account.cancel() }
            } else if !account.email.isEmpty {
                Image(systemName: "person.crop.circle.fill").foregroundStyle(.secondary)
                Text("Signed in as " + account.email).font(.system(size: 12))
                Button("Sign out") { account.signOut() }
            } else if !account.providers.isEmpty {
                Text("Bought a game already?").font(.system(size: 12)).foregroundStyle(.secondary)
                if account.providers.contains("apple") {
                    Button { account.signIn("apple") } label: { Label("Sign in with Apple", systemImage: "apple.logo") }
                }
                if account.providers.contains("google") {
                    Button { account.signIn("google") } label: { Label("Sign in with Google", systemImage: "g.circle") }
                }
            }
            if !account.message.isEmpty && account.signingIn == nil {
                Text(account.message).font(.system(size: 11)).foregroundStyle(.secondary).lineLimit(2)
            }
            Spacer()
        }
    }

    /// From a demo's "Get the full game": start the checkout once the store has loaded that game.
    private func buyRequested() {
        guard let id = model.requestedBuy, let game = store.games.first(where: { $0.id == id }) else { return }
        model.requestedBuy = nil
        if installed(game) == nil && !account.owned.contains(game.id) && purchase.pending == nil { buy(game) }
    }

    /// A purchase made on the website: pick up its key with the checkout ID and install.
    private func claimRequested() {
        guard let checkout = model.requestedClaim else { return }
        model.requestedClaim = nil
        purchase.resume(checkout) { id, title, key in
            account.markOwned(id)
            install(id: id, title: store.games.first { $0.id == id }?.title ?? title, key: key)
        }
    }

    private func buy(_ game: StoreGame) {
        purchase.start(game, fallback: StoreCatalog.safe(game.checkout, hosts: ["buy.polar.sh", "polar.sh"])) { id, title, key in
            account.markOwned(id)
            install(id: id, title: title, key: key)
        }
    }

    /// Runs as soon as Soju is free; the key goes to the Keychain inside didim_games.py.
    private func install(id: String, title: String, key: String) {
        if model.busy {
            DispatchQueue.main.asyncAfter(deadline: .now() + 3) { install(id: id, title: title, key: key) }
            return
        }
        model.run(["didim", "activate", id], title: "Installing " + title + "…", environment: ["SOJU_DIDIM_KEY": key])
    }

    private func keySheet(_ game: StoreGame) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Install " + game.title).font(.system(size: 18, weight: .bold))
            Text("Paste the key from the Polar receipt email. It starts with DSTC-.")
                .font(.system(size: 12)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            TextField("License key", text: $key).textFieldStyle(.roundedBorder).font(.system(.body, design: .monospaced))
            Link("Lost the email? Find your key on Polar", destination: URL(string: "https://polar.sh/didim-labs/portal")!)
                .font(.system(size: 11))
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

/// Buying from inside Soju: the site worker opens a Polar checkout for this Soju only, and once the
/// payment clears it hands back the license key, so nobody has to copy it out of an email.
@MainActor final class PurchaseWatcher: ObservableObject {
    struct Pending: Equatable { let game: String; let title: String; let id: String; let url: URL }
    @Published var pending: Pending?
    @Published var message = ""
    private var timer: Timer?
    private var started = Date()
    private static let api = "https://didimlabs.com/api/didim/checkout"

    func start(_ game: StoreGame, fallback: URL?, ready: @escaping (String, String, String) -> Void) {
        message = "Opening checkout…"
        var request = URLRequest(url: URL(string: Self.api)!)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try? JSONSerialization.data(withJSONObject: ["game": game.id])
        request.timeoutInterval = 20
        URLSession.shared.dataTask(with: request) { data, response, _ in
            let body = data.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
            DispatchQueue.main.async {
                guard (response as? HTTPURLResponse)?.statusCode == 200, let id = body?["id"] as? String,
                      id.range(of: "^[0-9a-f-]{36}$", options: .regularExpression) != nil,
                      let link = (body?["url"] as? String).flatMap({ StoreCatalog.safe($0, hosts: ["polar.sh", "buy.polar.sh"]) }) else {
                    // The old way still works: Polar emails the key.
                    self.message = ""
                    if let fallback { NSWorkspace.shared.open(fallback) }
                    return
                }
                self.pending = Pending(game: game.id, title: game.title, id: id, url: link)
                self.message = ""
                self.started = Date()
                NSWorkspace.shared.open(link)
                self.timer?.invalidate()
                self.timer = Timer.scheduledTimer(withTimeInterval: 4, repeats: true) { [weak self] _ in
                    DispatchQueue.main.async { self?.poll(ready: ready) }
                }
            }
        }.resume()
    }

    func cancel() {
        timer?.invalidate(); timer = nil; pending = nil; message = ""
    }

    /// Watch a checkout that was opened somewhere else (the website).
    func resume(_ id: String, ready: @escaping (String, String, String) -> Void) {
        pending = Pending(game: "", title: "your game", id: id, url: URL(string: "https://didimlabs.com/store/thanks.html")!)
        message = "Checking your purchase…"
        started = Date()
        timer?.invalidate()
        poll(ready: ready)
        timer = Timer.scheduledTimer(withTimeInterval: 4, repeats: true) { [weak self] _ in
            DispatchQueue.main.async { self?.poll(ready: ready) }
        }
    }

    private func poll(ready: @escaping (String, String, String) -> Void) {
        guard let pending else { cancel(); return }
        if Date().timeIntervalSince(started) > 45 * 60 {
            cancel(); message = "Stopped waiting for the payment. If you paid, use the key from the Polar email."
            return
        }
        var request = URLRequest(url: URL(string: Self.api + "/" + pending.id)!)
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.timeoutInterval = 15
        URLSession.shared.dataTask(with: request) { data, _, _ in
            let body = data.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
            DispatchQueue.main.async {
                guard self.pending == pending, let status = body?["status"] as? String else { return }
                switch status {
                case "ready":
                    let game = pending.game.isEmpty ? ((body?["game"] as? String) ?? "") : pending.game
                    if let key = body?["key"] as? String, key.range(of: "^[A-Za-z0-9-]{8,100}$", options: .regularExpression) != nil,
                       game.range(of: "^[a-z0-9][a-z0-9-]{0,63}$", options: .regularExpression) != nil {
                        self.cancel()
                        ready(game, pending.title, key)
                    }
                case "processing": self.message = "Payment received. Getting your license key…"
                case "expired", "failed":
                    self.cancel(); self.message = "The checkout " + status + ". Press Buy to try again."
                default: break
                }
            }
        }.resume()
    }
}

/// Sign in with Apple or Google (through didimlabs.com) to find games bought with that email.
/// The keys go straight into the Keychain; Soju remembers only the email and which games are owned.
@MainActor final class AccountModel: ObservableObject {
    @Published var email = UserDefaults.standard.string(forKey: "didimAccountEmail") ?? ""
    @Published var owned = Set(UserDefaults.standard.stringArray(forKey: "didimOwned") ?? [])
    @Published var providers: [String] = []
    @Published var signingIn: String?
    @Published var message = ""
    private var timer: Timer?
    private var state = ""
    /// One-time code the website hands this Soju through soju://auth/done; the poll needs it.
    private var code = ""
    private var started = Date()
    private static let site = "https://didimlabs.com"
    static let keychainService = "Soju Didim Labs license"

    func loadProviders() {
        var request = URLRequest(url: URL(string: Self.site + "/auth/providers")!)
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.timeoutInterval = 15
        URLSession.shared.dataTask(with: request) { data, _, _ in
            let list = (data.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }?["providers"] as? [String]) ?? []
            DispatchQueue.main.async { self.providers = list.filter { ["apple", "google"].contains($0) } }
        }.resume()
    }

    func signIn(_ provider: String) {
        var bytes = [UInt8](repeating: 0, count: 32)
        guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess else { return }
        state = Data(bytes).base64EncodedString().replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
        var parts = URLComponents(string: Self.site + "/auth/start")!
        parts.queryItems = [URLQueryItem(name: "provider", value: provider), URLQueryItem(name: "state", value: state)]
        guard let url = parts.url else { return }
        signingIn = provider; message = ""; started = Date(); code = ""
        NSWorkspace.shared.open(url)
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: 2, repeats: true) { [weak self] _ in
            DispatchQueue.main.async { self?.poll() }
        }
    }

    func cancel() { timer?.invalidate(); timer = nil; signingIn = nil }

    /// The browser opened soju://auth/done for the sign-in this Soju started.
    func received(state: String, code: String) {
        guard signingIn != nil, state == self.state else { return }
        self.code = code
        poll()
    }

    func signOut() {
        cancel()
        email = ""; owned = []; message = "Signed out. Installed games stay installed."
        UserDefaults.standard.removeObject(forKey: "didimAccountEmail")
        UserDefaults.standard.removeObject(forKey: "didimOwned")
    }

    /// A game bought in this Soju counts as owned even without signing in.
    func markOwned(_ game: String) {
        owned.insert(game)
        UserDefaults.standard.set(Array(owned), forKey: "didimOwned")
    }

    private func poll() {
        guard signingIn != nil else { return }
        if Date().timeIntervalSince(started) > 10 * 60 { cancel(); message = "Sign-in timed out. Try again."; return }
        let current = state
        var request = URLRequest(url: URL(string: Self.site + "/auth/poll/" + current + "?v=2" + (code.isEmpty ? "" : "&code=" + code))!)
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.timeoutInterval = 15
        URLSession.shared.dataTask(with: request) { data, _, _ in
            let body = data.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
            DispatchQueue.main.async {
                guard self.state == current, let status = body?["status"] as? String else { return }
                switch status {
                case "done": self.cancel(); self.finish(body ?? [:])
                case "failed": self.cancel(); self.message = (body?["error"] as? String) ?? "Sign-in didn't work."
                case "expired": self.cancel(); self.message = "Sign-in expired. Try again."
                default: break
                }
            }
        }.resume()
    }

    private func finish(_ body: [String: Any]) {
        let address = (body["email"] as? String) ?? ""
        var found: [String] = []
        for item in (body["owned"] as? [[String: Any]]) ?? [] {
            guard let game = item["game"] as? String, let key = item["key"] as? String,
                  game.range(of: "^[a-z0-9][a-z0-9-]{0,63}$", options: .regularExpression) != nil,
                  key.range(of: "^[A-Za-z0-9-]{8,100}$", options: .regularExpression) != nil else { continue }
            let save = Process()
            save.executableURL = URL(fileURLWithPath: "/usr/bin/security")
            save.arguments = ["add-generic-password", "-U", "-s", Self.keychainService, "-a", game, "-l", "Soju: " + game, "-w", key]
            if (try? save.run()) != nil { save.waitUntilExit(); if save.terminationStatus == 0 { found.append(game) } }
        }
        email = address
        owned.formUnion(found)
        UserDefaults.standard.set(address, forKey: "didimAccountEmail")
        UserDefaults.standard.set(Array(owned), forKey: "didimOwned")
        if found.isEmpty {
            message = (body["private"] as? Bool) == true
                ? "No purchases for this Apple relay address. Sign in with Google, or with Apple sharing your real email."
                : "No purchases found for " + address + ". Sign in with the email you paid with."
        } else {
            message = "Found " + String(found.count) + (found.count == 1 ? " game" : " games") + " for " + address + "."
        }
    }
}
