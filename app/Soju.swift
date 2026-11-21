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
        case .battlenet: return "Diablo, Overwatch, StarCraft and more"
        case .steam: return "Your Steam games"
        case .epic: return "Your Epic Games library"
        case .gog: return "Your GOG library"
        case .ea, .rockstar, .ubisoft: return "New: sign-in works, games not tested yet"
        case .xbox: return "Game Pass and Microsoft Store · experimental"
        case .didim: return "Native Mac games"
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
    @Published var starAsk = StarAsk.load()
    var gameLaunches: [String: Process] = [:]
    var scanner: Process?
    @Published var installed: Set<Launcher> = []
    @Published var busy = false
    @Published var activity = "Ready."
    @Published var log = "Choose a launcher to get started. Existing installations are detected automatically."
    @Published var failed = false
    /// How far the running operation's download is (0...1), read from curl's meter in its log;
    /// nil while it reports none (unpacking, checking).
    @Published var progress: Double?
    /// Speed and time left for that download, or what happens after it.
    @Published var progressDetail = ""
    @Published var launching: Set<Launcher> = []
    @Published var requestedPage: String?
    /// A game the store should start checking out, from a soju://store/<game>?buy=1 link.
    @Published var requestedBuy: String?
    /// A Didim Labs demo started while nobody is signed in: the store asks for Apple or Google sign-in, then plays it.
    @Published var requestedDemoPlay: String?
    /// Signed in to Didim Labs (Apple or Google) in the store.
    var didimSignedIn: Bool { !(UserDefaults.standard.string(forKey: "didimAccountEmail") ?? "").isEmpty }
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
                requestedAuth = [state, code]
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
    /// Anonymous play counts for didimlabs.com (scripts/play-stats.py); on unless the stats-off file exists.
    @Published var shareStats = !FileManager.default.fileExists(atPath: FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent(".battlenet-macos/stats-off").path) {
        didSet {
            let file = base.appendingPathComponent("stats-off")
            if shareStats { try? FileManager.default.removeItem(at: file) } else {
                try? FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
                FileManager.default.createFile(atPath: file.path, contents: nil)
            }
        }
    }
    /// Starts a background count of one session or of the installed games. Never shows errors.
    func sendStats(_ args: [String], logName: String) {
        guard shareStats, ProcessInfo.processInfo.environment["SOJU_NO_STATS"] != "1" else { return }
        if let (p, _) = try? task(["stats"] + args, logName: logName) { try? p.run() }
    }
    private var operation: Process?
    private var launches: [Launcher: Process] = [:]
    private var logReader: FileHandle?
    private var poller: Timer?
    var version: String { Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev" }

    init() { refresh(); refreshLibrary() }

    /// The engine this app's scripts pin (ENGINE_TAG in scripts/update.sh) and the one installed.
    private func engineBehind() -> Bool {
        guard let script = try? String(contentsOf: root.appendingPathComponent("scripts/update.sh"), encoding: .utf8),
              let line = script.split(separator: "\n").first(where: { $0.hasPrefix("ENGINE_TAG=") }) else { return false }
        let want = line.dropFirst("ENGINE_TAG=".count).trimmingCharacters(in: CharacterSet(charactersIn: "\" "))
        let engine = base.appendingPathComponent("cx26-engine")
        guard FileManager.default.fileExists(atPath: engine.appendingPathComponent("bin/wine").path) else { return false }
        let have = (try? String(contentsOf: engine.appendingPathComponent(".soju-engine-release"), encoding: .utf8))?
            .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return !want.isEmpty && have != want
    }
    private var componentTimer: Timer?

    /// Called once Soju knows it is the newest release: if the installed engine is older than the one
    /// this release pins, run Update components as soon as no launcher or game runs.
    /// SOJU_NO_AUTO_UPDATE=1 leaves it to the button.
    func startComponentUpdateWhenIdle() {
        guard ProcessInfo.processInfo.environment["SOJU_NO_AUTO_UPDATE"] != "1", engineBehind() else { return }
        let attempt = { [weak self] in
            guard let self, !self.busy, self.engineBehind(), wineIsIdle() else { return }
            self.componentTimer?.invalidate(); self.componentTimer = nil
            self.run(["update"], title: "Updating the Wine engine…")
        }
        attempt()
        if engineBehind() && componentTimer == nil && !busy {
            componentTimer = Timer.scheduledTimer(withTimeInterval: 60, repeats: true) { _ in
                Task { @MainActor in attempt() }
            }
        }
    }

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
        busy = true; failed = false; activity = title; log = ""; progress = nil; progressDetail = ""
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
                    self.busy = false; self.operation = nil; self.progress = nil; self.progressDetail = ""
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

    /// Platforms → a launcher's Remove…: its bottle with the games in it, and its app shortcut.
    func confirmRemove(_ launcher: Launcher) {
        let alert = NSAlert()
        alert.messageText = "Remove " + launcher.title + "?"
        alert.informativeText = "This deletes " + launcher.title + " from Soju together with the games installed in it and their local saves. "
            + "Games you moved to another drive stay on that drive. Your account and purchases stay with the store, "
            + "and you can install it again from Stores. Other launchers are not touched."
        alert.alertStyle = .warning
        alert.addButton(withTitle: "Remove " + launcher.title)
        alert.addButton(withTitle: "Cancel")
        alert.buttons.first?.hasDestructiveAction = true
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        run(["uninstall", "launcher", launcher.rawValue], title: "Removing " + launcher.title + "…")
    }

    /// Platforms → a launcher's Force Quit…: stops every process in its bottle, the launcher and its
    /// games included, also ones whose Wine server already died (they ignore Quit and block a Mac
    /// shutdown). Unsaved game progress is lost.
    func confirmForceQuit(_ launcher: Launcher) {
        let alert = NSAlert()
        alert.messageText = "Force quit " + launcher.title + "?"
        alert.informativeText = "This closes " + launcher.title + " and every game started from it right away, "
            + "including ones that no longer respond or keep the Mac from shutting down. Progress that the game has not saved is lost."
        alert.alertStyle = .warning
        alert.addButton(withTitle: "Force Quit")
        alert.addButton(withTitle: "Cancel")
        alert.buttons.first?.hasDestructiveAction = true
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        run(["quit", launcher.rawValue], title: "Closing " + launcher.title + " and its games…")
    }

    /// Platforms → Uninstall Soju…: everything soju uninstall --yes removes, then the app goes to the Trash.
    func confirmUninstallAll() {
        let alert = NSAlert()
        alert.messageText = "Uninstall Soju?"
        alert.informativeText = "This deletes every launcher Soju installed, all games inside them and their local saves, "
            + "Soju's engines and downloads (in ~/.battlenet-macos), and then moves Soju to the Trash. "
            + "Games you moved to another drive stay on that drive. Accounts and purchases stay with each store."
        alert.alertStyle = .critical
        alert.addButton(withTitle: "Uninstall Soju")
        alert.addButton(withTitle: "Cancel")
        alert.buttons.first?.hasDestructiveAction = true
        guard alert.runModal() == .alertFirstButtonReturn, !busy else { return }
        busy = true; failed = false; activity = "Uninstalling Soju…"; log = ""
        do {
            let (p, url) = try task(["uninstall", "all"], logName: "uninstall")
            p.terminationHandler = { [weak self] process in
                DispatchQueue.main.async {
                    guard let self else { return }
                    self.busy = false
                    guard process.terminationStatus == 0 else {
                        self.failed = true; self.activity = "Uninstall stopped. See the details below."
                        self.log = (try? String(contentsOf: url, encoding: .utf8)) ?? ""
                        return
                    }
                    UserDefaults.standard.removePersistentDomain(forName: Bundle.main.bundleIdentifier ?? "app.soju.desktop")
                    NSWorkspace.shared.recycle([Bundle.main.bundleURL]) { _, _ in
                        DispatchQueue.main.async { NSApp.terminate(nil) }
                    }
                }
            }
            try p.run()
        } catch {
            busy = false; failed = true; activity = "Could not start the uninstall."; log = error.localizedDescription
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
        if let meter = DownloadMeter.last(in: text) {
            progress = meter.done ? nil : meter.fraction
            progressDetail = meter.done ? "Downloaded. Setting it up…" : meter.detail
        }
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
