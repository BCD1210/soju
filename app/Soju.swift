import SwiftUI
import AppKit
import UniformTypeIdentifiers

enum Launcher: String, CaseIterable, Identifiable {
    case battlenet, steam, epic, gog, ea, rockstar, ubisoft, xbox, didim
    var id: String { rawValue }
    /// Platforms with something to install on the Platforms page (not native Didim Labs games).
    static var installable: [Launcher] { allCases.filter { $0 != .didim } }
    /// Games that run as a task of Soju until they quit, instead of through an open launcher.
    var runsInSoju: Bool { self == .xbox || self == .didim }
    var title: String {
        switch self {
        case .battlenet: return "Battle.net"
        case .steam: return "Steam"
        case .epic: return "Epic Games"
        case .gog: return "GOG GALAXY"
        case .ea: return "EA app"
        case .rockstar: return "Rockstar Games"
        case .ubisoft: return "Ubisoft Connect"
        case .xbox: return "Xbox PC"
        case .didim: return "Didim Labs"
        }
    }
    var detail: String {
        switch self {
        case .steam: return "Windows library · D3D11 with DXMT"
        case .ea, .rockstar, .ubisoft: return "Windows library · DXMT, new"
        case .xbox: return "Microsoft Store library · experimental"
        case .didim: return "Native Mac games"
        default: return "Windows library · Apple D3DMetal"
        }
    }
    /// Xbox PC has no Windows client to open; sign-in and downloads run in Soju.
    var opensAWindow: Bool { self != .xbox && self != .didim }
    var accountName: String {
        switch self {
        case .steam: return "Steam account"
        case .gog: return "GOG account"
        case .ea: return "EA account"
        case .rockstar: return "Rockstar Games account"
        case .ubisoft: return "Ubisoft account"
        case .epic: return "Epic account"
        case .battlenet: return "Blizzard account"
        case .xbox: return "Microsoft account"
        case .didim: return "Didim Labs"
        }
    }
    /// Action for an owned game that is not on this Mac yet.
    var ownedActionTitle: String {
        switch self {
        case .steam: return "Install"
        case .gog: return "Galaxy"
        case .xbox: return "Download"
        case .didim: return "Install"
        default: return "Open"
        }
    }
    var symbol: String {
        switch self {
        case .battlenet: return "bolt.circle.fill"
        case .steam: return "gamecontroller.fill"
        case .epic: return "shield.lefthalf.filled"
        case .gog: return "square.grid.2x2.fill"
        case .ea: return "e.circle.fill"
        case .rockstar: return "star.circle.fill"
        case .ubisoft: return "u.circle.fill"
        case .xbox: return "xbox.logo"
        case .didim: return "cat.fill"
        }
    }
    var executable: String {
        switch self {
        case .battlenet: return "bottle/drive_c/Program Files (x86)/Battle.net/Battle.net.exe"
        case .steam: return "steam-bottle/drive_c/Program Files (x86)/Steam/steam.exe"
        case .epic: return "epic-bottle/drive_c/Program Files/Epic Games/Launcher/Portal/Binaries/Win64/EpicGamesLauncher.exe"
        case .gog: return "gog-bottle/drive_c/Program Files/GOG Galaxy/GalaxyClient.exe"
        // The client sits behind a junction only Wine resolves; the folder is the install.
        case .ea: return "ea-bottle/drive_c/Program Files/Electronic Arts/EA Desktop"
        case .rockstar: return "rockstar-bottle/drive_c/Program Files/Rockstar Games/Launcher/Launcher.exe"
        case .ubisoft: return "ubisoft-bottle/drive_c/Program Files (x86)/Ubisoft/Ubisoft Game Launcher/UbisoftConnect.exe"
        case .xbox: return "xbox-bottle/drive_c/ProgramData/Soju/xbox.json"
        case .didim: return "didim-games"
        }
    }
}

/// The app icon (hand-drawn soju bottle and glass), used as the logo inside the app.
struct SojuLogo: View {
    var size: CGFloat
    /// The bottle-and-glass drawing without the icon tile; falls back to the app icon.
    static let mark: NSImage = Bundle.main.resourceURL.flatMap {
        NSImage(contentsOf: $0.appendingPathComponent("soju/resources/soju-mark.png"))
    } ?? NSApp.applicationIconImage
    var body: some View {
        Image(nsImage: Self.mark).resizable().interpolation(.high)
            .frame(width: size, height: size).accessibilityHidden(true)
    }
}

