import SwiftUI

/// `soju vr status --json` (scripts/d2r-vr.py status).
struct VRStatus: Decodable {
    var installed: Bool
    var current: Bool
    var game_build: String?
    var supported_build: String
    var source_build: String?
    var source_supported: Bool?
    var ready: Bool { installed && current }
}

extension SojuModel {
    /// Reads the D2R VR state in the background; nil when it cannot be read.
    func vrStatus(_ done: @escaping (VRStatus?) -> Void) {
        do {
            let (p, url) = try task(["vr", "status"], logName: "d2r-vr-status")
            p.terminationHandler = { _ in
                let text = (try? String(contentsOf: url, encoding: .utf8)) ?? ""
                let line = text.split(separator: "\n").last(where: { $0.hasPrefix("{") }).map(String.init) ?? ""
                let status = try? JSONDecoder().decode(VRStatus.self, from: Data(line.utf8))
                DispatchQueue.main.async { done(status) }
            }
            try p.run()
        } catch {
            done(nil)
        }
    }
}

/// D2R in VR and in first person on the Mac screen, step by step (D2R VR Unleashed, offline characters).
struct D2RVRSheet: View {
    @ObservedObject var model: SojuModel
    let game: InstalledGame
    let close: () -> Void
    @AppStorage("vrHeadsetAppReady") private var headsetReady = false
    @AppStorage("vrHeadset") private var headset = "quest"
    @AppStorage("vrFlatView") private var flatView = "first"
    @State private var status: VRStatus?
    @State private var loading = true
    private let green = Color(red: 0.16, green: 0.62, blue: 0.38)
    static let questGuide = URL(string: "https://github.com/anoshmisiosdev/oxrsys/blob/main/docs/platforms/quest.md")!
    static let oxrsys = URL(string: "https://github.com/anoshmisiosdev/oxrsys")!

    private var setUp: Bool { status?.ready == true }
    private var blocked: String? {
        guard let status, status.source_supported == false, let build = status.source_build else { return nil }
        return "D2R updated to " + build + ". VR support is being updated for it."
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack(alignment: .firstTextBaseline) {
                Text("D2R in VR").font(.system(size: 22, weight: .bold, design: .rounded))
                Spacer()
                Button { close() } label: { Image(systemName: "xmark.circle.fill").font(.system(size: 18)) }
                    .buttonStyle(.plain).foregroundStyle(.secondary).keyboardShortcut(.cancelAction)
            }
            Text("Experimental · offline characters only · not yet tried on a real headset. Your normal D2R and its online play are not changed.")
                .font(.system(size: 12)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)

            step(1, "Headset app", done: headsetReady) {
                Picker("", selection: $headset) {
                    Text("Meta Quest").tag("quest")
                    Text("PICO").tag("pico")
                }.pickerStyle(.segmented).labelsHidden().frame(width: 220)
                Text("Install the free OXRSys app on the headset (sideloaded), on the same Wi-Fi as this Mac.")
                    .font(.system(size: 12)).foregroundStyle(.secondary)
                HStack {
                    Button("How to install ›") { NSWorkspace.shared.open(headset == "quest" ? Self.questGuide : Self.oxrsys) }
                    Spacer()
                    Button(headsetReady ? "Installed ✓" : "I installed it") { headsetReady.toggle() }
                }
            }

            step(2, "Set up VR on this Mac", done: setUp) {
                if let blocked {
                    Label(blocked, systemImage: "exclamationmark.triangle").font(.system(size: 12)).foregroundStyle(.orange)
                } else {
                    Text(setUp ? "Ready. Set it up again after a D2R update." : "About 2 minutes and 125 MB. Copies your Battle.net D2R; the original is left as it is.")
                        .font(.system(size: 12)).foregroundStyle(.secondary)
                }
                HStack {
                    if model.busy && model.activity.contains("D2R VR") { ProgressView().controlSize(.small) }
                    Spacer()
                    Button(setUp ? "Set Up Again" : "Set Up") {
                        model.run(["vr", "setup"], title: "Setting up D2R VR…")
                    }.disabled(model.busy || blocked != nil)
                }
            }

            step(3, "Connect and play", done: false) {
                Text("Open OXRSys on the headset and press Connect, then press Play in VR. Make an offline character with the stick and A.")
                    .font(.system(size: 12)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                HStack {
                    Spacer()
                    Button { close(); model.playVR(game) } label: { Label("Play in VR", systemImage: "eyeglasses") }
                        .buttonStyle(.borderedProminent).tint(green)
                        .disabled(!setUp || !headsetReady || model.busy || model.launchingGames.contains(game.id))
                }
            }

            Divider()
            VStack(alignment: .leading, spacing: 8) {
                Text("No headset? Play on this Mac").font(.system(size: 14, weight: .semibold))
                Text("The same copy on the Mac screen: mouse to look, W A S D to walk. fn+F1 / fn+F2 / fn+F3 switch the view in the game.")
                    .font(.system(size: 12)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                HStack {
                    Picker("", selection: $flatView) {
                        Text("First person").tag("first")
                        Text("Third person").tag("third")
                        Text("Classic").tag("classic")
                    }.pickerStyle(.segmented).labelsHidden().frame(width: 300)
                    Spacer()
                    Button { close(); model.playVR(game, flat: true, view: flatView) } label: { Label("Play on Mac", systemImage: "display") }
                        .disabled(!setUp || model.busy || model.launchingGames.contains(game.id))
                }
                if !setUp && !loading {
                    Text("Set up VR first (step 2); it is the same copy.").font(.system(size: 11)).foregroundStyle(.secondary)
                }
            }
        }
        .padding(26).frame(width: 560)
        .onAppear(perform: reload)
        .onChange(of: model.busy) { _, busy in if !busy { reload() } }
    }

    private func reload() {
        loading = true
        model.vrStatus { s in status = s; loading = false }
    }

    private func step<Content: View>(_ number: Int, _ title: String, done: Bool,
                                     @ViewBuilder content: () -> Content) -> some View {
        HStack(alignment: .top, spacing: 12) {
            ZStack {
                Circle().fill(done ? green : Color.primary.opacity(0.08)).frame(width: 26, height: 26)
                if done { Image(systemName: "checkmark").font(.system(size: 12, weight: .bold)).foregroundStyle(.white) }
                else { Text(String(number)).font(.system(size: 13, weight: .semibold)) }
            }
            VStack(alignment: .leading, spacing: 8) {
                Text(title).font(.system(size: 14, weight: .semibold))
                content()
            }
        }
        .padding(12).frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 11))
    }
}
