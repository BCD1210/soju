import SwiftUI
import AppKit

struct InstalledGame: Decodable, Identifiable {
    let id: String
    let platform: Launcher
    let title: String
    let installPath: String
    let artwork: String?
    let issue: String?
    enum CodingKeys: String, CodingKey {
        case id, platform, title, artwork, issue
        case installPath = "install_path"
    }
}
extension Launcher: Decodable {}
struct LibrarySnapshot: Decodable {
    let games: [InstalledGame]
    let warnings: [String]
}
struct LibraryPreferences: Codable {
    var favorites: Set<String> = []
    var opened: [String: Date] = [:]
    static func load() -> Self {
        guard let data = UserDefaults.standard.data(forKey: "soju.library.v1"),
              let value = try? JSONDecoder().decode(Self.self, from: data) else { return Self() }
        return value
    }
}

extension SojuModel {
    func savePreferences() {
        if let data = try? JSONEncoder().encode(preferences) {
            UserDefaults.standard.set(data, forKey: "soju.library.v1")
        }
    }
    func favorite(_ game: InstalledGame) {
        if preferences.favorites.contains(game.id) { preferences.favorites.remove(game.id) }
        else { preferences.favorites.insert(game.id) }
        savePreferences()
    }
    func refreshLibrary() {
        guard !scanning else { return }
        scanning = true
        do {
            let (p, url) = try task(["library"], logName: "library-scan")
            scanner = p
            p.terminationHandler = { [weak self] process in
                // Decode off the main thread; a library can contain many games.
                let data = (try? Data(contentsOf: url)) ?? Data()
                let result = try? JSONDecoder().decode(LibrarySnapshot.self, from: data)
                DispatchQueue.main.async {
                    guard let self, self.scanner === process else { return }
                    self.scanning = false; self.scanner = nil
                    if process.terminationStatus == 0, let result {
                        self.games = result.games; self.scanWarnings = result.warnings
                    } else {
                        self.scanWarnings = ["Could not refresh your library. Check that Python 3 is installed, then retry."]
                    }
                }
            }
            try p.run()
            DispatchQueue.main.asyncAfter(deadline: .now() + 25) { [weak self, weak p] in
                guard let self, let p, self.scanner === p, p.isRunning else { return }
                p.terminate()
                self.scanning = false; self.scanner = nil
                self.scanWarnings = ["Scanning took too long. Reconnect any external game drives and refresh."]
            }
        } catch {
            scanner = nil; scanning = false
            scanWarnings = ["Could not scan installed games: " + error.localizedDescription]
        }
    }
    func launchIssue(for game: InstalledGame) -> String? {
        if let issue = game.issue { return issue }
        if !FileManager.default.fileExists(atPath: game.installPath) {
            return "This game folder is unavailable. Reconnect its drive and refresh."
        }
        if game.platform == .steam {
            let local = base.appendingPathComponent("steam-runtime/bin/wine").path
            let stable = "/Applications/Wine Stable.app/Contents/Resources/wine/bin/wine"
            if !FileManager.default.isExecutableFile(atPath: local) && !FileManager.default.isExecutableFile(atPath: stable) {
                return "Install Steam in Platforms to prepare its Windows environment."
            }
        } else {
            if !FileManager.default.isExecutableFile(atPath: base.appendingPathComponent("cx26-engine/bin/wine").path) {
                return "Install this platform to prepare its Windows environment."
            }
            if !FileManager.default.fileExists(atPath: base.appendingPathComponent("cx26-engine/lib/external/libd3dshared.dylib").path) {
                return "Add Apple's Game Porting Toolkit in Platforms before playing."
            }
        }
        return nil
    }
    func play(_ game: InstalledGame) {
        guard !busy, !launchingGames.contains(game.id) else { return }
        if let issue = launchIssue(for: game) { libraryIssue = issue; return }
        do {
            let (p, url) = try task(["game", game.id], logName: "game-" + game.id.replacingOccurrences(of: ":", with: "-"))
            launchingGames.insert(game.id); gameLaunches[game.id] = p
            p.terminationHandler = { [weak self] process in
                DispatchQueue.main.async {
                    guard let self else { return }
                    if self.gameLaunches[game.id] === process {
                        self.gameLaunches.removeValue(forKey: game.id)
                        self.launchingGames.remove(game.id)
                    }
                    if process.terminationStatus != 0 {
                        self.libraryIssue = "Could not launch " + game.title + ". Use Diagnose or open " + game.platform.title + " to check its installation and login."
                        if !self.busy {
                            self.failed = true; self.activity = game.title + " needs attention."
                            if let data = try? Data(contentsOf: url) { self.log = String(decoding: data.suffix(60000), as: UTF8.self) }
                        }
                    }
                }
            }
            try p.run()
            failed = false
            preferences.opened[game.id] = Date(); savePreferences()
            activity = "Opening " + game.title + " through " + game.platform.title + "…"
            // Clients may stay alive for hours: this is a launch request, not a game-running signal.
            DispatchQueue.main.asyncAfter(deadline: .now() + 8) { [weak self, weak p] in
                guard let self, let p, self.gameLaunches[game.id] === p else { return }
                self.launchingGames.remove(game.id)
            }
        } catch {
            launchingGames.remove(game.id); gameLaunches.removeValue(forKey: game.id)
            libraryIssue = "Could not start the game: " + error.localizedDescription
        }
    }
}

