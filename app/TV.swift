import SwiftUI

/// While streaming is meant to be on, bring Sunshine back if it quits by itself (it can when a stream drops).
enum TVKeeper {
    static let app = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Applications/Sunshine.app")
    static func start() {
        Timer.scheduledTimer(withTimeInterval: 10, repeats: true) { _ in check() }
    }
    static func check() {
        guard UserDefaults.standard.bool(forKey: "tvStreaming"), FileManager.default.fileExists(atPath: app.path) else { return }
        let probe = Process()
        probe.executableURL = URL(fileURLWithPath: "/usr/bin/pgrep")
        probe.arguments = ["-f", app.appendingPathComponent("Contents/MacOS/Sunshine").path]
        probe.terminationHandler = { finished in
            guard finished.terminationStatus != 0 else { return }
            let open = Process()
            open.executableURL = URL(fileURLWithPath: "/usr/bin/open")
            open.arguments = ["-g", app.path]
            try? open.run()
        }
        try? probe.run()
    }
}

/// Play on a TV, iPad or phone: Soju runs Sunshine on this Mac and the other device uses Moonlight.
struct TVStatus: Decodable {
    var installed = false
    var version = ""
    var running = false
    var permission = true
    var devices: [String] = []
    var pending = 0
}

struct TVView: View {
    @EnvironmentObject var model: SojuModel
    @State private var status = TVStatus()
    @State private var pin = ""
    @State private var timer: Timer?
    private let accent = Color(red: 0.36, green: 0.75, blue: 0.55)

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                VStack(alignment: .leading, spacing: 7) {
                    Text("TV & iPad").font(.system(size: 30, weight: .bold, design: .rounded))
                    Text("Games keep running on this Mac. Your TV, iPad or phone shows the picture and sends the controls back over your home Wi‑Fi.")
                        .font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                }
                if model.busy { Label(model.activity, systemImage: "hourglass").font(.system(size: 12)) }
                step(1, "Turn on streaming", done: status.installed && status.running && status.permission) {
                    if !status.installed {
                        Text("Soju installs Sunshine, a free open-source streaming server signed by LizardByte. It sends no usage data, and its settings stay on this Mac.")
                        Button("Set Up Streaming") { wantOn(true); model.run(["tv", "setup"], title: "Setting up streaming…") }
                            .buttonStyle(.borderedProminent).tint(accent).disabled(model.busy)
                    } else if !status.permission {
                        Text("macOS needs your OK before Sunshine can show your screen. In Screen & System Audio Recording, turn on Sunshine, then press Start.")
                        HStack {
                            Button("Open Settings") { NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_ScreenCapture")!) }
                            Button("Start") { model.run(["tv", "stop"], title: "Restarting…"); DispatchQueue.main.asyncAfter(deadline: .now() + 2) { model.run(["tv", "start"], title: "Starting streaming…") } }
                        }
                    } else {
                        HStack(spacing: 10) {
                            Label(status.running ? "On" : "Off", systemImage: status.running ? "dot.radiowaves.left.and.right" : "pause.circle")
                                .foregroundStyle(status.running ? accent : .secondary)
                            Button(status.running ? "Turn Off" : "Turn On") {
                                wantOn(!status.running)
                                model.run(["tv", status.running ? "stop" : "start"], title: status.running ? "Stopping…" : "Starting streaming…")
                            }.disabled(model.busy)
                        }
                    }
                }
                step(2, "Get Moonlight on the other device", done: !status.devices.isEmpty) {
                    Text("Moonlight is free and open source. Your TV or iPad and this Mac must be on the same network.")
                    HStack(spacing: 14) {
                        Link("iPad, iPhone, Apple TV", destination: URL(string: "https://apps.apple.com/app/moonlight-game-streaming/id1000551566")!)
                        Link("Android, Google TV", destination: URL(string: "https://play.google.com/store/apps/details?id=com.limelight")!)
                        Link("Other devices", destination: URL(string: "https://moonlight-stream.org")!)
                    }.font(.system(size: 12))
                }
                step(3, "Connect", done: !status.devices.isEmpty) {
                    Text("Open Moonlight and choose \u{201C}Soju\u{201D}. It shows a 4-digit PIN. Type it here.")
                    HStack {
                        TextField("PIN", text: $pin).textFieldStyle(.roundedBorder).frame(width: 90)
                            .onSubmit(pair)
                        Button("Connect", action: pair).buttonStyle(.borderedProminent).tint(accent)
                            .disabled(model.busy || !status.running || pin.range(of: "^\\d{4}$", options: .regularExpression) == nil)
                        if status.pending > 0 { Text("A device is waiting.").font(.system(size: 11)).foregroundStyle(.secondary) }
                    }
                    if !status.devices.isEmpty {
                        VStack(alignment: .leading, spacing: 6) {
                            ForEach(status.devices, id: \.self) { name in
                                HStack {
                                    Image(systemName: "display").foregroundStyle(.secondary)
                                    Text(name)
                                    Button("Remove") { model.run(["tv", "unpair", name], title: "Removing " + name + "…") }
                                        .buttonStyle(.link).disabled(model.busy)
                                }
                            }
                        }.padding(.top, 4)
                    }
                }
                VStack(alignment: .leading, spacing: 6) {
                    Label("Samsung or LG TV, or a smart monitor", systemImage: "airplayvideo").font(.system(size: 15, weight: .semibold))
                    Text("These don't have Moonlight, but most take AirPlay, built into macOS. Open Control Center, choose Screen Mirroring, then your TV. Drag the game window onto the TV (or switch the game to full screen there). Play with the keyboard, mouse or controller connected to this Mac.")
                        .font(.system(size: 12)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                }.padding(14).frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.primary.opacity(0.04), in: RoundedRectangle(cornerRadius: 12))
                step(4, "Play", done: false) {
                    Text("In Moonlight, open Desktop. You see this Mac's screen: start a game from Soju as usual. Mouse and keyboard work, and a controller on the TV acts as mouse and keys; games that only take a controller may not respond, because the Mac version of Sunshine can't create a virtual controller.")
                }
            }.padding(28).frame(maxWidth: 760, alignment: .leading)
        }
        .onAppear { refresh(); timer = Timer.scheduledTimer(withTimeInterval: 3, repeats: true) { _ in DispatchQueue.main.async { refresh() } } }
        .onDisappear { timer?.invalidate(); timer = nil }
    }

    private func pair() {
        let code = pin.trimmingCharacters(in: .whitespaces)
        guard code.range(of: "^\\d{4}$", options: .regularExpression) != nil else { return }
        pin = ""
        model.run(["tv", "pin", code], title: "Connecting…")
    }

    /// Remembered so Soju can bring Sunshine back if it quits on its own (it can when a stream drops).
    private func wantOn(_ on: Bool) { UserDefaults.standard.set(on, forKey: "tvStreaming") }

    private func refresh() {
        guard !model.busy else { return }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        process.arguments = [model.root.appendingPathComponent("scripts/stream_tv.py").path, "status"]
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice
        process.terminationHandler = { _ in
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            if let next = try? JSONDecoder().decode(TVStatus.self, from: data) {
                DispatchQueue.main.async {
                    status = next
                }
            }
        }
        try? process.run()
    }

    private func step<Content: View>(_ number: Int, _ title: String, done: Bool, @ViewBuilder content: () -> Content) -> some View {
        HStack(alignment: .top, spacing: 14) {
            ZStack {
                Circle().fill(done ? accent : Color.primary.opacity(0.08)).frame(width: 28, height: 28)
                if done { Image(systemName: "checkmark").font(.system(size: 12, weight: .bold)).foregroundStyle(.white) }
                else { Text(String(number)).font(.system(size: 13, weight: .semibold)) }
            }
            VStack(alignment: .leading, spacing: 8) {
                Text(title).font(.system(size: 15, weight: .semibold))
                content().font(.system(size: 12)).fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}
