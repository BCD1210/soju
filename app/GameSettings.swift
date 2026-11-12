import SwiftUI

/// scripts/game-settings.py show <game id>
struct GameSettings: Decodable {
    struct Renderer: Decodable {
        let available: Bool
        let value: String?
        let reason: String?
        let recommended: String?
        let why: String?
    }
    let renderer: Renderer?
}

extension SojuModel {
    /// Runs scripts/game-settings.py and returns its JSON, or the reason it failed.
    func gameSettings(_ args: [String]) async -> Result<GameSettings, ServiceFailure> {
        do {
            let (p, url) = try task(["game-settings"] + args, logName: "game-settings")
            let status: Int32 = try await withCheckedThrowingContinuation { done in
                p.terminationHandler = { done.resume(returning: $0.terminationStatus) }
                do { try p.run() } catch { done.resume(throwing: error) }
            }
            let text = (try? String(contentsOf: url, encoding: .utf8)) ?? ""
            if status == 0, let line = text.split(separator: "\n").last,
               let value = try? JSONDecoder().decode(GameSettings.self, from: Data(line.utf8)) {
                return .success(value)
            }
            let reason = text.split(separator: "\n").last(where: { $0.hasPrefix("soju: ") }).map { String($0.dropFirst(6)) }
            return .failure(ServiceFailure(message: reason ?? "The setting could not be read."))
        } catch {
            return .failure(ServiceFailure(message: error.localizedDescription))
        }
    }
}

/// The settings of one game, in its detail sheet: what used to be spread over
/// Platforms and the command line.
struct GameSettingsSection: View {
    @EnvironmentObject var model: SojuModel
    let game: InstalledGame
    @State private var settings: GameSettings?
    @State private var saving = false
    @State private var problem: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Settings").font(.system(size: 13, weight: .semibold))
            if let opened = model.preferences.opened[game.id] {
                Label("Last played " + opened.formatted(date: .abbreviated, time: .shortened), systemImage: "clock")
                    .font(.system(size: 12)).foregroundStyle(.secondary)
            }
            if game.platform != .didim {
                Toggle("Show FPS while playing (applies to every game)", isOn: $model.fpsHUD)
                    .toggleStyle(.checkbox).font(.system(size: 12))
            }
            if let renderer = settings?.renderer {
                if renderer.available {
                    HStack(spacing: 10) {
                        Text("Graphics").font(.system(size: 12))
                        Picker("Graphics", selection: Binding(
                            get: { renderer.value ?? "dxmt" },
                            set: { choose($0) })) {
                            Text("Standard").tag("dxmt")
                            Text("DXVK").tag("dxvk")
                        }.pickerStyle(.segmented).labelsHidden().frame(width: 190).disabled(saving)
                            .accessibilityLabel("Graphics for " + game.title)
                        if saving { ProgressView().controlSize(.small) }
                    }
                    Text(renderer.recommended == "dxvk"
                         ? "DXVK is recommended for this game. " + (renderer.why ?? "")
                         : "Standard suits most games. Try DXVK if this one freezes or draws wrongly. Quit Steam before changing it.")
                        .font(.system(size: 11)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                } else if renderer.reason == "gptk" {
                    Label("Steam is in Overwatch and Diablo II mode, so this game runs on Apple GPTK. Switch the mode in Platforms.", systemImage: "info.circle")
                        .font(.system(size: 11)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                }
            }
            if let problem {
                Label(problem, systemImage: "exclamationmark.triangle").font(.system(size: 11)).foregroundStyle(.orange)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .task(id: game.id) {
            guard game.platform == .steam, game.isInstalled else { return }
            if case .success(let value) = await model.gameSettings(["show", game.id]) { settings = value }
        }
    }

    private func choose(_ value: String) {
        guard !saving, value != settings?.renderer?.value else { return }
        saving = true; problem = nil
        Task {
            switch await model.gameSettings(["renderer", game.id, value]) {
            case .success(let next): settings = next
            case .failure(let failure): problem = failure.message
            }
            saving = false
        }
    }
}
