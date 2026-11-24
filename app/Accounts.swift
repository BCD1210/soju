import SwiftUI
import AppKit

struct AccountSummary: Decodable, Identifiable {
    let platform: Launcher
    let label: String
    let updatedAt: String
    let count: Int
    var id: String { platform.rawValue }
    enum CodingKeys: String, CodingKey { case platform, label, count; case updatedAt = "updated_at" }
}
struct GalaxyAccount: Decodable, Identifiable { let id: String; let label: String; let count: Int }
struct ServiceFailure: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

enum ServiceBridge {
    @discardableResult static func start(root: URL, base: URL, args: [String], input: [String: String] = [:],
                                        completion: @escaping (Result<Data, Error>) -> Void) -> Process? {
        let p = Process()
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("soju-service-" + UUID().uuidString + ".json")
        do {
            try Data().write(to: url)
            try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
            let output = try FileHandle(forWritingTo: url)
            let pipe = Pipe()
            let inputData = try JSONEncoder().encode(input)
            guard inputData.count <= 8 * 1024 * 1024 else { throw ServiceFailure(message: "Account settings are too large.") }
            p.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            p.arguments = ["python3", root.appendingPathComponent("scripts/library-service.py").path] + args
            p.environment = ["PATH": "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin", "HOME": FileManager.default.homeDirectoryForCurrentUser.path,
                             "SOJU_BASE": base.path, "PYTHONIOENCODING": "utf-8", "PYTHONDONTWRITEBYTECODE": "1"]
            p.standardInput = pipe; p.standardOutput = output; p.standardError = FileHandle.nullDevice
            p.terminationHandler = { process in
                defer { try? FileManager.default.removeItem(at: url) }
                let result: Result<Data, Error>
                do {
                    let handle = try FileHandle(forReadingFrom: url)
                    defer { try? handle.close() }
                    let data = try handle.read(upToCount: 8 * 1024 * 1024 + 1) ?? Data()
                    guard data.count <= 8 * 1024 * 1024 else { throw ServiceFailure(message: "The response is too large.") }
                    if process.terminationStatus != 0 {
                        let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
                        throw ServiceFailure(message: object?["error"] as? String ?? "Request interrupted or unavailable. Please retry.")
                    }
                    result = .success(data)
                } catch { result = .failure(error) }
                DispatchQueue.main.async { completion(result) }
            }
            try p.run()
            try? output.close()
            // A large library must not block the main thread on the pipe buffer.
            DispatchQueue.global(qos: .userInitiated).async {
                try? pipe.fileHandleForWriting.write(contentsOf: inputData)
                try? pipe.fileHandleForWriting.close()
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + 50) { [weak p] in
                if let p, p.isRunning { p.terminate() }
            }
            return p
        } catch {
            p.terminationHandler = nil
            if p.isRunning { p.terminate() }
            try? FileManager.default.removeItem(at: url)
            completion(.failure(error))
            return nil
        }
    }
}