@MainActor final class SojuModel: ObservableObject {
    @Published var accountSummaries: [AccountSummary] = []
    @Published var accountBusy = false
    @Published var accountMessage = ""
    var accountTask: Process?
    @Published var games: [InstalledGame] = []
    @Published var scanning = false
    @Published var scanWarnings: [String] = []
    @Published var xboxLocation: XboxLocation?
    @Published var launchingGames: Set<String> = []
    @Published var stoppingGames: Set<String> = []
    @Published var libraryIssue: String?
    @Published var preferences = LibraryPreferences.load()
    var gameLaunches: [String: Process] = [:]
    var scanner: Process?
    @Published var installed: Set<Launcher> = []
    @Published var busy = false
    @Published var activity = "Ready."
    @Published var log = "Choose a launcher to get started. Existing installations are detected automatically."
    @Published var failed = false
    @Published var selection: Set<Launcher> = [.battlenet]
    @Published var launching: Set<Launcher> = []
    @Published var requestedPage: String?
    /// A game the store should start checking out, from a soju://store/<game>?buy=1 link.
    @Published var requestedBuy: String?
    /// A checkout paid on the website, from the thanks page's "Install in Soju" button.
    @Published var requestedClaim: String?
    /// The website's sign-in result handoff, from soju://auth/done?state=…&code=… ([state, code]).
    @Published var requestedAuth: [String]?

