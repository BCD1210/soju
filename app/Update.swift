import SwiftUI
import AppKit

/// Asks GitHub for the release list when Soju opens and again every few hours (Soju stays open for days
/// at a time), and remembers a newer version, if any.
/// The request carries nothing about this Mac beyond what any download from GitHub does.
@MainActor final class UpdateModel: ObservableObject {
    @Published var available: String?
    @Published var checked = false
    /// The last check did not reach GitHub (offline, or its 60-an-hour limit for an address): not the same as up to date.
    @Published var unreachable = false
    private var loaded = false

    func load(current: String) {
        guard !loaded else { return }
        loaded = true
        fetch(current: current)
    }

    /// The same check again, for a Soju that has been open since an earlier release.
    func recheck(current: String) {
        checked = false
        fetch(current: current)
    }

    private func fetch(current: String) {
        // SOJU_UPDATE_FILE points at a saved release list for checking the card without the network.
        if let path = ProcessInfo.processInfo.environment["SOJU_UPDATE_FILE"],
           let data = FileManager.default.contents(atPath: path) {
            available = UpdateCheck.latest(from: data).flatMap { UpdateCheck.isNewer($0, than: current) ? $0 : nil }
            checked = true
            return
        }
        var request = URLRequest(url: UpdateCheck.feed)
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.timeoutInterval = 15
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        URLSession.shared.dataTask(with: request) { data, response, _ in
            let ok = (response as? HTTPURLResponse)?.statusCode == 200
            let latest = ok ? data.flatMap { UpdateCheck.latest(from: $0) } : nil
            DispatchQueue.main.async {
                self.available = latest.flatMap { UpdateCheck.isNewer($0, than: current) ? $0 : nil }
                self.unreachable = !ok
                self.checked = true
            }
        }.resume()
    }
}

/// Downloads and installs a release through scripts/self-update.sh, which checks the
/// Developer ID team, notarization and version before it replaces the app.
@MainActor final class SelfUpdate: ObservableObject {
    @Published var state = ""      // "", "downloading", "ready", "failed: …"
    private var timer: Timer?

    func start(_ version: String) {
        let base = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".battlenet-macos")
        let logs = base.appendingPathComponent("logs")
        try? FileManager.default.createDirectory(at: logs, withIntermediateDirectories: true)
        let stateFile = logs.appendingPathComponent("self-update.state")
        try? FileManager.default.removeItem(at: stateFile)
        let root = Bundle.main.resourceURL!.appendingPathComponent("soju")
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/bin/bash")
        p.arguments = [root.appendingPathComponent("scripts/desktop-action.sh").path, "self-update", version,
                       Bundle.main.bundlePath, String(ProcessInfo.processInfo.processIdentifier), stateFile.path]
        var env = ProcessInfo.processInfo.environment
        env["SOJU_BASE"] = base.path
        env["PATH"] = "/usr/bin:/bin:/usr/sbin:/sbin"
        p.environment = env
        do { try p.run() } catch { state = "failed: " + error.localizedDescription; return }
        state = "downloading"
        timer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            Task { @MainActor in
                guard let self else { return }
                let text = (try? String(contentsOf: stateFile, encoding: .utf8))?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                if text == "ready" {
                    self.timer?.invalidate(); NSApp.terminate(nil)
                } else if text.hasPrefix("failed") {
                    self.timer?.invalidate(); self.state = text
                } else if !text.isEmpty {
                    self.state = text
                }
            }
        }
    }
}

/// True when this Soju has no process of its own running: a game or launcher it started writes to Soju,
/// and closing Soju for the update would cut it off. Steam or Battle.net left running from an earlier
/// session, or started in the background, are not Soju's own and go on through an update.
func sojuHasNoChildren() -> Bool {
    let p = Process()
    p.executableURL = URL(fileURLWithPath: "/usr/bin/pgrep")
    p.arguments = ["-P", String(ProcessInfo.processInfo.processIdentifier)]
    p.standardOutput = FileHandle.nullDevice
    p.standardError = FileHandle.nullDevice
    guard (try? p.run()) != nil else { return false }
    p.waitUntilExit()
    return p.terminationStatus != 0
}

