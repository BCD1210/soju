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