    func open(_ url: URL) {
        if url.scheme == "soju", url.host == "auth", url.path == "/done" {
            let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
            let state = items.first { $0.name == "state" }?.value ?? "", code = items.first { $0.name == "code" }?.value ?? ""
            let safe = "^[A-Za-z0-9_-]{32,64}$"
            if state.range(of: safe, options: .regularExpression) != nil, code.range(of: safe, options: .regularExpression) != nil {
                requestedPage = "store"; requestedAuth = [state, code]
            }
            return
        }
        guard url.scheme == "soju", url.host == "store" else { return }
        requestedPage = "store"
        let game = url.pathComponents.dropFirst().first ?? ""
        let buy = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems?.contains { $0.name == "buy" && $0.value == "1" } == true
        if buy, game.range(of: "^[a-z0-9][a-z0-9-]{0,63}$", options: .regularExpression) != nil { requestedBuy = game }
        if let checkout = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems?.first(where: { $0.name == "checkout" })?.value,
           checkout.range(of: "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$", options: .regularExpression) != nil {
            requestedClaim = checkout
        }
    }
    @Published var downloads: [String: String] = [:]
    var downloadTasks: [String: Process] = [:]
    var downloadTimers: [String: Timer] = [:]
    var stoppingDownloads: Set<String> = []
    @Published var signInTask: Process?
    @Published var issueTitle = "Game needs attention"
    let base = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".battlenet-macos")
    let root = Bundle.main.resourceURL!.appendingPathComponent("soju")
    /// Apple's Metal HUD (frame rate) in games; play.sh reads the file at launch.
    @Published var fpsHUD = FileManager.default.fileExists(atPath: FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent(".battlenet-macos/fps-hud").path) {
        didSet {
            let file = base.appendingPathComponent("fps-hud")
            if fpsHUD {
                try? FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
                FileManager.default.createFile(atPath: file.path, contents: nil)
            } else { try? FileManager.default.removeItem(at: file) }
        }
    }
    private var operation: Process?
    private var launches: [Launcher: Process] = [:]
    private var logReader: FileHandle?
    private var poller: Timer?
    var version: String { Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev" }

    init() { refresh(); refreshLibrary() }

    func refresh() {
        installed = Set(Launcher.installable.filter {
            FileManager.default.fileExists(atPath: base.appendingPathComponent($0.executable).path)
        })
        steamGPTK = FileManager.default.fileExists(atPath: base.appendingPathComponent("steam-gptk").path)
            && FileManager.default.fileExists(atPath: base.appendingPathComponent(
                "steam-gptk-bottle/drive_c/Program Files (x86)/Steam/steam.exe").path)
    }
    /// soju steam-gptk: Steam and its games on Apple GPTK (scripts/steam-gptk.sh).
    @Published var steamGPTK = false

    func task(_ args: [String], logName: String, environment extra: [String: String] = [:]) throws -> (Process, URL) {
        let dir = base.appendingPathComponent("logs")
        // Logs can name games, paths and backend errors: keep them to this user.
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: dir.path)
        let url = dir.appendingPathComponent(logName + ".log")
        try Data().write(to: url, options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
        let output = try FileHandle(forWritingTo: url)
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/bin/bash")
        p.arguments = [root.appendingPathComponent("scripts/desktop-action.sh").path] + args
        p.currentDirectoryURL = root
        var env = ProcessInfo.processInfo.environment
        env["PATH"] = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
        env["SOJU_BASE"] = base.path
        env["SOJU_NONINTERACTIVE"] = "1"
        env["PYTHONDONTWRITEBYTECODE"] = "1"
        env["TERM"] = "dumb"
        for key in ["ENGINE", "WINEPREFIX", "WINEDLLOVERRIDES", "DYLD_FALLBACK_LIBRARY_PATH"] { env.removeValue(forKey: key) }
        env.merge(extra) { _, new in new }
        p.environment = env
        p.standardInput = FileHandle.nullDevice
        p.standardOutput = output
        p.standardError = output
        return (p, url)
    }

    func run(_ args: [String], title: String, environment: [String: String] = [:]) {
        guard !busy else { return }
        busy = true; failed = false; activity = title; log = ""
        do {
            let (p, url) = try task(args, logName: "desktop", environment: environment)
            logReader = try FileHandle(forReadingFrom: url)
            operation = p
            p.terminationHandler = { [weak self] process in
                DispatchQueue.main.async {
                    guard let self else { return }
                    self.readLog()
                    self.poller?.invalidate(); self.poller = nil
                    try? self.logReader?.close(); self.logReader = nil
                    self.busy = false; self.operation = nil
                    self.failed = process.terminationStatus != 0
                    self.activity = self.failed ? "Needs attention. See the details below." : "Finished."
                    self.refresh(); self.refreshLibrary()
                }
            }
            try p.run()
            poller = Timer.scheduledTimer(withTimeInterval: 0.4, repeats: true) { [weak self] _ in
                DispatchQueue.main.async { self?.readLog() }
            }
        } catch {
            busy = false; failed = true; activity = "Could not start the operation."
            log = error.localizedDescription
        }
    }

    private func readLog() {
        guard let reader = logReader else { return }
        let data = reader.readDataToEndOfFile()
        guard !data.isEmpty else { return }
        let text = String(decoding: data, as: UTF8.self)
            .replacingOccurrences(of: "\u{001B}\\[[0-9;]*[A-Za-z]", with: "", options: .regularExpression)
            .replacingOccurrences(of: "\r", with: "\n")
        log = String((log + text).suffix(60000))
    }

    func installSelection() {
        let selected = Launcher.installable.filter { selection.contains($0) }.map(\.rawValue)
        guard !selected.isEmpty else { return }
        run(["install", selected.joined(separator: ",")], title: "Installing selected launchers…")
    }

    func launch(_ launcher: Launcher) {
        guard launcher.opensAWindow else { requestedPage = "accounts"; return }
        guard !launching.contains(launcher), !busy else { return }
        do {
            let (p, url) = try task(["launch", launcher.rawValue], logName: "desktop-" + launcher.rawValue)
            launching.insert(launcher); launches[launcher] = p
            p.terminationHandler = { [weak self] process in
                DispatchQueue.main.async {
                    guard let self else { return }
                    self.launching.remove(launcher); self.launches.removeValue(forKey: launcher)
                    if process.terminationStatus != 0 && !self.busy {
                        self.failed = true; self.activity = launcher.title + " needs attention."
                        if let data = try? Data(contentsOf: url) { self.log = String(decoding: data.suffix(60000), as: UTF8.self) }
                    }
                    self.refresh(); self.refreshLibrary()
                }
            }
            try p.run()
            activity = "Opening " + launcher.title + "…"
        } catch {
            launching.remove(launcher); launches.removeValue(forKey: launcher)
            failed = true; activity = "Could not open " + launcher.title
            log = error.localizedDescription
        }
    }

    /// Opens a prefilled compatibility report (game, store, Mac, macOS, engine) in the browser.
    func report(_ game: InstalledGame) {
        guard let (p, _) = try? task(["report", game.id], logName: "desktop-report") else { return }
        try? p.run()
    }

    func addGPTK() {
        let panel = NSOpenPanel()
        panel.title = "Choose the mounted Game Porting Toolkit volume or payload"
        panel.message = "Download Apple's evaluation environment, open its DMG, then select the mounted volume."
        panel.canChooseDirectories = true; panel.canChooseFiles = false
        panel.directoryURL = URL(fileURLWithPath: "/Volumes")
        guard panel.runModal() == .OK, let url = panel.url else { return }
        run(["gptk", url.path], title: "Installing Apple GPTK…")
    }

    /// A Didim Labs game zip (bought and downloaded outside Soju) installs into didim-games.
    func installDidimGame() {
        let panel = NSOpenPanel()
        panel.title = "Choose a Didim Labs game"
        panel.message = "Select the game .zip you downloaded."
        panel.allowedContentTypes = [.zip]
        panel.canChooseDirectories = false
        panel.directoryURL = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first
        guard panel.runModal() == .OK, let url = panel.url else { return }
        run(["didim", "install", url.path], title: "Installing the game…")
    }

    func copyLog() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(log, forType: .string)
    }
}