/// True when no Wine server runs, so no launcher or game would be cut off by an update.
func wineIsIdle() -> Bool {
    let p = Process()
    p.executableURL = URL(fileURLWithPath: "/usr/bin/pgrep")
    p.arguments = ["-x", "wineserver"]
    p.standardOutput = FileHandle.nullDevice
    p.standardError = FileHandle.nullDevice
    guard (try? p.run()) != nil else { return false }
    p.waitUntilExit()
    return p.terminationStatus != 0
}

/// Sidebar card shown only when a newer release exists. The update installs itself as soon as nothing
/// Soju started is still running (and Soju is not installing something), then Soju restarts. Waiting for
/// every Wine process to end left most players on old releases: Steam and Battle.net stay open (play
/// counts, 48 hours to 2026-11-25: 30 of 31 Macs that pressed Play ran a release older than that day's). The button
/// does it at once. SOJU_NO_AUTO_UPDATE=1 leaves it to the button. When Soju is up to date,
/// `upToDate` runs once, for the component check.
struct UpdateCard: View {
    let current: String
    var busy = false
    var upToDate: () -> Void = {}
    @StateObject private var updates = UpdateModel()
    @StateObject private var installer = SelfUpdate()
    @State private var autoTimer: Timer?
    @State private var recheckTimer: Timer?
    @State private var reported = false

    private func tryAutoInstall() {
        guard let version = updates.available, installer.state.isEmpty,
              ProcessInfo.processInfo.environment["SOJU_NO_AUTO_UPDATE"] != "1",
              !busy, wineIsIdle() || sojuHasNoChildren() else { return }
        autoTimer?.invalidate(); autoTimer = nil
        installer.start(version)
    }

    var body: some View {
        // An empty view never gets onAppear, so keep a zero-height placeholder until there is something to show.
        VStack(spacing: 0) {
            if let version = updates.available {
                VStack(alignment: .leading, spacing: 6) {
                    Label("Soju \(version) is out", systemImage: "arrow.down.circle.fill")
                        .font(.system(size: 13, weight: .semibold)).foregroundStyle(.primary)
                    Text(installer.state.isEmpty ? "You have \(current). It installs by itself while no game you started from Soju is running." :
                         installer.state == "downloading" ? "Downloading and checking the signature…" :
                         installer.state.hasPrefix("failed") ? installer.state.replacingOccurrences(of: "failed: ", with: "Could not update: ") : "Installing…")
                        .font(.system(size: 12)).foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                    HStack(spacing: 8) {
                        Button("Update and Restart") { installer.start(version) }
                            .controlSize(.small).disabled(!installer.state.isEmpty && !installer.state.hasPrefix("failed"))
                        Link("Release notes", destination: UpdateCheck.page(for: version)).font(.system(size: 12))
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 13).padding(.vertical, 11)
                .background(Color.green.opacity(0.16), in: RoundedRectangle(cornerRadius: 12))
                .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Color.green.opacity(0.45)))
                .padding(.top, 10)
            } else {
                Color.clear.frame(height: 0)
            }
        }
        .onAppear {
            updates.load(current: current)
            if recheckTimer == nil {
                recheckTimer = Timer.scheduledTimer(withTimeInterval: 3 * 3600, repeats: true) { _ in
                    Task { @MainActor in updates.recheck(current: current) }
                }
            }
        }
        .onChange(of: updates.checked) { _, done in
            guard done else { return }
            if updates.available != nil {
                // A failed attempt (a download cut off, a busy disk) is tried again at the next check (every
                // 3 hours), not only after a relaunch; not every minute, which would download it again and again.
                if installer.state.hasPrefix("failed") { installer.state = "" }
                tryAutoInstall()
                if autoTimer == nil {
                    autoTimer = Timer.scheduledTimer(withTimeInterval: 60, repeats: true) { _ in
                        Task { @MainActor in tryAutoInstall() }
                    }
                }
            } else if !reported && !updates.unreachable {
                reported = true
                upToDate()
            }
        }
    }
}
