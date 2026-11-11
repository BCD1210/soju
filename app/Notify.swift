import SwiftUI

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

/// Wishlist for a game that isn't on sale yet: one email when it comes out. Signed in, it's one press;
/// otherwise a small field asks for the address. Remembered on this Mac so the button stays filled in.
struct WishlistButton: View {
    let game: String
    let signedInEmail: String
    let accent: Color
    @AppStorage("didimWishlist") private var saved = ""
    @State private var asking = false
    @State private var email = ""
    @State private var status = ""
    @State private var sending = false
    private var done: Bool { saved.split(separator: ",").contains(Substring(game)) }

    var body: some View {
        HStack(spacing: 8) {
            if done {
                Label("Wishlisted", systemImage: "heart.fill").font(.system(size: 12, weight: .semibold)).foregroundStyle(accent)
                    .help("We'll email you once when it's out.")
            } else if asking {
                TextField("Email", text: $email).textFieldStyle(.roundedBorder).frame(width: 200).onSubmit { send(email) }
                Button(sending ? "Saving…" : "Wishlist") { send(email) }
                    .disabled(sending || email.range(of: "^[^\\s@]+@[^\\s@]+\\.[^\\s@]{2,}$", options: .regularExpression) == nil)
                Button("Cancel") { asking = false; status = "" }.buttonStyle(.borderless).font(.system(size: 11))
            } else {
                Button { signedInEmail.isEmpty ? (asking = true) : send(signedInEmail) } label: {
                    Label(sending ? "Saving…" : "Wishlist", systemImage: "heart")
                }.disabled(sending).help("Get one email when this game comes out")
            }
            if !status.isEmpty { Text(status).font(.system(size: 11)).foregroundStyle(.secondary) }
        }
    }

    private func send(_ address: String) {
        let address = address.trimmingCharacters(in: .whitespaces)
        guard !sending, !address.isEmpty else { return }
        sending = true; status = ""
        var request = URLRequest(url: URL(string: "https://didimlabs.com/api/wishlist")!)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try? JSONSerialization.data(withJSONObject: ["email": address, "game": game])
        request.timeoutInterval = 15
        URLSession.shared.dataTask(with: request) { data, response, _ in
            let ok = (response as? HTTPURLResponse)?.statusCode == 200
            let message = (data.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }?["error"] as? String)
            DispatchQueue.main.async {
                sending = false
                if ok {
                    asking = false
                    saved = (saved.split(separator: ",").map(String.init) + [game]).joined(separator: ",")
                } else { status = message ?? "Couldn't save that. Try again later." }
            }
        }.resume()
    }
}
