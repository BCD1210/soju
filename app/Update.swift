import SwiftUI

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

/// Sidebar card shown only when a newer release exists. It opens the release page; Soju does not replace itself.
struct UpdateCard: View {
    let current: String
    @StateObject private var updates = UpdateModel()

    var body: some View {
        // An empty view never gets onAppear, so keep a zero-height placeholder until there is something to show.
        VStack(spacing: 0) {
            if let version = updates.available {
                Link(destination: UpdateCheck.page(for: version)) {
                    VStack(alignment: .leading, spacing: 4) {
                        Label("Soju \(version) is out", systemImage: "arrow.down.circle.fill")
                            .font(.system(size: 12, weight: .semibold)).foregroundStyle(.primary)
                        Text("You have \(current). Open the download page.")
                            .font(.system(size: 11)).foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 13).padding(.vertical, 11)
                    .background(Color.green.opacity(0.16), in: RoundedRectangle(cornerRadius: 12))
                    .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Color.green.opacity(0.45)))
                }
                .buttonStyle(.plain).help("Release notes and download for Soju \(version)")
                .padding(.top, 10)
            } else {
                Color.clear.frame(height: 0)
            }
        }
        .onAppear { updates.load(current: current) }
    }
}