struct PlatformsView: View {
    @EnvironmentObject var model: SojuModel
    @Environment(\.colorScheme) private var scheme
    private var green: Color { scheme == .dark ? Color(red: 0.42, green: 0.83, blue: 0.67) : Color(red: 0.12, green: 0.34, blue: 0.28) }
    // Three columns keep five platforms in two rows, so the log stays on screen.
    private let columns = Array(repeating: GridItem(.flexible(), spacing: 14, alignment: .top), count: 3)

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 7) {
                    HStack(spacing: 10) {
                        SojuLogo(size: 38)
                        Text("Soju").font(.system(size: 30, weight: .bold, design: .rounded))
                        Text("open source").font(.system(size: 12)).foregroundStyle(.secondary)
                    }
                    Text("Install and manage your official Windows launchers and Xbox PC downloads.").foregroundStyle(.secondary)
                }
                Spacer()
                Menu {
                    Button("Project & releases") { open("https://github.com/BCD1210/soju/releases") }
                    Button("Report compatibility") { open("https://github.com/BCD1210/soju/issues/new?template=compatibility.yml") }
                    Button("Open local logs") { NSWorkspace.shared.open(model.base.appendingPathComponent("logs")) }
                } label: { Image(systemName: "ellipsis.circle").font(.title2) }.menuStyle(.borderlessButton).fixedSize()
            }
            LazyVGrid(columns: columns, spacing: 16) {
                ForEach(Launcher.installable) { launcher in
                    card(launcher)
                }
            }
            HStack(spacing: 12) {
                Button(action: model.installSelection) {
                    Label("Install selected", systemImage: "arrow.down.circle")
                }.buttonStyle(.borderedProminent).tint(green)
                    .disabled(model.busy || model.selection.isEmpty)
                Button("Diagnose") { model.run(["doctor"], title: "Checking your setup…") }
                    .disabled(model.busy)
                Button("Update components") { model.run(["update"], title: "Checking component updates…") }
                    .disabled(model.busy)
                Toggle("Show FPS in games", isOn: $model.fpsHUD).toggleStyle(.checkbox)
                    .help("Apple's Metal HUD: frame rate and frame time in the corner of games started after this")
                Spacer()
                Menu("Apple GPTK") {
                    Button("Download and install") { model.run(["gptk"], title: "Downloading Apple GPTK…") }
                    Toggle("Use for Steam (Overwatch, Diablo II on Steam)", isOn: Binding(
                        get: { model.steamGPTK },
                        set: { on in model.run(["steam-gptk", on ? "on" : "off"],
                                               title: on ? "Moving Steam to Apple GPTK…" : "Moving Steam back to Wine 11…") }))
                        .disabled(!model.installed.contains(.steam))
                    Divider()
                    Button("Apple's download page") { open("https://developer.apple.com/games/game-porting-toolkit/") }
                    Button("Add mounted toolkit…", action: model.addGPTK)
                }.disabled(model.busy).fixedSize()
            }
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 8) {
                    if model.busy { ProgressView().controlSize(.small) }
                    else { Image(systemName: model.failed ? "exclamationmark.circle" : "checkmark.circle").foregroundStyle(model.failed ? Color.orange : green) }
                    Text(model.activity).font(.system(size: 12, weight: .medium))
                    Spacer()
                    Button("Copy log", action: model.copyLog).buttonStyle(.plain).foregroundStyle(.secondary)
                }
                ScrollViewReader { proxy in
                    ScrollView {
                        Text(model.log).font(.system(size: 11, design: .monospaced))
                            .textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
                        Color.clear.frame(height: 1).id("end")
                    }.frame(minHeight: 115, maxHeight: .infinity)
                        .onChange(of: model.log) { proxy.scrollTo("end", anchor: .bottom) }
                }
            }.padding(14).background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 12))
            HStack {
                Text("Accounts, purchases and updates stay with each official store.")
                Spacer()
                Text("v" + model.version)
            }.font(.system(size: 10)).foregroundStyle(.secondary)
        }.padding(28).frame(minWidth: 730, idealWidth: 850, minHeight: 610, idealHeight: 680)
            .background(Color(nsColor: .windowBackgroundColor))
            .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in model.refresh() }
    }

    private func card(_ launcher: Launcher) -> some View {
        let installed = model.installed.contains(launcher)
        return VStack(alignment: .leading, spacing: 14) {
            HStack {
                Image(systemName: launcher.symbol).font(.system(size: 25)).foregroundStyle(green)
                    .frame(width: 46, height: 46).background(green.opacity(0.08), in: RoundedRectangle(cornerRadius: 12))
                VStack(alignment: .leading, spacing: 4) {
                    Text(launcher.title).font(.system(size: 17, weight: .semibold))
                    Text(installed ? "Installed" : "Not installed").font(.system(size: 11))
                        .foregroundStyle(installed ? green : Color.secondary)
                }
                Spacer()
                Toggle("Select " + launcher.title, isOn: Binding(
                    get: { model.selection.contains(launcher) },
                    set: { if $0 { model.selection.insert(launcher) } else { model.selection.remove(launcher) } }
                )).labelsHidden().toggleStyle(.checkbox).disabled(model.busy)
                    .accessibilityLabel("Select " + launcher.title)
            }
            Text(launcher.detail).font(.system(size: 11)).foregroundStyle(.secondary)
            HStack {
                Text(launcher.accountName)
                    .font(.system(size: 10)).foregroundStyle(.tertiary)
                Spacer()
                if installed && !launcher.opensAWindow {
                    Button(model.accountSummaries.contains { $0.platform == launcher } ? "Manage…" : "Sign In…") { model.requestedPage = "accounts" }
                        .accessibilityLabel("Open Accounts for " + launcher.title)
                } else if installed {
                    Button(model.launching.contains(launcher) ? "Running" : "Open") { model.launch(launcher) }
                        .disabled(model.busy || model.launching.contains(launcher))
                        .accessibilityLabel("Open " + launcher.title)
                } else {
                    Button("Install") { model.run(["install", launcher.rawValue], title: "Installing " + launcher.title + "…") }
                        .disabled(model.busy).accessibilityLabel("Install " + launcher.title)
                }
            }
        }.padding(18).background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 16))
            .overlay(RoundedRectangle(cornerRadius: 16).stroke(green.opacity(0.12), lineWidth: 1))
    }

    private func open(_ url: String) { NSWorkspace.shared.open(URL(string: url)!) }
}