private extension Launcher {
    var accent: Color {
        switch self {
        case .battlenet: return Color(red: 0.28, green: 0.63, blue: 0.91)
        case .steam: return Color(red: 0.36, green: 0.67, blue: 0.77)
        case .epic: return Color(red: 0.82, green: 0.64, blue: 0.40)
        case .gog: return Color(red: 0.70, green: 0.47, blue: 0.87)
        }
    }
}

struct LibraryView: View {
    @EnvironmentObject var model: SojuModel
    @State private var page = "library"
    @State private var query = ""
    @State private var platform = "all"
    @State private var order = "name"
    @State private var selected: InstalledGame?
    @State private var showDiagnostics = false
    @Environment(\.colorScheme) private var scheme
    private var green: Color { scheme == .dark ? Color(red: 0.42, green: 0.83, blue: 0.67) : Color(red: 0.12, green: 0.34, blue: 0.28) }
    private var visibleGames: [InstalledGame] {
        model.games.filter {
            (page != "favorites" || model.preferences.favorites.contains($0.id)) &&
            (platform == "all" || $0.platform.rawValue == platform) &&
            (query.isEmpty || $0.title.localizedStandardContains(query))
        }.sorted {
            if order == "recent" {
                let a = model.preferences.opened[$0.id] ?? .distantPast
                let b = model.preferences.opened[$1.id] ?? .distantPast
                if a != b { return a > b }
            }
            let comparison = $0.title.localizedStandardCompare($1.title)
            return comparison == .orderedSame ? $0.id < $1.id : comparison == .orderedAscending
        }
    }

