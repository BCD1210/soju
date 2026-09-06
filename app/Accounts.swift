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
                             "SOJU_BASE": base.path, "PYTHONIOENCODING": "utf-8"]
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
    func forgetAccount(_ platform: Launcher) {
        guard !accountBusy else { return }
        do {
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
        if !installed.contains(game.platform) { libraryIssue = "Set up " + game.platform.title + " in Platforms first."; return }
        run(["game-install", game.id], title: "Opening " + game.platform.title + " to install " + game.title + "…")
    }
}

struct AccountsView: View {
    @EnvironmentObject var model: SojuModel
    @State private var showSteamLogin = false
    @State private var users: [GalaxyAccount] = []
    @State private var userID = ""
    @State private var userTask: Process?
    @State private var usersMessage = ""
    private func summary(_ platform: Launcher) -> AccountSummary? { model.accountSummaries.first { $0.platform == platform } }
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                Text("Your accounts").font(.system(size: 30, weight: .bold, design: .rounded))
                Text("Bring owned games into your library, including games you have not installed.").foregroundStyle(.secondary)
                GroupBox {
                    VStack(alignment: .leading, spacing: 12) {
                        accountHeader(.steam)
                        Text("Sign in on Steam’s official page to import your games. No API key or profile URL is needed.")
                            .font(.callout).foregroundStyle(.secondary)
                        Text("Steam handles your password, QR sign-in and Steam Guard. Your web session stays on this Mac.")
                            .font(.caption).foregroundStyle(.secondary)
                        HStack {
                            Button("Open Steam") { model.launch(.steam) }.disabled(model.busy)
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
                    }.padding(12).frame(maxWidth: .infinity, alignment: .leading)
                }
                Text("Imports are snapshots stored on this Mac. Sync again after purchases or account changes. Disconnect removes the import and Soju’s saved Steam web session, without removing installed games.")
                    .font(.caption).foregroundStyle(.secondary)
            }.padding(28)
        }.onAppear(perform: loadUsers)
            .sheet(isPresented: $showSteamLogin) {
                SteamLoginView(root: model.root, completion: { value in
                    showSteamLogin = false
                    model.importSteam(value)
                }, cancel: { showSteamLogin = false })
            }
    }
    private func accountHeader(_ platform: Launcher) -> some View {
        HStack {
            Label(platform.title, systemImage: platform.symbol).font(.headline)
            if let value = summary(platform) {
                Text(value.label + " · " + String(value.count) + " games · " + String(value.updatedAt.prefix(10))).font(.caption).foregroundStyle(.secondary)
                Spacer()
                Button("Disconnect") { model.forgetAccount(platform) }.disabled(model.accountBusy)
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