extension SojuModel {
    func reloadAccountLibrary() {
        scanner?.terminate(); scanner = nil; scanning = false
        refreshLibrary()
    }
    func syncAccount(_ platform: Launcher, input: [String: String] = [:]) {
        guard !accountBusy else { return }
        accountBusy = true; accountMessage = "Importing your games…"
        accountTask = ServiceBridge.start(root: root, base: base, args: ["sync", platform.rawValue], input: input) { [weak self] result in
            guard let self else { return }
            self.accountBusy = false; self.accountTask = nil
            switch result {
            case .success(let data):
                let response = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
                self.accountMessage = response?["message"] as? String ?? "Library imported."
                self.reloadAccountLibrary()
            case .failure(let error): self.accountMessage = error.localizedDescription
            }
        }
    }
    func importSteam(_ snapshot: [String: Any]) {
        do {
            let data = try JSONSerialization.data(withJSONObject: snapshot)
            guard let value = String(data: data, encoding: .utf8) else { throw ServiceFailure(message: "Steam returned an unreadable library.") }
            syncAccount(.steam, input: ["snapshot": value])
        } catch { accountMessage = error.localizedDescription }
    }
    /// Choose where new Xbox PC downloads go: a folder on an APFS external drive, or the default on this Mac.
    func chooseXboxLocation(useDefault: Bool = false) {
        guard !accountBusy else { return }
        var folder = "--default"
        if !useDefault {
            let panel = NSOpenPanel()
            panel.title = "Choose where Xbox PC games are downloaded"
            panel.message = "Pick a folder on an external drive (APFS, Mac OS Extended or exFAT). Games already downloaded stay where they are."
            panel.prompt = "Use This Folder"
            panel.canChooseDirectories = true; panel.canChooseFiles = false; panel.canCreateDirectories = true
            panel.directoryURL = URL(fileURLWithPath: "/Volumes")
            guard panel.runModal() == .OK, let url = panel.url else { return }
            folder = url.path
            // Read the folder here so macOS asks now whether Soju may use this drive,
            // not later in the middle of a library scan.
            _ = try? FileManager.default.contentsOfDirectory(atPath: url.path)
        }
        do {
            accountBusy = true; accountMessage = "Checking the download location…"
            let (p, url) = try task(["xbox", "location", folder], logName: "xbox-location")
            p.terminationHandler = { [weak self] process in
                let output = (try? String(contentsOf: url, encoding: .utf8)) ?? ""
                let lines = output.split(separator: "\n").map { $0.replacingOccurrences(of: "soju: ", with: "") }
                DispatchQueue.main.async {
                    guard let self else { return }
                    self.accountBusy = false
                    self.accountMessage = process.terminationStatus == 0
                        ? lines.joined(separator: " ")
                        : (lines.last ?? "Could not use that folder for Xbox downloads.")
                    self.refreshLibrary()
                }
            }
            try p.run()
        } catch {
            accountBusy = false
            accountMessage = "Could not change the download location: " + error.localizedDescription
        }
    }
    func forgetAccount(_ platform: Launcher) {
        guard !accountBusy else { return }
        do {
            if platform == .xbox {
                // Running downloads need the sign-in; pause them first. The import is
                // removed by xbox.sh only after the sign-out itself succeeded.
                for task in downloadTasks.values { task.terminate() }
                stoppingDownloads.formUnion(downloadTasks.keys)
                accountBusy = true; accountMessage = "Signing out of Microsoft…"
                let (p, _) = try task(["xbox", "logout"], logName: "xbox-account")
                p.terminationHandler = { [weak self] process in
                    DispatchQueue.main.async {
                        guard let self else { return }
                        self.accountBusy = false
                        switch process.terminationStatus {
                        case 0: self.accountMessage = "Signed out of Microsoft. Downloaded games are kept."
                        // xbox.sh: the import is gone, but the Keychain entry could not be removed.
                        case 2: self.accountMessage = "The Xbox library was removed, but the Microsoft sign-in is still in the Keychain. Delete \"Xodus Service\" in Keychain Access, or sign out again."
                        default: self.accountMessage = "Sign-out did not finish, so you are still signed in. Open Diagnostics in Library for details."
                        }
                        self.reloadAccountLibrary()
                    }
                }
                try p.run()
                return
            }
            let url = base.appendingPathComponent("account-libraries/" + platform.rawValue + ".json")
            if FileManager.default.fileExists(atPath: url.path) { try FileManager.default.removeItem(at: url) }
            if platform == .steam {
                accountBusy = true
                SteamWebSession.clear { [weak self] in
                    DispatchQueue.main.async {
                        self?.accountBusy = false
                        self?.accountMessage = "Steam disconnected. Installed games are kept."
                        self?.reloadAccountLibrary()
                    }
                }
            } else {
                accountMessage = "Account import removed. Installed games are kept."
                reloadAccountLibrary()
            }
        } catch { accountMessage = error.localizedDescription }
    }
    func installOwnedGame(_ game: InstalledGame) {
        guard !busy, !game.isInstalled else { return }
        if !installed.contains(game.platform) { libraryIssue = game.platform.title + " is not installed in Soju yet. Open Stores and press Install next to " + game.platform.title + ", then come back to this game."; return }
        if game.platform == .xbox { download(game); return }
        run(["game-install", game.id], title: "Opening " + game.platform.title + " to install " + game.title + "…")
    }
    /// Xbox downloads can take hours, so they run beside other operations, one per game.
    func download(_ game: InstalledGame) {
        guard downloadTasks[game.id] == nil else { return }
        if game.downloadBlocked, let issue = game.issue { libraryIssue = issue; return }
        do {
            let name = "xbox-download-" + game.id.replacingOccurrences(of: ":", with: "-")
            let (p, url) = try task(["game-install", game.id], logName: name)
            downloadTasks[game.id] = p; downloads[game.id] = "Starting download…"
            downloadTimers[game.id] = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
                DispatchQueue.main.async { self?.readProgress(game.id, url) }
            }
            p.terminationHandler = { [weak self] process in
                DispatchQueue.main.async {
                    guard let self else { return }
                    self.downloadTimers.removeValue(forKey: game.id)?.invalidate()
                    self.downloadTasks.removeValue(forKey: game.id); self.downloads.removeValue(forKey: game.id)
                    let stopped = self.stoppingDownloads.remove(game.id) != nil
                    NSApp.dockTile.badgeLabel = self.downloadTasks.isEmpty ? nil : String(self.downloadTasks.count)
                    // 130: stopped from outside the app (soju xbox-kill); the checkpoint is kept.
                    if (stopped && process.terminationStatus != 0) || (process.terminationReason == .exit && process.terminationStatus == 130) {
                        self.activity = "Download of " + game.title + " paused. Download again to resume it."
                    } else if process.terminationReason == .exit && process.terminationStatus == 0 {
                        self.activity = game.title + " is downloaded."
                        NSApp.requestUserAttention(.informationalRequest)
                    } else if process.terminationReason == .exit {
                        let text = (try? String(contentsOf: url, encoding: .utf8)) ?? ""
                        let reason = text.split(separator: "\n").last(where: { $0.hasPrefix("soju: ") }).map { String($0.dropFirst(6)) }
                        self.issueTitle = "Couldn't download " + game.title
                        self.libraryIssue = reason ?? "The download stopped. Download again to resume it."
                        NSApp.requestUserAttention(.informationalRequest)
                        self.log = String(text.suffix(60000))
                    } else {
                        self.activity = "Download of " + game.title + " stopped. Download again to resume it."
                    }
                    self.refreshLibrary()
                }
            }
            try p.run()
            NSApp.dockTile.badgeLabel = String(downloadTasks.count)
            activity = "Downloading " + game.title + "…"
        } catch {
            downloadTasks.removeValue(forKey: game.id); downloads.removeValue(forKey: game.id)
            libraryIssue = "Could not start the download: " + error.localizedDescription
        }
    }
    func pauseAllDownloads() {
        stoppingDownloads.formUnion(downloadTasks.keys)
        for task in downloadTasks.values { task.terminate() }
    }
    func stopDownload(_ game: InstalledGame) {
        guard let task = downloadTasks[game.id] else { return }
        stoppingDownloads.insert(game.id); task.terminate()
    }
    private func readProgress(_ id: String, _ url: URL) {
        guard downloadTasks[id] != nil, let handle = try? FileHandle(forReadingFrom: url) else { return }
        defer { try? handle.close() }
        let size = (try? handle.seekToEnd()) ?? 0
        try? handle.seek(toOffset: size > 2048 ? size - 2048 : 0)
        let text = String(decoding: handle.readDataToEndOfFile(), as: UTF8.self)
        if let line = text.split(separator: "\n").last(where: { $0.hasPrefix("  ") }) {
            downloads[id] = line.trimmingCharacters(in: .whitespaces)
        }
    }
    func cancelSignIn() { signInTask?.terminate() }
    func xboxSignIn(market: String) {
        guard !accountBusy else { return }
        accountBusy = true; accountMessage = "Finish signing in on the Microsoft page that opened. If it stays on \"Please wait\" after an account notice, close it and sign in again."
        do {
            let (p, url) = try task(["xbox", "login"], logName: "xbox-account")
            signInTask = p
            p.terminationHandler = { [weak self] process in
                DispatchQueue.main.async {
                    guard let self else { return }
                    self.accountBusy = false; self.signInTask = nil
                    // 143: xbox.sh closed the sign-in window after Cancel and exited itself.
                    if process.terminationReason == .uncaughtSignal || process.terminationStatus == 143 { self.accountMessage = "Sign-in cancelled."; return }
                    if process.terminationStatus == 0 { self.syncAccount(.xbox, input: ["market": market]) }
                    else {
                        let text = (try? String(contentsOf: url, encoding: .utf8)) ?? ""
                        self.accountMessage = text.contains("not completed") ? "Sign-in was not completed. If the page stayed on \"Please wait\" after an account notice, sign in again: Microsoft shows that notice once." : "Sign-in did not finish. Open Diagnostics in Library for details."
                        self.log = String(text.suffix(60000))
                    }
                }
            }
            try p.run()
        } catch { accountBusy = false; accountMessage = error.localizedDescription }
    }
}