final class SojuDelegate: NSObject, NSApplicationDelegate {
    var isBusy: (() -> Bool)?
    var downloads: (() -> Int)?
    var pauseDownloads: (() -> Void)?
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        if let count = downloads?(), count > 0 {
            let alert = NSAlert()
            alert.messageText = count == 1 ? "A download is in progress" : String(count) + " downloads are in progress"
            alert.informativeText = "Quitting pauses them. Download again later to continue where they stopped."
            alert.addButton(withTitle: "Pause and Quit")
            alert.addButton(withTitle: "Cancel")
            guard alert.runModal() == .alertFirstButtonReturn else { return .terminateCancel }
            pauseDownloads?()
        }
        if isBusy?() == true {
            let alert = NSAlert()
            alert.messageText = "An operation is still running"
            alert.informativeText = "Let the current installation or update finish before quitting Soju."
            alert.runModal()
            return .terminateCancel
        }
        return .terminateNow
    }
}

@main struct SojuApp: App {
    @NSApplicationDelegateAdaptor(SojuDelegate.self) var delegate
    @StateObject private var model = SojuModel()
    var body: some Scene {
        Window("Soju", id: "main") {
            LibraryView().environmentObject(model).onOpenURL { model.open($0) }.onAppear {
                TVKeeper.start()
                delegate.isBusy = { model.busy }
                delegate.downloads = { model.downloadTasks.count }
                delegate.pauseDownloads = { model.pauseAllDownloads() }
                NSApp.setActivationPolicy(.regular)
                NSApp.activate(ignoringOtherApps: true)
            }
        }.defaultSize(width: 1140, height: 780)
    }
}
