import SwiftUI
import AppKit
import WebKit

/// The store's GOAT shelf: games people made with GOAT, listed by their makers and approved by Didim Labs.
@MainActor final class GoatShelfModel: ObservableObject {
    @Published var games: [GoatGame] = []
    @Published var loaded = false
    @Published var offline = false

    func load() {
        var request = URLRequest(url: URL(string: GoatCatalog.address)!)
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.timeoutInterval = 15
        URLSession.shared.dataTask(with: request) { data, response, _ in
            let fresh = (response as? HTTPURLResponse)?.statusCode == 200 ? data.flatMap(GoatCatalog.decode) : nil
            DispatchQueue.main.async {
                if let fresh { self.games = fresh; self.offline = false; self.loaded = true } else { self.offline = true }
            }
        }.resume()
    }
}

struct GoatShelfView: View {
    @ObservedObject var shelf: GoatShelfModel
    let accent: Color

    var body: some View {
        ScrollView {
            VStack(spacing: 18) {
                ForEach(shelf.games) { card($0) }
                if shelf.games.isEmpty {
                    Text(shelf.offline ? "The GOAT shelf is unavailable right now. Check the internet connection."
                         : shelf.loaded ? "No games here yet. Games made with GOAT show up once their makers list them." : "Loading the GOAT shelf…")
                        .foregroundStyle(.secondary).padding(.top, 40)
                }
            }
        }
    }

    private func card(_ game: GoatGame) -> some View {
        HStack(alignment: .top, spacing: 22) {
            ZStack {
                LinearGradient(colors: [accent.opacity(0.5), .black], startPoint: .topLeading, endPoint: .bottomTrailing)
                if let url = GoatCatalog.coverURL(game.cover) {
                    AsyncImage(url: url) { image in image.resizable().scaledToFill() } placeholder: { Image(systemName: "gamecontroller.fill").font(.largeTitle) }
                } else {
                    Image(systemName: "gamecontroller.fill").font(.largeTitle)
                }
            }.frame(width: 320, height: 150).clipped().clipShape(RoundedRectangle(cornerRadius: 12))
            VStack(alignment: .leading, spacing: 9) {
                Text("MADE WITH GOAT").font(.system(size: 10, weight: .bold)).tracking(0.8).foregroundStyle(accent)
                Text(game.title).font(.system(size: 22, weight: .bold, design: .rounded)).lineLimit(2)
                Text("by @" + game.maker).font(.system(size: 12, weight: .semibold))
                Text(game.summary).font(.system(size: 12)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                HStack(spacing: 10) {
                    Button { GoatPlayer.play(game) } label: { Label("Play", systemImage: "play.fill") }
                        .buttonStyle(.borderedProminent).tint(.green)
                    Text("Free · plays right here").font(.system(size: 11)).foregroundStyle(.secondary)
                }.padding(.top, 4)
            }
            Spacer(minLength: 0)
        }.padding(18).background(Color.primary.opacity(0.04), in: RoundedRectangle(cornerRadius: 16))
    }
}

/// A game's own window. The page inside is the game's address and stays there: a link to anywhere else opens in the browser.
@MainActor final class GoatPlayer: NSObject, NSWindowDelegate, WKNavigationDelegate, WKUIDelegate {
    private static var open: [String: GoatPlayer] = [:]
    private let id: String
    private let home: URL
    private let web: WKWebView
    private let window: NSWindow
    /// When this game last sent the browser somewhere: a page may not open one address after another by itself.
    private var lastOutside = Date.distantPast

    /// A link out of the game goes to the browser: https only, and at most one every few seconds.
    private func outside(_ target: URL) {
        guard target.scheme == "https", Date().timeIntervalSince(lastOutside) > 3 else { return }
        lastOutside = Date()
        NSWorkspace.shared.open(target)
    }

    static func play(_ game: GoatGame) {
        if let player = open[game.id] { player.window.makeKeyAndOrderFront(nil); NSApp.activate(ignoringOtherApps: true); return }
        guard let url = GoatCatalog.playURL(game.play) else { return }
        open[game.id] = GoatPlayer(game: game, url: url)
    }

    private init(game: GoatGame, url: URL) {
        id = game.id
        home = url
        let config = WKWebViewConfiguration()
        // Kept between plays, so a game's own save (its page's storage, apart for every game by its address) is still there next time.
        config.websiteDataStore = .default()
        config.mediaTypesRequiringUserActionForPlayback = []
        config.preferences.isElementFullscreenEnabled = true
        web = WKWebView(frame: NSRect(x: 0, y: 0, width: 1280, height: 720), configuration: config)
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1280, height: 720), styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
        super.init()
        web.navigationDelegate = self
        web.uiDelegate = self
        window.title = game.title + " · @" + game.maker
        window.isReleasedWhenClosed = false
        window.contentMinSize = NSSize(width: 480, height: 320)
        window.contentView = web
        window.delegate = self
        window.center()
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(web)
        NSApp.activate(ignoringOtherApps: true)
        web.load(URLRequest(url: url, cachePolicy: .useProtocolCachePolicy, timeoutInterval: 30))
    }

    func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction, decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        guard let target = action.request.url else { decisionHandler(.cancel); return }
        // What the game's page loads into itself (frames, about:blank, data it made) is its own business; where the window goes is not.
        if let frame = action.targetFrame, !frame.isMainFrame { decisionHandler(.allow); return }
        if action.targetFrame != nil, GoatCatalog.staysInGame(target, game: home) { decisionHandler(.allow); return }
        // Anywhere else: only a link the player pressed leaves, and it leaves to the browser. A page that sends itself
        // elsewhere (a redirect, a script) goes nowhere.
        if action.navigationType == .linkActivated { outside(target) }
        decisionHandler(.cancel)
    }

    /// window.open and links to a new tab: never a second window here.
    func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration, for action: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
        if let target = action.request.url { outside(target) }
        return nil
    }

    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
        guard (error as NSError).code != NSURLErrorCancelled else { return }
        let text = "This game could not be opened. Check the internet connection and try Play again."
        webView.loadHTMLString("<body style=\"margin:0;height:100vh;display:flex;align-items:center;justify-content:center;background:#111;color:#bbb;font:14px -apple-system\"><p>\(text)</p></body>", baseURL: nil)
    }

    func windowWillClose(_ notification: Notification) {
        // Stops its sound and its work at once, then lets go of it.
        web.stopLoading()
        web.loadHTMLString("", baseURL: nil)
        GoatPlayer.open[id] = nil
    }
}