struct AccountsView: View {
    @EnvironmentObject var model: SojuModel
    @ObservedObject private var didim = AccountModel.shared
    @State private var showSteamLogin = false
    @State private var users: [GalaxyAccount] = []
    @State private var userID = ""
    @State private var userTask: Process?
    @State private var usersMessage = ""
    @State private var confirmDisconnect = false
    @State private var showLog = false
    /// The store whose button was pressed last: its row shows what is happening and how it ended.
    @State private var lastAction: Launcher?
    /// Whether that button started a launcher task (install, remove) or an account task (sign in, sync).
    @State private var lastWasTask = false
    @AppStorage("soju.store.region") private var market = "US"
    private static let imported: Set<Launcher> = [.steam, .gog, .xbox]
    private var xboxLocationText: String {
        guard let location = model.xboxLocation else { return "This Mac" }
        var text = location.isDefault ? "This Mac" : location.volume + " (" + location.path + ")"
        if !location.available { return text + " · drive not connected" }
        if let free = location.freeBytes { text += " · " + ByteCountFormatter.string(fromByteCount: free, countStyle: .file) + " free" }
        return text
    }
    private func summary(_ platform: Launcher) -> AccountSummary? { model.accountSummaries.first { $0.platform == platform } }
    private func installedGames(_ platform: Launcher) -> Int { model.games.filter { $0.platform == platform && $0.isInstalled }.count }
    private func working(_ platform: Launcher) -> Bool {
        lastAction == platform && (lastWasTask ? model.busy : model.accountBusy)
    }
    private func note(_ platform: Launcher) -> String {
        guard lastAction == platform else { return "" }
        if lastWasTask { return model.busy || model.failed ? model.activity : "" }
        return model.accountMessage
    }
    private func task(_ platform: Launcher, _ args: [String], _ title: String) {
        lastAction = platform; lastWasTask = true
        model.run(args, title: title)
    }
    private func account(_ platform: Launcher, _ action: () -> Void) {
        lastAction = platform; lastWasTask = false
        action()
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 8) {
                Text("Stores").font(.system(size: 32, weight: .bold, design: .rounded))
                Text("Install a store's launcher to play its games. Sign in to see the games you own, installed or not.").foregroundStyle(.secondary)
                HStack(spacing: 10) {
                    let all = Launcher.installable
                    let games = Self.imported.compactMap { summary($0)?.count }.reduce(0, +)
                    chip(String(all.filter { model.installed.contains($0) }.count) + " of " + String(all.count) + " stores installed")
                    chip(games == 1 ? "1 owned game imported" : String(games) + " owned games imported")
                    chip("Sign-ins stay on this Mac")
                }.padding(.top, 10)

                heading("Didim Labs")
                card { didimRow }

                heading("Game stores")
                card {
                    ForEach(Array(Launcher.installable.enumerated()), id: \.element) { index, launcher in
                        if index > 0 { Divider() }
                        storeRow(launcher)
                        if launcher == .steam && model.installed.contains(.steam) { detail { steamMode } }
                        if launcher == .gog && users.count > 1 {
                            detail {
                                Picker("Galaxy account", selection: $userID) {
                                    Text("Choose an account").tag("")
                                    ForEach(users) { user in Text(user.label + " · " + String(user.count) + " games").tag(user.id) }
                                }.frame(maxWidth: 420)
                            }
                        }
                        if launcher == .xbox && model.installed.contains(.xbox) { detail { xboxSettings } }
                    }
                }

                heading("Settings")
                card { settings }
                activityPanel

                HStack {
                    Text("Owned games are a snapshot: sync again after you buy something. Signing out keeps installed games; removing a store removes the games installed in it.")
                    Spacer()
                    Button("Uninstall Soju…") { model.confirmUninstallAll() }.buttonStyle(.link).disabled(model.busy)
                }.font(.system(size: 11)).foregroundStyle(.secondary).padding(.top, 10)
            }.padding(28)
        }.onAppear { loadUsers(); didim.loadProviders() }
            .confirmationDialog("Sign out of Xbox PC?", isPresented: $confirmDisconnect) {
                Button("Sign Out", role: .destructive) { account(.xbox) { model.forgetAccount(.xbox) } }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text(model.downloadTasks.isEmpty ? "Your imported Xbox library is removed. Downloaded games stay on this Mac."
                     : "Running downloads are paused. Your imported Xbox library is removed; downloaded games stay on this Mac.")
            }
            .sheet(isPresented: $showSteamLogin) {
                SteamLoginView(root: model.root, completion: { value in
                    showSteamLogin = false
                    model.importSteam(value)
                }, cancel: { showSteamLogin = false })
            }
    }

    // MARK: rows

    private var didimRow: some View {
        let signedIn = !didim.email.isEmpty
        let owned = didim.owned.count
        let status = didim.signingIn != nil ? "Finish signing in in your browser"
            : signedIn ? didim.email + (owned == 0 ? "" : owned == 1 ? " · 1 game" : " · " + String(owned) + " games")
            : "Not signed in · needed to buy games and play demos"
        return row(symbol: "person.crop.circle", title: "Didim Labs account", ok: signedIn, status: status,
                   note: didim.signingIn == nil ? didim.message : "") {
            if didim.signingIn != nil {
                ProgressView().controlSize(.small)
                Button("Cancel") { didim.cancel() }
            } else if signedIn {
                Button("Sign out") { didim.signOut() }
            } else if didim.providers.isEmpty {
                Button("Retry") { didim.loadProviders() }.help("Sign-in is not available right now. Check the internet connection.")
            } else {
                if didim.providers.contains("apple") {
                    Button { didim.signIn("apple") } label: { Label("Apple", systemImage: "apple.logo") }.buttonStyle(.borderedProminent)
                }
                if didim.providers.contains("google") {
                    Button { didim.signIn("google") } label: { Label("Google", systemImage: "g.circle") }.buttonStyle(.borderedProminent)
                }
            }
        }
    }

    /// What a store's row says: the launcher first, then the owned library where Soju can import one.
    private func status(_ launcher: Launcher) -> String {
        let installed = model.installed.contains(launcher)
        let count = installedGames(launcher)
        var parts = [!installed ? "Not installed" : count == 0 ? "Installed" : count == 1 ? "1 game installed" : String(count) + " games installed"]
        if Self.imported.contains(launcher) {
            if let value = summary(launcher) { parts.append(summaryLine(value)) }
            else if launcher == .gog && !usersMessage.isEmpty && installed { parts.append(usersMessage) }
            else { parts.append(launcher == .steam && !installed ? "sign in works without installing" : "not signed in") }
        }
        return parts.joined(separator: " · ")
    }

    /// One main button per store, the next step for it: install, then sign in, then open or sync.
    private func storeRow(_ launcher: Launcher) -> some View {
        let installed = model.installed.contains(launcher)
        let connected = summary(launcher) != nil
        let importable = Self.imported.contains(launcher)
        let gogBlocked = users.count > 1 && userID.isEmpty
        return row(symbol: launcher.symbol, title: launcher.title, badge: launcher == .xbox ? "Experimental" : nil,
                   ok: installed, status: status(launcher), note: note(launcher), help: launcher.detail) {
            if working(launcher) { ProgressView().controlSize(.small) }
            if launcher == .xbox, model.signInTask != nil { Button("Cancel") { model.cancelSignIn() } }
            if !installed {
                Button("Install") { task(launcher, ["install", launcher.rawValue], "Installing " + launcher.title + "…") }
                    .buttonStyle(.borderedProminent).disabled(model.busy).accessibilityLabel("Install " + launcher.title)
            } else if importable && !connected {
                Button(launcher == .gog ? "Import games" : "Sign in") { signIn(launcher) }
                    .buttonStyle(.borderedProminent).disabled(model.accountBusy || (launcher == .gog && gogBlocked))
                    .accessibilityLabel("Sign in to " + launcher.title)
            } else if launcher.opensAWindow {
                Button(model.launching.contains(launcher) ? "Running" : "Open") { model.launch(launcher) }
                    .disabled(model.busy || model.launching.contains(launcher)).accessibilityLabel("Open " + launcher.title)
            } else {
                Button("Sync") { signIn(launcher) }.disabled(model.accountBusy).accessibilityLabel("Sync " + launcher.title)
            }
            more {
                if installed && launcher.opensAWindow && importable && !connected {
                    Button("Open " + launcher.title) { model.launch(launcher) }.disabled(model.busy || model.launching.contains(launcher))
                }
                if importable && (connected ? launcher.opensAWindow : !installed) {
                    Button(connected ? "Sync owned games" : launcher == .gog ? "Import owned games" : "Sign in to see owned games") { signIn(launcher) }
                        .disabled(model.accountBusy || (launcher == .gog && (gogBlocked || !installed)) || (launcher == .xbox && !installed))
                }
                if launcher == .gog { Button("Refresh account list", action: loadUsers).disabled(userTask != nil) }
                if launcher == .battlenet {
                    Button("Use Existing Battle.net Game…") { account(.battlenet) { model.importBattlenetGame() } }.disabled(model.busy)
                }
                if launcher == .steam && !connected {
                    Button("Clear saved sign-in") { account(.steam) { model.forgetAccount(.steam) } }.disabled(model.accountBusy)
                }
                if connected {
                    Button(launcher == .xbox ? "Sign Out…" : "Disconnect account") {
                        if launcher == .xbox { confirmDisconnect = true } else { account(launcher) { model.forgetAccount(launcher) } }
                    }.disabled(model.accountBusy)
                }
                if installed {
                    Divider()
                    if launcher.opensAWindow {
                        Button("Force Quit…") { model.confirmForceQuit(launcher) }.disabled(model.busy)
                    }
                    Button("Remove " + launcher.title + "…", role: .destructive) { lastAction = launcher; lastWasTask = true; model.confirmRemove(launcher) }
                        .disabled(model.busy)
                }
            }
        }
    }

    private func signIn(_ launcher: Launcher) {
        account(launcher) {
            switch launcher {
            case .steam: showSteamLogin = true
            case .gog: model.syncAccount(.gog, input: userID.isEmpty ? [:] : ["user_id": userID])
            case .xbox:
                if summary(.xbox) == nil { model.xboxSignIn(market: market) } else { model.syncAccount(.xbox, input: ["market": market]) }
            default: break
            }
        }
    }

    private var steamMode: some View {
        // Steam games that need Apple's toolkit (D3D12, Blizzard games sold on Steam): scripts/steam-gptk.sh
        Toggle("DirectX 12 mode (Apple GPTK)", isOn: Binding(
            get: { model.steamGPTK },
            set: { on in task(.steam, ["steam-gptk", on ? "on" : "off"], on ? "Switching Steam to DirectX 12 mode…" : "Switching Steam back…") }))
            .toggleStyle(.checkbox).font(.system(size: 13)).disabled(model.busy)
            .help("Runs Steam and its games with Apple's Game Porting Toolkit, which DirectX 12 games, Overwatch and Diablo II: Resurrected need. It turns on by itself when you press Play on such a game. Steam asks you to sign in once the first time.")
    }

    private var xboxSettings: some View {
        Grid(alignment: .leading, horizontalSpacing: 14, verticalSpacing: 8) {
            GridRow {
                Text("Store region").foregroundStyle(.secondary)
                Picker("Store region", selection: $market) {
                    Text("US · USD").tag("US"); Text("Korea · KRW").tag("KR"); Text("UK · GBP").tag("GB")
                    Text("Germany · EUR").tag("DE"); Text("Japan · JPY").tag("JP")
                }.labelsHidden().frame(width: 160)
                    .help("Used for the Xbox library and Discover. A connected library syncs again when you change it.")
                    .onChange(of: market) {
                        if summary(.xbox) != nil && !model.accountBusy { account(.xbox) { model.syncAccount(.xbox, input: ["market": market]) } }
                    }
                Spacer()
            }
            GridRow {
                Text("Downloads go to").foregroundStyle(.secondary)
                Text(xboxLocationText).foregroundStyle(model.xboxLocation?.available == false ? .orange : .primary)
                    .lineLimit(2).textSelection(.enabled)
                HStack {
                    Spacer()
                    if model.xboxLocation?.isDefault == false {
                        Button("Use This Mac") { account(.xbox) { model.chooseXboxLocation(useDefault: true) } }.disabled(model.accountBusy)
                    }
                    Button("Change…") { account(.xbox) { model.chooseXboxLocation() } }.disabled(model.accountBusy)
                        .help("Download new Xbox PC games to a folder on an external drive (APFS, Mac OS Extended or exFAT).")
                }
            }
        }.font(.system(size: 13))
    }

    // MARK: settings

    private var settings: some View {
        VStack(alignment: .leading, spacing: 0) {
            setting("Show FPS in games", "Frame rate in the corner of games started after this (Apple's Metal HUD).") {
                Toggle("Show FPS in games", isOn: $model.fpsHUD).labelsHidden().toggleStyle(.switch).controlSize(.small)
            }
            Divider()
            setting("Share which games I play", "Anonymous: the games you install, how long they stay open and a one-word reason when one will not start, with your Mac model and macOS. No account, email or file paths.") {
                Toggle("Share which games I play", isOn: $model.shareStats).labelsHidden().toggleStyle(.switch).controlSize(.small)
            }
            Divider()
            setting("Check my setup", "Finds what is missing or broken and says how to fix it.") {
                Button("Diagnose") { lastAction = nil; showLog = true; model.run(["doctor"], title: "Checking your setup…") }.disabled(model.busy)
            }
            Divider()
            setting("Soju components", "The Wine engine and helper files. Soju also updates them by itself when no game runs.") {
                Menu("Apple GPTK") {
                    Button("Download and install") { lastAction = nil; model.run(["gptk"], title: "Downloading Apple GPTK…") }
                    Divider()
                    Button("Apple's download page") { NSWorkspace.shared.open(URL(string: "https://developer.apple.com/games/game-porting-toolkit/")!) }
                    Button("Add mounted toolkit…", action: model.addGPTK)
                }.disabled(model.busy).fixedSize()
                Button("Update") { lastAction = nil; model.run(["update"], title: "Checking component updates…") }.disabled(model.busy)
            }
            Divider()
            setting("Help", "Releases, compatibility reports and the log files on this Mac.") {
                Button("Releases") { NSWorkspace.shared.open(URL(string: "https://github.com/BCD1210/soju/releases")!) }
                Button("Report a game") { NSWorkspace.shared.open(URL(string: "https://github.com/BCD1210/soju/issues/new?template=compatibility.yml")!) }
                Button("Open logs") { NSWorkspace.shared.open(model.base.appendingPathComponent("logs")) }
            }
        }
    }
    private func setting<Controls: View>(_ title: String, _ text: String, @ViewBuilder controls: () -> Controls) -> some View {
        HStack(spacing: 14) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 14, weight: .medium))
                Text(text).font(.system(size: 12)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 8)
            controls()
        }.padding(.horizontal, 16).padding(.vertical, 10)
    }

    /// What Soju is doing right now, with the raw log behind "Details" (it opens by itself on a failure).
    private var activityPanel: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                if model.busy { ProgressView().controlSize(.small) }
                else { Image(systemName: model.failed ? "exclamationmark.circle" : "checkmark.circle").foregroundStyle(model.failed ? Color.orange : Color.green) }
                Text(model.activity).font(.system(size: 13, weight: .medium)).lineLimit(2)
                Spacer()
                Button(showLog || model.failed ? "Hide details" : "Details") { showLog.toggle() }
                    .buttonStyle(.plain).foregroundStyle(.secondary).disabled(model.failed)
                Button("Copy log", action: model.copyLog).buttonStyle(.plain).foregroundStyle(.secondary)
            }
            if showLog || model.failed {
                ScrollViewReader { proxy in
                    ScrollView {
                        Text(model.log).font(.system(size: 12, design: .monospaced))
                            .textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
                        Color.clear.frame(height: 1).id("end")
                    }.frame(height: 180)
                        .onChange(of: model.log) { proxy.scrollTo("end", anchor: .bottom) }
                }
            }
        }.padding(14).background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 12)).padding(.top, 8)
    }

    // MARK: pieces

    private func heading(_ text: String) -> some View {
        Text(text.uppercased()).font(.system(size: 12, weight: .semibold)).tracking(0.6).foregroundStyle(.secondary)
            .padding(.top, 16).padding(.leading, 4)
    }
    private func chip(_ text: String) -> some View {
        Text(text).font(.system(size: 13)).padding(.horizontal, 12).padding(.vertical, 6)
            .background(Color.primary.opacity(0.05), in: RoundedRectangle(cornerRadius: 8))
    }
    private func card<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        VStack(spacing: 0) { content() }
            .background(Color.primary.opacity(0.04), in: RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12).stroke(Color.primary.opacity(0.08)))
    }
    /// Settings that belong to the row above, indented under its title.
    private func detail<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        HStack { content(); Spacer(minLength: 0) }
            .padding(.leading, 66).padding(.trailing, 16).padding(.bottom, 12)
    }
    private func more<Items: View>(@ViewBuilder _ items: () -> Items) -> some View {
        Menu { items() } label: { Image(systemName: "ellipsis") }
            .menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize().help("More")
    }
    /// One store: icon, name, a status line with a dot, and its buttons on the right.
    private func row<Buttons: View>(symbol: String, title: String, badge: String? = nil, ok: Bool, status: String, note: String, help: String = "",
                                    @ViewBuilder buttons: () -> Buttons) -> some View {
        HStack(spacing: 14) {
            Image(systemName: symbol).font(.system(size: 17)).frame(width: 36, height: 36)
                .background(Color.primary.opacity(0.08), in: RoundedRectangle(cornerRadius: 9))
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 8) {
                    Text(title).font(.system(size: 16, weight: .semibold)).help(help)
                    if let badge {
                        Text(badge).font(.system(size: 11, weight: .medium)).padding(.horizontal, 6).padding(.vertical, 1)
                            .background(Color.primary.opacity(0.1), in: RoundedRectangle(cornerRadius: 4))
                    }
                }
                HStack(spacing: 6) {
                    Circle().fill(ok ? Color.green : Color.secondary.opacity(0.5)).frame(width: 7, height: 7)
                    Text(verbatim: status).font(.system(size: 13)).foregroundStyle(.secondary).lineLimit(2)
                }
                if !note.isEmpty {
                    Text(verbatim: note).font(.system(size: 13)).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer(minLength: 8)
            buttons()
        }.padding(.horizontal, 16).padding(.vertical, 12)
    }

    /// "1 game · synced Nov 2", without the internal account number some imports carry ("Galaxy · …1379").
    private func summaryLine(_ value: AccountSummary) -> String {
        let label = value.label.components(separatedBy: " · ").filter { !$0.hasPrefix("…") && $0 != "Galaxy" }.joined(separator: " · ")
        let games = value.count == 1 ? "1 owned game" : "\(value.count) owned games"
        var updated = String(value.updatedAt.prefix(10))
        let parse = DateFormatter(); parse.dateFormat = "yyyy-MM-dd"; parse.locale = Locale(identifier: "en_US_POSIX")
        if let date = parse.date(from: updated) {
            let show = DateFormatter(); show.setLocalizedDateFormatFromTemplate("MMM d"); updated = show.string(from: date)
        }
        return [games, "synced " + updated, label].filter { !$0.isEmpty }.joined(separator: " · ")
    }
    private func loadUsers() {
        guard userTask == nil else { return }
        userTask = ServiceBridge.start(root: model.root, base: model.base, args: ["gog-users"]) { result in
            userTask = nil
            switch result {
            case .success(let data):
                struct Response: Decodable { let users: [GalaxyAccount] }
                users = (try? JSONDecoder().decode(Response.self, from: data).users) ?? []
                if users.count == 1 { userID = users[0].id }
                else if !users.contains(where: { $0.id == userID }) { userID = "" }
                usersMessage = users.isEmpty ? "No owned Galaxy library found yet · open Galaxy once" : ""
            case .failure(let error): usersMessage = error.localizedDescription
            }
        }
    }
}
