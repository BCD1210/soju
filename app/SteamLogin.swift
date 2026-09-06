import SwiftUI
import WebKit

enum SteamWebSession {
    // WebKit's store belongs to Soju, separate from Safari and the Steam client.
    static let dataStore = WKWebsiteDataStore.default()
    static func clear(completion: @escaping () -> Void) {
        dataStore.removeData(ofTypes: WKWebsiteDataStore.allWebsiteDataTypes(), modifiedSince: .distantPast, completionHandler: completion)
    }
}

@MainActor final class SteamLoginController: NSObject, ObservableObject, WKNavigationDelegate, WKUIDelegate {
    @Published var message = "Sign in on Steam's official page. Steam Guard and QR sign-in stay with Steam."
    @Published var host = "steamcommunity.com"
    @Published var reading = false
    let web: WKWebView
    private let reader: String
    private var complete: (([String: Any]) -> Void)?
    private var timer: Timer?
    private var generation = UUID()
    private var readingGeneration: UUID?
    private var openedLibrary = false
    private var active = true

    init(root: URL) {
        let config = WKWebViewConfiguration()
        config.websiteDataStore = SteamWebSession.dataStore
        config.preferences.javaScriptCanOpenWindowsAutomatically = false
        web = WKWebView(frame: .zero, configuration: config)
        reader = (try? String(contentsOf: root.appendingPathComponent("resources/steam-library.js"), encoding: .utf8)) ?? ""
        super.init()
        web.navigationDelegate = self
        web.uiDelegate = self
    }
    func start(completion: @escaping ([String: Any]) -> Void) {
        guard complete == nil else { return }
        complete = completion
        web.load(URLRequest(url: URL(string: "https://steamcommunity.com/login/home/?goto=my%2Fgames%2F%3Ftab%3Dall")!))
        timer = Timer.scheduledTimer(withTimeInterval: 3, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.checkLibrary() }
        }
    }
    func stop() {
        active = false; generation = UUID(); complete = nil
        timer?.invalidate(); timer = nil
        web.stopLoading()
    }
    func retry() {
        openedLibrary = false
        message = "Loading your Steam library…"
        web.load(URLRequest(url: URL(string: "https://steamcommunity.com/my/games/?tab=all")!))
    }
    private func allowed(_ url: URL?) -> Bool {
        guard let url, url.scheme == "https", url.user == nil, url.password == nil,
              url.port == nil || url.port == 443 else { return false }
        return ["steamcommunity.com", "store.steampowered.com", "login.steampowered.com", "help.steampowered.com"].contains(url.host?.lowercased() ?? "")
    }
    func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction, decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        let main = action.targetFrame?.isMainFrame ?? true
        // Steam may use third-party HTTPS CAPTCHA frames. They cannot become the main page.
        if !main {
            decisionHandler(action.request.url?.scheme == "https" || action.request.url?.absoluteString == "about:blank" ? .allow : .cancel)
            return
        }
        guard allowed(action.request.url) else {
            message = "This window only opens official Steam pages."
            decisionHandler(.cancel); return
        }
        if action.targetFrame == nil {
            decisionHandler(.cancel); webView.load(action.request); return
        }
        decisionHandler(.allow)
    }
    func webView(_ webView: WKWebView, didStartProvisionalNavigation navigation: WKNavigation!) {
        generation = UUID(); readingGeneration = nil; reading = false
        host = allowed(webView.url) ? webView.url!.host! : "Steam"
    }
    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        host = allowed(webView.url) ? webView.url!.host! : "Steam"
        checkLibrary()
    }
    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
        if (error as NSError).code != NSURLErrorCancelled { message = "Steam could not load. Check your connection and retry." }
    }
    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        if (error as NSError).code != NSURLErrorCancelled { message = "Steam could not load. Check your connection and retry." }
    }
    private func checkLibrary() {
        guard active, !web.isLoading, readingGeneration == nil, !reader.isEmpty,
              allowed(web.url), ["steamcommunity.com", "store.steampowered.com"].contains(web.url?.host ?? "") else { return }
        let current = generation
        readingGeneration = current
        web.callAsyncJavaScript(reader, arguments: [:], in: nil, in: .page) { [weak self] result in
            guard let self, self.active, self.generation == current else { return }
            self.readingGeneration = nil
            guard case .success(let raw) = result, let value = raw as? [String: Any], let state = value["state"] as? String else { return }
            switch state {
            case "ready":
                self.reading = true; self.message = "Importing your games…"
                self.timer?.invalidate(); self.timer = nil
                let callback = self.complete
                self.complete = nil; self.active = false
                callback?(value)
            case "open-library":
                if !self.openedLibrary {
                    self.openedLibrary = true
                    self.web.load(URLRequest(url: URL(string: "https://steamcommunity.com/my/games/?tab=all")!))
                } else { self.message = "Let your All Games page load, then choose Retry import if needed." }
            case "unavailable":
                self.message = "Steam could not share a complete library. Retry import after the page loads. Your previous import is kept."
                self.timer?.invalidate(); self.timer = nil
            default: break
            }
        }
    }
}

struct SteamLoginWebView: NSViewRepresentable {
    let controller: SteamLoginController
    func makeNSView(context: Context) -> WKWebView { controller.web }
    func updateNSView(_ nsView: WKWebView, context: Context) {}
}

struct SteamLoginView: View {
    @StateObject private var controller: SteamLoginController
    let completion: ([String: Any]) -> Void
    let cancel: () -> Void
    init(root: URL, completion: @escaping ([String: Any]) -> Void, cancel: @escaping () -> Void) {
        _controller = StateObject(wrappedValue: SteamLoginController(root: root))
        self.completion = completion; self.cancel = cancel
    }
    var body: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 5) {
                    Text("Sign in to Steam").font(.headline)
                    Label("https://" + controller.host, systemImage: "lock.fill").font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Button("Retry import") { controller.retry() }.disabled(controller.reading)
                Button("Cancel", action: cancel).keyboardShortcut(.cancelAction)
            }.padding(16)
            Divider()
            SteamLoginWebView(controller: controller)
            Divider()
            Text(controller.message).font(.callout).frame(maxWidth: .infinity, alignment: .leading).padding(16)
        }.frame(width: 940, height: 680)
            .onAppear { controller.start(completion: completion) }
            .onDisappear { controller.stop() }
    }
}
