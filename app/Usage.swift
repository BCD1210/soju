import SwiftUI

/// Opt-in, anonymous usage counts. Off until the user says yes. Each event is just a name, the Soju
/// version and a random ID made when counts were turned on (deleted when they are turned off).
enum Usage {
    static let defaults = UserDefaults.standard
    static var asked: Bool { defaults.object(forKey: "usageCounts") != nil }
    static var enabled: Bool { defaults.bool(forKey: "usageCounts") }

    static func set(_ on: Bool) {
        defaults.set(on, forKey: "usageCounts")
        if on { defaults.set(UUID().uuidString, forKey: "usageInstall") } else { defaults.removeObject(forKey: "usageInstall") }
    }

    static func send(_ event: String) {
        guard enabled, let install = defaults.string(forKey: "usageInstall") else { return }
        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0"
        var request = URLRequest(url: URL(string: "https://didimlabs.com/api/stats")!)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try? JSONSerialization.data(withJSONObject: ["event": event, "version": version, "install": install])
        request.timeoutInterval = 10
        URLSession.shared.dataTask(with: request).resume()
    }
}

/// Shown once at the bottom of the library until the user answers.
struct UsagePrompt: View {
    @State private var answered = Usage.asked
    var body: some View {
        if !answered {
            HStack(spacing: 12) {
                Image(systemName: "chart.bar").foregroundStyle(.secondary)
                VStack(alignment: .leading, spacing: 2) {
                    Text("Send anonymous usage counts?").font(.system(size: 12, weight: .semibold))
                    Text("Counts of app opens, demo downloads and purchases, so we can tell what works. No names, emails or game lists.")
                        .font(.system(size: 11)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                }
                Spacer()
                Link("Details", destination: URL(string: "https://didimlabs.com/privacy.html#app")!).font(.system(size: 11))
                Button("No thanks") { Usage.set(false); answered = true }
                Button("Allow") { Usage.set(true); answered = true; Usage.send("app_open") }.buttonStyle(.borderedProminent)
            }
            .padding(12).background(Color.primary.opacity(0.05), in: RoundedRectangle(cornerRadius: 10))
        }
    }
}

/// Release news by email. The address goes to didimlabs.com/api/notify and nowhere else.
struct NotifyRow: View {
    @State private var email = ""
    @State private var status = ""
    @State private var sending = false
    var body: some View {
        HStack(spacing: 8) {
            Text("Hear about our next game:").font(.system(size: 12))
            TextField("Email", text: $email).textFieldStyle(.roundedBorder).frame(maxWidth: 220)
                .onSubmit(send)
            Button(sending ? "Sending…" : "Email me", action: send)
                .disabled(sending || email.range(of: "^[^\\s@]+@[^\\s@]+\\.[^\\s@]{2,}$", options: .regularExpression) == nil)
            Text(status).font(.system(size: 11)).foregroundStyle(.secondary)
            Spacer()
        }
    }
    private func send() {
        let address = email.trimmingCharacters(in: .whitespaces)
        guard !sending, !address.isEmpty else { return }
        sending = true; status = ""
        var request = URLRequest(url: URL(string: "https://didimlabs.com/api/notify")!)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try? JSONSerialization.data(withJSONObject: ["email": address, "source": "soju"])
        request.timeoutInterval = 15
        URLSession.shared.dataTask(with: request) { data, response, _ in
            let ok = (response as? HTTPURLResponse)?.statusCode == 200
            let message = (data.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }?["error"] as? String)
            DispatchQueue.main.async {
                sending = false
                if ok { email = ""; status = "Thanks. One email when it's out, nothing else." }
                else { status = message ?? "Couldn't save that. Try again later." }
            }
        }.resume()
    }
}
