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
        if !installed.contains(game.platform) { libraryIssue = game.platform.title + " is not installed in Soju yet. Open Platforms and press Install on the " + game.platform.title + " card, then come back to this game."; return }
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
    @State private var showSteamLogin = false
    @State private var users: [GalaxyAccount] = []
    @State private var userID = ""
    @State private var userTask: Process?
    @State private var usersMessage = ""
    @AppStorage("soju.store.region") private var market = "US"
    private var xboxLocationText: String {
        guard let location = model.xboxLocation else { return "Downloads go to this Mac." }
        var text = location.isDefault ? "Downloads go to this Mac" : "Downloads go to " + location.volume + " (" + location.path + ")"
        if !location.available { return text + " · drive not connected" }
        if let free = location.freeBytes { text += " · " + ByteCountFormatter.string(fromByteCount: free, countStyle: .file) + " free" }
        return text
    }
    @State private var confirmDisconnect = false
    private func summary(_ platform: Launcher) -> AccountSummary? { model.accountSummaries.first { $0.platform == platform } }
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                Text("Your accounts").font(.system(size: 30, weight: .bold, design: .rounded))
                Text("Bring owned games into your library, including games you have not installed.").foregroundStyle(.secondary)
                GroupBox {
                    VStack(alignment: .leading, spacing: 12) {
                        accountHeader(.steam)
                        Text("Sign in below to import your owned games. You do not need to install Windows Steam first.")
                            .font(.callout).foregroundStyle(.secondary)
                        Text("Steam handles your password, QR sign-in and Steam Guard. Your web session stays on this Mac.")
                            .font(.caption).foregroundStyle(.secondary)
                        HStack {
                            Button("Open Windows Steam") { model.launch(.steam) }.disabled(model.busy || !model.installed.contains(.steam))
                            if summary(.steam) == nil {
                                Button("Clear saved sign-in") { model.forgetAccount(.steam) }.disabled(model.accountBusy)
                            }
                            Spacer()
                            Button(summary(.steam) == nil ? "Sign in to Steam" : "Sync Steam library") { showSteamLogin = true }
                                .buttonStyle(.borderedProminent).disabled(model.accountBusy)
                        }
                    }.padding(12)
                }
                GroupBox {
                    VStack(alignment: .leading, spacing: 12) {
                        accountHeader(.gog)
                        Text("Import the owned library saved by GOG GALAXY on this Mac. Open Galaxy and let it finish refreshing before importing.")
                            .font(.callout).foregroundStyle(.secondary)
                        if users.count > 1 {
                            Picker("Galaxy account", selection: $userID) {
                                Text("Choose an account").tag("")
                                ForEach(users) { user in Text(user.label + " · " + String(user.count) + " games").tag(user.id) }
                            }
                        }
                        if !usersMessage.isEmpty { Text(usersMessage).font(.caption).foregroundStyle(.secondary) }
                        HStack {
                            Button("Open Galaxy") { model.launch(.gog) }.disabled(model.busy)
                            Button("Refresh account list", action: loadUsers).disabled(userTask != nil)
                            Spacer()
                            Button("Import from Galaxy") { model.syncAccount(.gog, input: userID.isEmpty ? [:] : ["user_id": userID]) }
                                .buttonStyle(.borderedProminent).disabled(model.accountBusy || (users.count > 1 && userID.isEmpty))
                        }
                    }.padding(12)
                }
                GroupBox {
                    VStack(alignment: .leading, spacing: 12) {
                        accountHeader(.xbox)
                        Text("Sign in with your Microsoft account on Microsoft's own page. Soju imports the games you own and your PC Game Pass games, downloads them and starts them. Xbox PC support is experimental: not every game works.")
                            .font(.callout).foregroundStyle(.secondary)
                        Text("Your sign-in stays in the macOS Keychain; macOS may ask once to allow access. Some newer games can't be downloaded yet.")
                            .font(.caption).foregroundStyle(.secondary)
                        HStack {
                            Picker("Store region", selection: $market) {
                                Text("US · USD").tag("US"); Text("Korea · KRW").tag("KR"); Text("UK · GBP").tag("GB")
                                Text("Germany · EUR").tag("DE"); Text("Japan · JPY").tag("JP")
                            }.frame(width: 230)
                                .help("Used for the Xbox library and Discover. A connected library syncs again when you change it.")
                                .onChange(of: market) {
                                    if summary(.xbox) != nil && !model.accountBusy { model.syncAccount(.xbox, input: ["market": market]) }
                                }
                            if !model.installed.contains(.xbox) {
                                Button("Set Up Xbox PC…") { model.requestedPage = "platforms" }.buttonStyle(.link)
                            }
                            Spacer()
                            if model.signInTask != nil {
                                Button("Cancel") { model.cancelSignIn() }
                            }
                            if summary(.xbox) != nil {
                                Button("Sync Xbox Library") { model.syncAccount(.xbox, input: ["market": market]) }
                                    .buttonStyle(.borderedProminent).disabled(model.accountBusy || !model.installed.contains(.xbox))
                            } else {
                                Button("Sign In to Microsoft…") { model.xboxSignIn(market: market) }
                                    .buttonStyle(.borderedProminent).disabled(model.accountBusy || !model.installed.contains(.xbox))
                            }
                        }
                        if model.installed.contains(.xbox) {
                            HStack(spacing: 10) {
                                Label(xboxLocationText, systemImage: model.xboxLocation?.isDefault == false ? "externaldrive" : "internaldrive")
                                    .font(.callout).foregroundStyle(model.xboxLocation?.available == false ? .orange : .secondary)
                                    .lineLimit(2).textSelection(.enabled)
                                Spacer()
                                Button("Change…") { model.chooseXboxLocation() }.disabled(model.accountBusy)
                                    .help("Download new Xbox PC games to a folder on an external drive (APFS, Mac OS Extended or exFAT).")
                                if model.xboxLocation?.isDefault == false {
                                    Button("Use This Mac") { model.chooseXboxLocation(useDefault: true) }.disabled(model.accountBusy)
                                }
                            }
                        }
                    }.padding(12)
                }
                HStack(spacing: 12) {
                    if model.accountBusy { ProgressView().controlSize(.small) }
                    Text(model.accountMessage).font(.callout).textSelection(.enabled)
                }
                GroupBox("Epic Games & Battle.net") {
                    VStack(alignment: .leading, spacing: 10) {
                        Text("Installed games already appear in Soju. Full account imports for these platforms are not available yet. Open the official launcher to browse your owned games.")
                            .font(.callout).foregroundStyle(.secondary)
                        HStack {
                            Button("Open Epic Games") { model.launch(.epic) }.disabled(model.busy)
                            Button("Open Battle.net") { model.launch(.battlenet) }.disabled(model.busy)
                        }
                        Text("Already have a Battle.net game on this Mac or an external drive, from another app or a backup? Soju can use that folder instead of downloading again.")
                            .font(.callout).foregroundStyle(.secondary)
                        Button("Use Existing Battle.net Game…") { model.importBattlenetGame() }.disabled(model.busy)
                    }.padding(12).frame(maxWidth: .infinity, alignment: .leading)
                }
                Text("Imports are snapshots stored on this Mac. Sync again after purchases or account changes. Disconnect removes the import (and Soju’s saved Steam web session or Microsoft sign-in), without removing installed games.")
                    .font(.caption).foregroundStyle(.secondary)
            }.padding(28)
        }.onAppear(perform: loadUsers)
            .confirmationDialog("Sign out of Xbox PC?", isPresented: $confirmDisconnect) {
                Button("Sign Out", role: .destructive) { model.forgetAccount(.xbox) }
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
    /// "1 game · updated Nov 2", without the internal account number some imports carry ("Galaxy · …1379").
    private func summaryLine(_ value: AccountSummary) -> String {
        let label = value.label.components(separatedBy: " · ").filter { !$0.hasPrefix("…") }.joined(separator: " · ")
        let games = value.count == 1 ? "1 game" : "\(value.count) games"
        var updated = String(value.updatedAt.prefix(10))
        let parse = DateFormatter(); parse.dateFormat = "yyyy-MM-dd"; parse.locale = Locale(identifier: "en_US_POSIX")
        if let date = parse.date(from: updated) {
            let show = DateFormatter(); show.setLocalizedDateFormatFromTemplate("MMM d"); updated = show.string(from: date)
        }
        return [label, games, "updated " + updated].filter { !$0.isEmpty }.joined(separator: " · ")
    }
    private func accountHeader(_ platform: Launcher) -> some View {
        HStack {
            Label(platform.title, systemImage: platform.symbol).font(.headline)
            if let value = summary(platform) {
                Text(verbatim: summaryLine(value)).font(.caption).foregroundStyle(.secondary)
                Spacer()
                Button(platform == .xbox ? "Sign Out…" : "Disconnect") {
                    if platform == .xbox { confirmDisconnect = true } else { model.forgetAccount(platform) }
                }.disabled(model.accountBusy)
            } else { Spacer(); Text("Not connected").font(.caption).foregroundStyle(.secondary) }
        }
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
                usersMessage = users.isEmpty ? "No owned Galaxy library found yet." : ""
            case .failure(let error): usersMessage = error.localizedDescription
            }
        }
    }
}