    var body: some View {
        HStack(spacing: 0) {
            sidebar
            Divider()
            if page == "platforms" { PlatformsView().frame(maxWidth: .infinity, maxHeight: .infinity) }
            else { library }
        }
        .frame(minWidth: 1010, minHeight: 710)
        .background(Color(nsColor: .windowBackgroundColor))
        .sheet(item: $selected) { game in detail(game) }
        .sheet(isPresented: $showDiagnostics) { diagnostics }
        .alert("Game needs attention", isPresented: Binding(get: { model.libraryIssue != nil }, set: { if !$0 { model.libraryIssue = nil } })) {
            Button("View details") { model.libraryIssue = nil; showDiagnostics = true }
            Button("OK", role: .cancel) { model.libraryIssue = nil }
        } message: { Text(model.libraryIssue ?? "") }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            model.refresh(); model.refreshLibrary()
        }
    }

    private var sidebar: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 10) {
                Image(systemName: "leaf.fill").font(.title2).foregroundStyle(green)
                Text("Soju").font(.system(size: 26, weight: .bold, design: .rounded))
            }.padding(.bottom, 4).padding(.top, 16)
            Text("ALL YOUR GAMES. ONE HOME.")
                .font(.system(size: 8, weight: .semibold)).tracking(1).foregroundStyle(.secondary).padding(.bottom, 30)
            nav("library", "Library", "square.grid.2x2", count: model.games.count)
            nav("favorites", "Favorites", "star", count: model.games.filter { model.preferences.favorites.contains($0.id) }.count)
            Divider().padding(.vertical, 16)
            nav("platforms", "Platforms", "square.stack.3d.up", count: nil)
            Spacer()
            VStack(alignment: .leading, spacing: 9) {
                Label("On your Mac", systemImage: "desktopcomputer").font(.system(size: 12, weight: .medium))
                Text("Installed games appear automatically. Your accounts stay with each platform.")
                    .font(.system(size: 11)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }.padding(13).background(green.opacity(0.07), in: RoundedRectangle(cornerRadius: 12))
            HStack {
                Text("v" + model.version).foregroundStyle(.secondary)
                Spacer()
                Button { NSWorkspace.shared.open(URL(string: "https://github.com/BCD1210/soju/releases")!) } label: {
                    Image(systemName: "arrow.up.right.square")
                }.buttonStyle(.plain).help("Project and app updates")
            }.font(.system(size: 11)).padding(.top, 14)
        }.padding(20).frame(width: 205).background(Color.primary.opacity(0.025))
    }
    private func nav(_ id: String, _ title: String, _ symbol: String, count: Int?) -> some View {
        Button {
            page = id
        } label: {
            HStack(spacing: 11) {
                Image(systemName: symbol).frame(width: 18)
                Text(title)
                Spacer()
                if let count { Text(String(count)).font(.system(size: 11)).opacity(0.65) }
            }.font(.system(size: 13, weight: page == id ? .semibold : .regular))
                .padding(.horizontal, 11).padding(.vertical, 11).contentShape(Rectangle())
                .background(page == id ? green.opacity(0.13) : .clear, in: RoundedRectangle(cornerRadius: 9))
                .foregroundStyle(page == id ? green : Color.primary)
        }.buttonStyle(.plain).accessibilityLabel(title)
    }
    private var library: some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack(alignment: .center) {
                VStack(alignment: .leading, spacing: 7) {
                    Text(page == "favorites" ? "Your favorites" : "Your library")
                        .font(.system(size: 30, weight: .bold, design: .rounded))
                    Text("Pick a game. We’ll open the right launcher.").font(.system(size: 13)).foregroundStyle(.secondary)
                }
                Spacer()
                Button {
                    model.refresh(); model.refreshLibrary()
                } label: {
                    Label(model.scanning ? "Scanning…" : "Refresh", systemImage: "arrow.clockwise")
                }.disabled(model.scanning || model.busy).help("Rescan local installation records")
            }
            HStack(spacing: 12) {
                HStack(spacing: 8) {
                    Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                    TextField("Search your games", text: $query).textFieldStyle(.plain)
                        .accessibilityLabel("Search games")
                    if !query.isEmpty { Button { query = "" } label: { Image(systemName: "xmark.circle.fill") }.buttonStyle(.plain) }
                }.padding(10).background(Color.primary.opacity(0.05), in: RoundedRectangle(cornerRadius: 9))
                Picker("Platform", selection: $platform) {
                    Text("All platforms").tag("all")
                    ForEach(Launcher.allCases) { Text($0.title).tag($0.rawValue) }
                }.labelsHidden().frame(width: 150).accessibilityLabel("Filter by platform")
                Picker("Sort", selection: $order) {
                    Text("Name").tag("name")
                    Text("Recently opened").tag("recent")
                }.labelsHidden().frame(width: 145).accessibilityLabel("Sort games")
            }
            if !model.scanWarnings.isEmpty {
                Label(model.scanWarnings.joined(separator: "\n"), systemImage: "exclamationmark.triangle")
                    .font(.system(size: 11)).foregroundStyle(.orange).textSelection(.enabled)
            }
            if visibleGames.isEmpty {
                Spacer()
                VStack(spacing: 14) {
                    Image(systemName: model.scanning ? "arrow.clockwise" : page == "favorites" ? "star" : "gamecontroller")
                        .font(.system(size: 44, weight: .light)).foregroundStyle(green)
                    Text(model.scanning ? "Finding your games…" : model.games.isEmpty ? "Your library starts here" : "No games to show")
                        .font(.system(size: 21, weight: .semibold))
                    Text(model.games.isEmpty ? "Install a game through one of your platforms, then refresh.\nThis library shows games installed in Soju’s Windows environments." : page == "favorites" ? "Star a game in your library to keep it here." : "Try another search or platform filter.")
                        .font(.system(size: 12)).foregroundStyle(.secondary).multilineTextAlignment(.center)
                    Button(model.games.isEmpty ? "Manage platforms" : "Show all games") {
                        if model.games.isEmpty { page = "platforms" }
                        else { page = "library"; query = ""; platform = "all" }
                    }.buttonStyle(.borderedProminent).tint(green)
                }.frame(maxWidth: .infinity)
                Spacer()
            } else {
                ScrollView {
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 180, maximum: 280), spacing: 18)], spacing: 20) {
                        ForEach(visibleGames) { game in gameCard(game) }
                    }.padding(.vertical, 3)
                }
            }
            HStack(spacing: 8) {
                Circle().fill(model.failed ? Color.orange : green).frame(width: 5, height: 5)
                Text(model.activity == "Ready when you are." ? "\(model.games.count) installed games · Stored locally" : model.activity)
                    .lineLimit(1)
                Spacer()
                Button("Diagnostics") { showDiagnostics = true }.buttonStyle(.plain)
            }.font(.system(size: 11)).foregroundStyle(.secondary)
        }.padding(28).frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func gameCard(_ game: InstalledGame) -> some View {
        let issue = model.launchIssue(for: game)
        return VStack(alignment: .leading, spacing: 0) {
            Button { selected = game } label: { GameArtwork(game: game).frame(height: 205).clipped() }
                .buttonStyle(.plain).accessibilityLabel("Details for " + game.title)
            VStack(alignment: .leading, spacing: 11) {
                HStack(spacing: 5) {
                    Circle().fill(game.platform.accent).frame(width: 5, height: 5)
                    Text(game.platform.title.uppercased()).font(.system(size: 9, weight: .bold)).tracking(0.7).foregroundStyle(.secondary)
                    Spacer()
                    Button { model.favorite(game) } label: {
                        Image(systemName: model.preferences.favorites.contains(game.id) ? "star.fill" : "star")
                            .foregroundStyle(model.preferences.favorites.contains(game.id) ? Color.yellow : Color.secondary)
                    }.buttonStyle(.plain).accessibilityLabel((model.preferences.favorites.contains(game.id) ? "Unfavorite " : "Favorite ") + game.title)
                }
                Text(game.title).font(.system(size: 14, weight: .semibold)).lineLimit(2)
                    .frame(height: 36, alignment: .topLeading).frame(maxWidth: .infinity, alignment: .leading)
                HStack {
                    Text(issue == nil ? "Installed" : "Needs attention").font(.system(size: 10))
                        .foregroundStyle(issue == nil ? Color.secondary : Color.orange)
                    Spacer()
                    Button {
                        if issue == nil { model.play(game) } else { selected = game }
                    } label: {
                        Label(model.launchingGames.contains(game.id) ? "Starting…" : issue == nil ? "Play" : "Details", systemImage: issue == nil ? "play.fill" : "exclamationmark.circle")
                    }.buttonStyle(.borderedProminent).tint(green).controlSize(.small)
                        .disabled(model.busy || model.launchingGames.contains(game.id))
                        .accessibilityLabel((issue == nil ? "Play " : "Check ") + game.title)
                }
            }.padding(14)
        }.background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 14))
            .clipShape(RoundedRectangle(cornerRadius: 14))
            .overlay(RoundedRectangle(cornerRadius: 14).stroke(Color.primary.opacity(0.07), lineWidth: 1))
            .contextMenu {
                Button("Show details") { selected = game }
                Button("Show in Finder") { NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: game.installPath) }
                Button("Open " + game.platform.title) { model.launch(game.platform) }.disabled(model.busy)
                Button("Diagnose") { diagnose(game) }.disabled(model.busy)
            }
    }

    private func diagnose(_ game: InstalledGame) {
        selected = nil
        model.run(["game-doctor", game.id], title: "Checking " + game.title + "…")
        showDiagnostics = true
    }
    private func detail(_ game: InstalledGame) -> some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack(alignment: .top, spacing: 22) {
                GameArtwork(game: game).frame(width: 165, height: 225).clipped().clipShape(RoundedRectangle(cornerRadius: 12))
                VStack(alignment: .leading, spacing: 13) {
                    Text(game.platform.title).font(.system(size: 11, weight: .semibold)).foregroundStyle(game.platform.accent)
                    Text(game.title).font(.system(size: 24, weight: .bold, design: .rounded))
                    Label("Installed on this Mac", systemImage: "internaldrive").font(.system(size: 12)).foregroundStyle(.secondary)
                    Text("Opens through " + game.platform.title + ". You may need to sign in or finish an update in the launcher.")
                        .font(.system(size: 12)).foregroundStyle(.secondary)
                    if let issue = model.launchIssue(for: game) {
                        Label(issue, systemImage: "exclamationmark.triangle").font(.system(size: 12)).foregroundStyle(.orange)
                    }
                    Text("Installation detected. Compatibility varies by game and Mac.")
                        .font(.system(size: 11)).foregroundStyle(.secondary)
                    Spacer()
                }
            }
            HStack {
                Button("Show files") { NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: game.installPath) }
                Button("Diagnose") { diagnose(game) }.disabled(model.busy)
                Button("Open " + game.platform.title) { model.launch(game.platform) }.disabled(model.busy)
                Spacer()
            }
            Divider()
            HStack {
                Button("Platforms") { selected = nil; page = "platforms" }
                Spacer()
                Button("Close") { selected = nil }.keyboardShortcut(.cancelAction)
                Button("Play") { selected = nil; model.play(game) }
                    .buttonStyle(.borderedProminent).tint(green)
                    .disabled(model.busy || model.launchIssue(for: game) != nil || model.launchingGames.contains(game.id))
            }
        }.padding(28).frame(width: 640)
    }
    private var diagnostics: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Text("Diagnostics").font(.title2.bold())
                Spacer()
                Button("Done") { showDiagnostics = false }.keyboardShortcut(.cancelAction)
            }
            HStack {
                if model.busy { ProgressView().controlSize(.small) }
                Text(model.activity).font(.system(size: 12)).foregroundStyle(model.failed ? Color.orange : Color.secondary)
            }
            ScrollView {
                Text(model.log).font(.system(size: 11, design: .monospaced)).textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }.padding(12).background(Color.primary.opacity(0.04), in: RoundedRectangle(cornerRadius: 9))
            HStack {
                Button("Check all platforms") { model.run(["doctor"], title: "Checking your setup…") }.disabled(model.busy)
                Spacer()
                Button("Copy log", action: model.copyLog)
                Button("Open logs") { NSWorkspace.shared.open(model.base.appendingPathComponent("logs")) }
            }
        }.padding(24).frame(width: 720, height: 480)
    }
}

