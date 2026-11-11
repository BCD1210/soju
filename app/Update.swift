import SwiftUI
import AppKit

/// Asks GitHub for the release list once per launch and remembers a newer version, if any.
/// The request carries nothing about this Mac beyond what any download from GitHub does.
@MainActor final class UpdateModel: ObservableObject {
    @Published var available: String?
    private var loaded = false

    func load(current: String) {
        guard !loaded else { return }
        loaded = true
        // SOJU_UPDATE_FILE points at a saved release list for checking the card without the network.
        if let path = ProcessInfo.processInfo.environment["SOJU_UPDATE_FILE"],
           let data = FileManager.default.contents(atPath: path) {
            available = UpdateCheck.latest(from: data).flatMap { UpdateCheck.isNewer($0, than: current) ? $0 : nil }
            return
        }
        var request = URLRequest(url: UpdateCheck.feed)
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.timeoutInterval = 15
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        URLSession.shared.dataTask(with: request) { data, response, _ in
            let latest = (response as? HTTPURLResponse)?.statusCode == 200 ? data.flatMap { UpdateCheck.latest(from: $0) } : nil
            DispatchQueue.main.async {
                self.available = latest.flatMap { UpdateCheck.isNewer($0, than: current) ? $0 : nil }
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

/// Sidebar card shown only when a newer release exists: installs it in place, or opens the release page.
struct UpdateCard: View {
    let current: String
    @StateObject private var updates = UpdateModel()
    @StateObject private var installer = SelfUpdate()

    var body: some View {
        // An empty view never gets onAppear, so keep a zero-height placeholder until there is something to show.
        VStack(spacing: 0) {
            if let version = updates.available {
                VStack(alignment: .leading, spacing: 6) {
                    Label("Soju \(version) is out", systemImage: "arrow.down.circle.fill")
                        .font(.system(size: 12, weight: .semibold)).foregroundStyle(.primary)
                    Text(installer.state.isEmpty ? "You have \(current)." :
                         installer.state == "downloading" ? "Downloading and checking the signature…" :
                         installer.state.hasPrefix("failed") ? installer.state.replacingOccurrences(of: "failed: ", with: "Could not update: ") : "Installing…")
                        .font(.system(size: 11)).foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                    HStack(spacing: 8) {
                        Button("Update and Restart") { installer.start(version) }
                            .controlSize(.small).disabled(!installer.state.isEmpty && !installer.state.hasPrefix("failed"))
                        Link("Release notes", destination: UpdateCheck.page(for: version)).font(.system(size: 11))
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
        .onAppear { updates.load(current: current) }
    }
}