struct GameArtwork: View {
    let game: InstalledGame
    @State private var artwork: NSImage?
    private static let cache: NSCache<NSString, NSImage> = {
        let value = NSCache<NSString, NSImage>(); value.countLimit = 150
        value.totalCostLimit = 64 * 1024 * 1024; return value
    }()
    var body: some View {
        ZStack {
            LinearGradient(colors: [game.platform.accent.opacity(0.55), Color(red: 0.06, green: 0.09, blue: 0.13)], startPoint: .topLeading, endPoint: .bottomTrailing)
            if let artwork {
                if game.artwork?.lowercased().hasSuffix(".ico") == true {
                    Image(nsImage: artwork).resizable().scaledToFit().padding(38).shadow(color: .black.opacity(0.35), radius: 16, y: 8)
                } else {
                    GeometryReader { proxy in
                        Image(nsImage: artwork).resizable().scaledToFill().frame(width: proxy.size.width, height: proxy.size.height).clipped()
                    }
                }
            } else {
                VStack(spacing: 18) {
                    Image(systemName: game.platform.symbol).font(.system(size: 38, weight: .light)).opacity(0.75)
                    Text(game.title).font(.system(size: 20, weight: .bold, design: .rounded)).multilineTextAlignment(.center).lineLimit(3)
                }.foregroundStyle(.white).padding(22)
            }
        }.accessibilityHidden(true)
        .task(id: game.artwork) {
            artwork = nil
            guard let path = game.artwork else { return }
            if let cached = Self.cache.object(forKey: path as NSString) { artwork = cached; return }
            let result: NSImage? = await Task.detached(priority: .utility) {
                guard let attributes = try? FileManager.default.attributesOfItem(atPath: path),
                      let size = attributes[.size] as? NSNumber, size.intValue < 16 * 1024 * 1024 else { return nil }
                return NSImage(contentsOfFile: path)
            }.value
            guard !Task.isCancelled else { return }
            if let result { Self.cache.setObject(result, forKey: path as NSString, cost: Int(result.size.width * result.size.height * 4)) }
            artwork = result
        }
    }
}
