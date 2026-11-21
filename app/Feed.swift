import SwiftUI
import AppKit
import ImageIO
import UniformTypeIdentifiers

/// The Didim Labs feed, the first shelf of the Didim Labs page: game news from Didim Labs and what signed-in players post,
/// one stream, with replies under a post as in a thread. Served by didimlabs.com/api/feed (site-worker/feed.js).
/// A photo on a post: its id on the site and its size, so its place is kept before it loads.
struct FeedImage: Identifiable, Hashable {
    let id: Int
    /// The photo's own key: the site gives the picture only to an address that carries it.
    let k: String
    let w: Int
    let h: Int
    var ratio: CGFloat { h > 0 ? CGFloat(w) / CGFloat(h) : 1.5 }
}

/// A photo picked for a post, already made small: the long side at most 1600 px as a JPEG under 440 KB, and a 560 px thumbnail.
/// Good on a Retina window, a tenth or less of what a camera or a screenshot saves, so storing and sending them costs next to nothing.
struct FeedPhoto: Identifiable {
    let id = UUID()
    let full: Data
    let thumb: Data
    let w: Int
    let h: Int
    let preview: NSImage

    static func make(_ url: URL) -> FeedPhoto? { CGImageSourceCreateWithURL(url as CFURL, nil).flatMap(make) }
    static func make(_ data: Data) -> FeedPhoto? { CGImageSourceCreateWithData(data as CFData, nil).flatMap(make) }

    private static func make(_ source: CGImageSource) -> FeedPhoto? {
        guard CGImageSourceGetCount(source) > 0 else { return nil }
        func scaled(_ side: Int) -> CGImage? {
            CGImageSourceCreateThumbnailAtIndex(source, 0, [kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceCreateThumbnailWithTransform: true,
                                                            kCGImageSourceThumbnailMaxPixelSize: side] as CFDictionary)
        }
        func jpeg(_ image: CGImage, _ quality: Double) -> Data? {
            NSBitmapImageRep(cgImage: image).representation(using: .jpeg, properties: [.compressionFactor: quality, .fallbackBackgroundColor: NSColor.white])
        }
        // Step down only as far as needed to fit.
        var full: (Data, CGImage)?
        for (side, quality) in [(1600, 0.8), (1600, 0.68), (1280, 0.7), (1024, 0.66), (800, 0.6)] {
            if let image = scaled(side), let data = jpeg(image, quality), data.count <= 440 * 1024 { full = (data, image); break }
        }
        var thumb: Data?
        for (side, quality) in [(560, 0.72), (480, 0.62), (360, 0.55)] {
            if let image = scaled(side), let data = jpeg(image, quality), data.count <= 85 * 1024 { thumb = data; break }
        }
        guard let full, let thumb else { return nil }
        return FeedPhoto(full: full.0, thumb: thumb, w: full.1.width, h: full.1.height, preview: NSImage(data: thumb) ?? NSImage())
    }
}

struct FeedPost: Identifiable {
    let id: Int
    let kind: String
    let nick: String
    let title: String
    let body: String
    let game: String
    let gameTitle: String
    let at: Date
    var likes: Int
    var liked: Bool
    var replies: Int
    let mine: Bool
    var tail: [FeedPost]
    var images: [FeedImage] = []
    var isNews: Bool { kind == "news" }

    /// A post this Mac just made, shown at once.
    init(id: Int, parent: Bool, kind: String, nick: String, title: String, body: String, game: String, gameTitle: String) {
        self.id = id; self.kind = kind; self.nick = nick; self.title = title; self.body = body
        self.game = parent ? "" : game; self.gameTitle = parent ? "" : gameTitle
        at = Date(); likes = 0; liked = false; replies = 0; mine = true; tail = []
    }

    init?(_ d: [String: Any]) {
        guard let id = d["id"] as? Int, let body = d["body"] as? String, let nick = d["nick"] as? String else { return nil }
        self.id = id; self.body = body; self.nick = nick
        kind = (d["kind"] as? String) ?? "player"; title = (d["title"] as? String) ?? ""
        game = (d["game"] as? String) ?? ""; gameTitle = (d["game_title"] as? String) ?? ""
        at = Date(timeIntervalSince1970: ((d["at"] as? Double) ?? 0) / 1000)
        likes = (d["likes"] as? Int) ?? 0; liked = (d["liked"] as? Bool) ?? false; replies = (d["replies"] as? Int) ?? 0
        mine = (d["mine"] as? Bool) ?? false
        tail = ((d["tail"] as? [[String: Any]]) ?? []).compactMap(FeedPost.init)
        images = ((d["images"] as? [[String: Any]]) ?? []).compactMap { i in
            guard let id = i["id"] as? Int else { return nil }
            return FeedImage(id: id, k: (i["k"] as? String) ?? "", w: (i["w"] as? Int) ?? 3, h: (i["h"] as? Int) ?? 2)
        }
    }
}

/// A post's text as it is shown: **bold**, *italic* and `code` are drawn as such, line breaks kept. A link is live only in
/// news from Didim Labs; in anyone else's post its words stay and the address behind them is dropped, so a post cannot dress
/// a strange address up as something else.
enum FeedText {
    static func shown(_ body: String, links: Bool) -> AttributedString {
        guard var text = try? AttributedString(markdown: body, options: AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace, failurePolicy: .returnPartiallyParsedIfPossible)) else { return AttributedString(body) }
        for run in text.runs where run.link != nil {
            let safe = links && run.link?.scheme == "https"
            if safe { text[run.range].underlineStyle = .single } else { text[run.range].link = nil }
        }
        return text
    }
}

/// The feed's token: given once when a sign-in finishes (AccountModel.finish), kept in a file only this user can read.
enum FeedToken {
    private static var file: URL { FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".battlenet-macos/didim-feed-token") }
    static func load() -> String {
        guard let text = try? String(contentsOf: file, encoding: .utf8) else { return "" }
        let token = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return token.range(of: "^[A-Za-z0-9_-]{40,64}$", options: .regularExpression) != nil ? token : ""
    }
    static func save(_ token: String) {
        guard token.range(of: "^[A-Za-z0-9_-]{40,64}$", options: .regularExpression) != nil else { return }
        try? FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        FileManager.default.createFile(atPath: file.path, contents: Data(token.utf8), attributes: [.posixPermissions: 0o600])
        try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: file.path)
    }
    static func clear() { try? FileManager.default.removeItem(at: file) }
}

@MainActor final class FeedModel: ObservableObject {
    @Published var posts: [FeedPost] = []
    @Published var filter = "all"
    @Published var nick = ""
    @Published var admin = false
    @Published var hasToken = !FeedToken.load().isEmpty
    @Published var loaded = false
    @Published var offline = false
    @Published var more = false
    @Published var busy = false
    @Published var note = ""
    /// Posts whose whole thread was fetched: their tail holds every reply.
    @Published var opened = Set<Int>()

    /// SOJU_FEED_URL points a development build at a worker on this machine.
    private static let site: String = {
        if let dev = ProcessInfo.processInfo.environment["SOJU_FEED_URL"], dev.hasPrefix("http://127.0.0.1:") || dev.hasPrefix("http://localhost:") { return dev }
        return "https://didimlabs.com"
    }()

    /// Signing out: the site forgets this Mac's token too.
    static func forget(_ token: String) {
        guard !token.isEmpty, let url = URL(string: site + "/api/feed/signout") else { return }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"; request.httpBody = Data("{}".utf8); request.timeoutInterval = 15
        request.setValue("application/json", forHTTPHeaderField: "Content-Type"); request.setValue("Bearer " + token, forHTTPHeaderField: "Authorization")
        URLSession.shared.dataTask(with: request).resume()
    }

    static func imageURL(_ image: FeedImage, full: Bool) -> URL? {
        guard image.k.range(of: "^[A-Za-z0-9_-]{20,64}$", options: .regularExpression) != nil else { return nil }
        return URL(string: site + "/api/feed/image/" + String(image.id) + "?k=" + image.k + "&s=" + (full ? "full" : "thumb"))
    }

    private func send(_ method: String, _ path: String, _ body: [String: Any]? = nil, done: @escaping (Int, [String: Any]) -> Void) {
        guard let url = URL(string: Self.site + "/api/feed" + path) else { return }
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.timeoutInterval = 15
        let token = FeedToken.load()
        if !token.isEmpty { request.setValue("Bearer " + token, forHTTPHeaderField: "Authorization") }
        if let body { request.setValue("application/json", forHTTPHeaderField: "Content-Type"); request.httpBody = try? JSONSerialization.data(withJSONObject: body) }
        URLSession.shared.dataTask(with: request) { data, response, _ in
            let status = (response as? HTTPURLResponse)?.statusCode ?? 0
            let value = (data.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }) ?? [:]
            DispatchQueue.main.async { done(status, value) }
        }.resume()
    }

    private func query(before: Int?) -> String {
        var parts: [String] = []
        if filter == "news" || filter == "player" { parts.append("kind=" + filter) }
        if let before { parts.append("before=" + String(before)) }
        return parts.isEmpty ? "" : "?" + parts.joined(separator: "&")
    }

    func load() {
        hasToken = !FeedToken.load().isEmpty
        guard hasToken else { posts = []; opened = []; nick = ""; admin = false; more = false; loaded = false; return }
        let asked = filter
        send("GET", query(before: nil)) { status, value in
            guard asked == self.filter else { return }
            if status == 401 { self.hasToken = false; self.posts = []; self.loaded = false; return }
            guard status == 200, let list = value["posts"] as? [[String: Any]] else { self.offline = true; self.loaded = true; return }
            var fresh = list.compactMap(FeedPost.init)
            // A thread that was open stays open, with what it showed, unless replies came or went meanwhile.
            var stillOpen = Set<Int>()
            for i in fresh.indices where self.opened.contains(fresh[i].id) {
                if let old = self.posts.first(where: { $0.id == fresh[i].id }), old.tail.count == fresh[i].replies { fresh[i].tail = old.tail; stillOpen.insert(fresh[i].id) }
            }
            withAnimation(.smooth(duration: 0.28)) {
                self.posts = fresh; self.more = (value["more"] as? Bool) ?? false
                self.opened = stillOpen; self.offline = false; self.loaded = true
            }
            let me = value["me"] as? [String: Any]
            self.nick = (me?["nick"] as? String) ?? ""; self.admin = (me?["admin"] as? Bool) ?? false
            // A token the site no longer knows (signed out elsewhere, an old one): posting asks for a sign-in again.
            if me == nil, self.hasToken { self.hasToken = false }
        }
    }

    func loadMore() {
        guard more, let last = posts.last else { return }
        let asked = filter
        send("GET", query(before: last.id)) { status, value in
            guard asked == self.filter, status == 200, let list = value["posts"] as? [[String: Any]] else { return }
            withAnimation(.smooth(duration: 0.28)) { self.posts += list.compactMap(FeedPost.init); self.more = (value["more"] as? Bool) ?? false }
        }
    }

    private func failed(_ status: Int, _ value: [String: Any]) -> Bool {
        if status == 200 { note = ""; return false }
        if status == 401 { hasToken = false }
        note = (value["error"] as? String) ?? "That didn't work. Check the internet connection and try again."
        return true
    }

    func setNick(_ value: String) {
        busy = true
        send("POST", "/nick", ["nick": value]) { status, body in
            self.busy = false
            if !self.failed(status, body) { self.nick = (body["nick"] as? String) ?? "" }
        }
    }

    /// Sends the photos one after another, then the post that carries them.
    func post(body: String, game: String, gameTitle: String, news: Bool, title: String, photos: [FeedPhoto], done: @escaping () -> Void) {
        busy = true
        var sent: [FeedImage] = []
        func next(_ rest: ArraySlice<FeedPhoto>) {
            guard let photo = rest.first else { finish(); return }
            send("POST", "/image", ["full": photo.full.base64EncodedString(), "thumb": photo.thumb.base64EncodedString(), "w": photo.w, "h": photo.h]) { status, value in
                if self.failed(status, value) { self.busy = false; return }
                if let id = value["image"] as? Int { sent.append(FeedImage(id: id, k: (value["k"] as? String) ?? "", w: photo.w, h: photo.h)) }
                next(rest.dropFirst())
            }
        }
        func finish() {
            var payload: [String: Any] = ["body": body, "game": game, "game_title": gameTitle, "images": sent.map(\.id)]
            if news { payload["news"] = true; payload["title"] = title }
            send("POST", "/post", payload) { status, value in
                self.busy = false
                guard !self.failed(status, value) else { return }
                done()
                // Shown at the top at once; the list is not fetched again for it.
                if let id = value["id"] as? Int, self.filter != (news ? "player" : "news") {
                    var mine = FeedPost(id: id, parent: false, kind: news ? "news" : "player", nick: news ? "Didim Labs" : self.nick, title: news ? title : "", body: body, game: game, gameTitle: gameTitle)
                    mine.images = sent
                    withAnimation(.spring(duration: 0.38, bounce: 0.18)) { self.posts.insert(mine, at: 0) }
                } else { self.load() }
            }
        }
        next(photos[...])
    }

    func reply(to post: Int, body: String, done: @escaping () -> Void) {
        busy = true
        send("POST", "/post", ["body": body, "parent": post]) { status, value in
            self.busy = false
            guard !self.failed(status, value) else { return }
            done()
            if let id = value["id"] as? Int, let i = self.posts.firstIndex(where: { $0.id == post }), self.posts[i].tail.count == self.posts[i].replies {
                let mine = FeedPost(id: id, parent: true, kind: "player", nick: self.nick, title: "", body: body, game: "", gameTitle: "")
                withAnimation(.spring(duration: 0.34, bounce: 0.15)) { self.posts[i].tail.append(mine); self.posts[i].replies += 1; self.opened.insert(post) }
            } else { self.open(post) }
        }
    }

    /// Fetches the whole thread of a post and shows it under the post.
    func open(_ post: Int) {
        send("GET", "/thread?id=" + String(post)) { status, value in
            guard status == 200, let list = value["replies"] as? [[String: Any]], let i = self.posts.firstIndex(where: { $0.id == post }) else { return }
            withAnimation(.smooth(duration: 0.3)) { self.posts[i].tail = list.compactMap(FeedPost.init); self.posts[i].replies = self.posts[i].tail.count; self.opened.insert(post) }
        }
    }

    private func setLike(_ id: Int, parent: Int?, likes: Int, liked: Bool) {
        if let parent, let i = posts.firstIndex(where: { $0.id == parent }), let j = posts[i].tail.firstIndex(where: { $0.id == id }) {
            posts[i].tail[j].likes = likes; posts[i].tail[j].liked = liked
        } else if let i = posts.firstIndex(where: { $0.id == id }) {
            posts[i].likes = likes; posts[i].liked = liked
        }
    }

    /// The heart answers at once; if the site says no it goes back to what it was.
    func like(_ post: FeedPost, parent: Int?) {
        let want = !post.liked
        withAnimation(.snappy(duration: 0.2)) { setLike(post.id, parent: parent, likes: max(0, post.likes + (want ? 1 : -1)), liked: want) }
        send("POST", "/like", ["post": post.id, "on": want]) { status, value in
            if self.failed(status, value) { withAnimation(.snappy(duration: 0.2)) { self.setLike(post.id, parent: parent, likes: post.likes, liked: post.liked) }; return }
            let likes = (value["likes"] as? Int) ?? post.likes, liked = (value["liked"] as? Bool) ?? want
            withAnimation(.snappy(duration: 0.2)) { self.setLike(post.id, parent: parent, likes: likes, liked: liked) }
        }
    }

    func report(_ post: FeedPost) {
        send("POST", "/report", ["post": post.id]) { status, value in
            if !self.failed(status, value) { self.note = "Reported. Thank you; we'll take a look." }
        }
    }

    func delete(_ post: FeedPost) {
        send("POST", "/delete", ["post": post.id]) { status, value in
            guard !self.failed(status, value) else { return }
            withAnimation(.smooth(duration: 0.3)) {
                if let i = self.posts.firstIndex(where: { $0.id == post.id }) { self.posts.remove(at: i) }
                else if let i = self.posts.firstIndex(where: { p in p.tail.contains { $0.id == post.id } }) {
                    self.posts[i].tail.removeAll { $0.id == post.id }; self.posts[i].replies = max(0, self.posts[i].replies - 1)
                }
            }
        }
    }
}

/// A game a post can be about: from the library, the store or the GOAT shelf.
struct FeedGameChoice: Identifiable, Hashable {
    let id: String
    let title: String
}

/// A quiet button of the feed (like, reply, add a photo): an icon and maybe a count, with a soft ground under the pointer.
private struct FeedAction: View {
    let symbol: String
    var text: String = ""
    var tint: Color = .secondary
    var bounce: Bool = false
    let action: () -> Void
    @State private var over = false
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        Button(action: action) {
            HStack(spacing: 5) {
                Image(systemName: symbol).symbolEffect(.bounce, value: bounce)
                if !text.isEmpty { Text(text).contentTransition(.numericText()) }
            }.font(.system(size: 12.5, weight: .medium)).monospacedDigit().foregroundStyle(tint)
                .padding(.horizontal, 9).padding(.vertical, 5)
                .background(Color.primary.opacity(over && enabled ? 0.08 : 0), in: Capsule()).contentShape(Capsule())
        }.buttonStyle(.plain).onHover { over = $0 }.animation(.easeOut(duration: 0.12), value: over)
    }
}

/// Google's "G" in its four colours, drawn (the app ships no picture files): a ring in four parts, open at the upper right,
/// with the bar running in from the right.
struct GoogleG: View {
    var body: some View {
        Canvas { ctx, size in
            let s = min(size.width, size.height), c = CGPoint(x: size.width / 2, y: size.height / 2)
            let width = s * 0.21, r = s / 2 - width / 2
            func arc(_ from: Double, _ to: Double, _ colour: Color) {
                var p = Path()
                p.addArc(center: c, radius: r, startAngle: .degrees(from), endAngle: .degrees(to), clockwise: false)
                ctx.stroke(p, with: .color(colour), style: StrokeStyle(lineWidth: width, lineCap: .butt))
            }
            // (Angles run clockwise from three o'clock.)
            arc(205, 316, Color(red: 0.918, green: 0.263, blue: 0.208))   // red, across the top
            arc(134, 206, Color(red: 0.984, green: 0.737, blue: 0.020))   // yellow, down the left
            arc(44, 135, Color(red: 0.204, green: 0.659, blue: 0.325))    // green, along the bottom
            let blue = Color(red: 0.259, green: 0.522, blue: 0.957)
            arc(0, 45, blue)                                              // blue, the lower right
            // The bar, kept inside the ring's outer edge so its corner does not stick out.
            var inside = ctx
            inside.clip(to: Path(ellipseIn: CGRect(x: c.x - s / 2, y: c.y - s / 2, width: s, height: s)))
            inside.fill(Path(CGRect(x: c.x, y: c.y - width / 2, width: s / 2, height: width)), with: .color(blue))
        }.accessibilityHidden(true)
    }
}

/// The two ways in, drawn as the feed's own buttons: a filled one and an outlined one, the same height and shape.
private struct GateButton: ButtonStyle {
    let filled: Bool
    @State private var over = false
    func makeBody(configuration: Configuration) -> some View {
        configuration.label.font(.system(size: 13.5, weight: .semibold))
            .foregroundStyle(filled ? Color(nsColor: .windowBackgroundColor) : Color.primary)
            .padding(.horizontal, 18).frame(height: 38)
            .background(filled ? AnyShapeStyle(Color.primary.opacity(configuration.isPressed ? 0.75 : over ? 0.88 : 1)) : AnyShapeStyle(Color.primary.opacity(configuration.isPressed ? 0.14 : over ? 0.09 : 0)), in: Capsule())
            .overlay { if !filled { Capsule().strokeBorder(Color.primary.opacity(0.28)) } }
            .contentShape(Capsule()).onHover { over = $0 }.animation(.easeOut(duration: 0.12), value: over)
    }
}

struct FeedShelfView: View {
    @ObservedObject var feed: FeedModel
    @ObservedObject var account: AccountModel
    let games: [FeedGameChoice]
    let accent: Color

    @State private var draft = ""
    @State private var game = ""
    @State private var asNews = false
    @State private var newsTitle = ""
    @State private var nickDraft = ""
    @State private var confirmNick = false
    @State private var replyTo: Int?
    @State private var replyDraft = ""
    @State private var deleting: FeedPost?
    @State private var photos: [FeedPhoto] = []
    /// The post whose whole text, photos and thread are shown in a sheet.
    @State private var detail: Int?
    @State private var dropping = false
    @State private var preparing = 0
    @FocusState private var writing: Bool
    private static let ago: RelativeDateTimeFormatter = { let f = RelativeDateTimeFormatter(); f.unitsStyle = .abbreviated; return f }()
    private let limit = 500
    /// The feed reads as one column, as a thread does, however wide the window is.
    private let column: CGFloat = 680

    /// Signed in to Didim Labs, with the feed's token: the feed is for people who are.
    private var signedIn: Bool { !account.email.isEmpty && feed.hasToken }
    private var canWrite: Bool { signedIn && !feed.nick.isEmpty }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                if !signedIn {
                    gate
                } else {
                    if feed.nick.isEmpty && feed.loaded { nickCard } else if !feed.nick.isEmpty { composer }
                    tabs.padding(.top, 20)
                    Divider().opacity(0.6)
                    ForEach(feed.posts) { post in
                        VStack(spacing: 0) { card(post); Divider().opacity(0.45) }
                            .transition(.asymmetric(insertion: .move(edge: .top).combined(with: .opacity), removal: .opacity.combined(with: .scale(scale: 0.97))))
                    }
                    if feed.posts.isEmpty {
                        if feed.loaded || feed.offline {
                            VStack(spacing: 8) {
                                Image(systemName: feed.offline ? "wifi.slash" : "bubble.left.and.bubble.right").font(.system(size: 26, weight: .light)).foregroundStyle(.tertiary)
                                Text(feed.offline ? "The feed is unavailable right now." : "Nothing here yet").font(.system(size: 14, weight: .semibold))
                                Text(feed.offline ? "Check the internet connection." : "Be the first to post about a game.").font(.system(size: 12)).foregroundStyle(.secondary)
                            }.frame(maxWidth: .infinity).padding(.top, 44).transition(.opacity)
                        } else {
                            // The shape of what is coming, so the page does not jump when it arrives.
                            ForEach(0..<3, id: \.self) { _ in placeholder }.transition(.opacity)
                        }
                    }
                    if feed.more { Button("Show older posts") { feed.loadMore() }.buttonStyle(.link).frame(maxWidth: .infinity).padding(.vertical, 16) }
                }
            }.frame(maxWidth: column, alignment: .leading).frame(maxWidth: .infinity, alignment: .leading)
        }
        .animation(.smooth(duration: 0.28), value: feed.note)
        .animation(.smooth(duration: 0.3), value: signedIn)
        .onChange(of: feed.filter) { feed.load() }
        .onChange(of: account.email) { feed.load() }
        .onChange(of: account.signIns) { feed.load() }
        .onAppear { feed.load() }
        .alert("Set your nickname to @" + cleanNick + "?", isPresented: $confirmNick) {
            Button("Set nickname") { feed.setNick(cleanNick) }
            Button("Cancel", role: .cancel) {}
        } message: { Text("It is the name on your posts and can't be changed afterwards. Your email is never shown.") }
        .alert("Delete this post?", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } })) {
            Button("Delete", role: .destructive) { if let post = deleting { feed.delete(post) }; deleting = nil }
            Button("Cancel", role: .cancel) { deleting = nil }
        } message: { Text("Its replies go with it.") }
        .sheet(isPresented: Binding(get: { detail != nil }, set: { if !$0 { detail = nil } })) { detailSheet }
    }

    // MARK: before signing in

    /// Signed out, the feed shows nothing of itself: what it is, the ways in, and the outline of posts fading away below.
    private var gate: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 10) {
                Text("Join the feed").font(.system(size: 26, weight: .bold, design: .rounded))
                Text(account.email.isEmpty ? "Game news from Didim Labs and posts from other players, about the games you play. Sign in to read and to write."
                     : "Sign in once more to open the feed. It was added after you last signed in.")
                    .font(.system(size: 14)).foregroundStyle(.secondary).lineSpacing(2).frame(maxWidth: 460, alignment: .leading).fixedSize(horizontal: false, vertical: true)
                if account.signingIn != nil {
                    HStack(spacing: 10) {
                        ProgressView().controlSize(.small)
                        Text("Finish signing in in your browser.").font(.system(size: 13))
                        Button("Cancel") { account.cancel() }.buttonStyle(GateButton(filled: false))
                    }.padding(.top, 8)
                } else if account.providers.isEmpty {
                    Text("Sign-in is unavailable right now. Check the internet connection.").font(.system(size: 12)).foregroundStyle(.secondary).padding(.top, 8)
                } else {
                    HStack(spacing: 10) {
                        if account.providers.contains("apple") { Button { account.signIn("apple") } label: { Label("Continue with Apple", systemImage: "apple.logo") }.buttonStyle(GateButton(filled: true)) }
                        if account.providers.contains("google") {
                            Button { account.signIn("google") } label: { HStack(spacing: 8) { GoogleG().frame(width: 15, height: 15); Text("Continue with Google") } }.buttonStyle(GateButton(filled: !account.providers.contains("apple")))
                        }
                    }.padding(.top, 8)
                }
                if !account.message.isEmpty && account.signingIn == nil { Text(account.message).font(.system(size: 12)).foregroundStyle(.secondary).padding(.top, 2) }
            }.padding(.top, 26).padding(.bottom, 30)
            // The outline of a feed, going faint: what is behind the door, without showing any of it.
            VStack(spacing: 0) {
                ForEach(0..<4, id: \.self) { i in
                    HStack(alignment: .top, spacing: 12) {
                        Circle().fill(Color.primary.opacity(0.09)).frame(width: 38, height: 38)
                        VStack(alignment: .leading, spacing: 9) {
                            RoundedRectangle(cornerRadius: 4).fill(Color.primary.opacity(0.1)).frame(width: [110, 140, 90, 120][i], height: 11)
                            RoundedRectangle(cornerRadius: 4).fill(Color.primary.opacity(0.07)).frame(height: 11)
                            RoundedRectangle(cornerRadius: 4).fill(Color.primary.opacity(0.07)).frame(width: [300, 220, 360, 260][i], height: 11)
                            if i == 1 { RoundedRectangle(cornerRadius: 12).fill(Color.primary.opacity(0.06)).frame(width: 300, height: 120) }
                        }
                    }.padding(.vertical, 16).frame(maxWidth: .infinity, alignment: .leading)
                    Divider().opacity(0.45)
                }
            }.mask(LinearGradient(colors: [.black, .black.opacity(0.55), .clear], startPoint: .top, endPoint: .bottom)).accessibilityHidden(true)
        }.frame(maxWidth: .infinity, alignment: .leading).transition(.opacity)
    }

    private var nickCard: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Pick a nickname").font(.system(size: 15, weight: .bold, design: .rounded))
            HStack(spacing: 8) {
                TextField("nickname (e.g. nyang_maker)", text: $nickDraft).textFieldStyle(.roundedBorder).frame(maxWidth: 260)
                    .onSubmit { if nickOK { confirmNick = true } }
                Button("Set nickname") { confirmNick = true }.buttonStyle(.borderedProminent).tint(accent).disabled(!nickOK || feed.busy)
            }
            Text("It is the name on your posts. Start with a letter; lower-case letters, digits and _ only, 3 to 20 long. It can't be changed once set. Your email is never shown.")
                .font(.system(size: 11)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            if !feed.note.isEmpty { Text(feed.note).font(.system(size: 11)).foregroundStyle(.orange) }
        }.padding(18).frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.primary.opacity(0.045), in: RoundedRectangle(cornerRadius: 16))
    }

    private var cleanNick: String {
        nickDraft.trimmingCharacters(in: .whitespacesAndNewlines).replacingOccurrences(of: "@", with: "").lowercased()
    }
    private var nickOK: Bool { cleanNick.range(of: "^[a-z][a-z0-9_]{2,19}$", options: .regularExpression) != nil }

    private func avatar(_ name: String, news: Bool, size: CGFloat) -> some View {
        Text(news ? "D" : String(name.prefix(1)).uppercased()).font(.system(size: size * 0.42, weight: .bold, design: .rounded))
            .foregroundStyle(.white).frame(width: size, height: size)
            .background(news ? AnyShapeStyle(LinearGradient(colors: [accent, accent.opacity(0.65)], startPoint: .top, endPoint: .bottom)) : AnyShapeStyle(Self.tint(name)), in: Circle())
    }
    /// One steady colour per nickname.
    private static func tint(_ name: String) -> Color {
        let hues: [Double] = [0.03, 0.42, 0.58, 0.75, 0.12, 0.5, 0.9, 0.33]
        let sum = name.unicodeScalars.reduce(0) { ($0 + Int($1.value)) % 9973 }
        return Color(hue: hues[sum % hues.count], saturation: 0.45, brightness: 0.6)
    }

    private var tabs: some View {
        HStack(spacing: 4) {
            ForEach([("all", "Everything"), ("news", "News"), ("player", "Players")], id: \.0) { id, title in
                Button { feed.filter = id } label: {
                    VStack(spacing: 7) {
                        Text(title).font(.system(size: 13, weight: feed.filter == id ? .semibold : .regular)).foregroundStyle(feed.filter == id ? Color.primary : Color.secondary)
                        Capsule().fill(feed.filter == id ? accent : Color.clear).frame(height: 2)
                    }.padding(.horizontal, 10).fixedSize().contentShape(Rectangle())
                }.buttonStyle(.plain)
            }
            Spacer(minLength: 8)
            if !feed.note.isEmpty && !feed.nick.isEmpty { Text(feed.note).font(.system(size: 11)).foregroundStyle(.orange).lineLimit(2).padding(.bottom, 8) }
        }.animation(.snappy(duration: 0.2), value: feed.filter)
    }

    private var placeholder: some View {
        VStack(spacing: 0) {
            HStack(alignment: .top, spacing: 12) {
                Circle().fill(Color.primary.opacity(0.08)).frame(width: 38, height: 38)
                VStack(alignment: .leading, spacing: 8) {
                    RoundedRectangle(cornerRadius: 4).fill(Color.primary.opacity(0.08)).frame(width: 120, height: 11)
                    RoundedRectangle(cornerRadius: 4).fill(Color.primary.opacity(0.06)).frame(height: 11)
                    RoundedRectangle(cornerRadius: 4).fill(Color.primary.opacity(0.06)).frame(width: 260, height: 11)
                }
            }.padding(.vertical, 16).frame(maxWidth: .infinity, alignment: .leading)
            Divider().opacity(0.45)
        }
    }

    // MARK: writing

    private var composer: some View {
        HStack(alignment: .top, spacing: 12) {
            avatar(feed.nick, news: asNews, size: 38)
            VStack(alignment: .leading, spacing: 8) {
                Text(asNews ? "Didim Labs" : feed.nick).font(.system(size: 13.5, weight: .semibold))
                if asNews { TextField("Title", text: $newsTitle).textFieldStyle(.plain).font(.system(size: 16, weight: .bold, design: .rounded)) }
                ZStack(alignment: .topLeading) {
                    TextEditor(text: Binding(get: { draft }, set: { draft = String($0.prefix(asNews ? 4000 : limit)) }))
                        .font(.system(size: 14)).scrollContentBackground(.hidden).frame(minHeight: writing || !draft.isEmpty ? 64 : 24, maxHeight: 160).focused($writing)
                        .padding(.leading, -5)
                    if draft.isEmpty {
                        Text("What are you playing? Share a tip, a moment, a question.").font(.system(size: 14)).foregroundStyle(.tertiary).allowsHitTesting(false)
                    }
                }
                if !photos.isEmpty {
                    HStack(spacing: 8) {
                        ForEach(photos) { photo in
                            Image(nsImage: photo.preview).resizable().scaledToFill().frame(width: 120, height: 80).clipShape(RoundedRectangle(cornerRadius: 10))
                                .overlay(alignment: .topTrailing) {
                                    Button { withAnimation(.snappy(duration: 0.22)) { photos.removeAll { $0.id == photo.id } } } label: {
                                        Image(systemName: "xmark.circle.fill").font(.system(size: 16)).foregroundStyle(.white, .black.opacity(0.6))
                                    }.buttonStyle(.plain).padding(4).help("Remove this photo")
                                }.transition(.scale(scale: 0.9).combined(with: .opacity))
                        }
                    }
                }
                HStack(spacing: 2) {
                    FeedAction(symbol: "photo", action: addPhotos).disabled(photos.count + preparing >= 2 || feed.busy).help("Add up to 2 photos, or drop them here")
                    Menu {
                        if asNews { Button("No game") { game = "" } }
                        ForEach(games) { choice in Button(choice.title) { game = choice.id } }
                    } label: {
                        Label(games.first { $0.id == game }?.title ?? (asNews ? "No game" : "Tag a game"), systemImage: "gamecontroller").font(.system(size: 12.5, weight: .medium)).lineLimit(1)
                    }.menuStyle(.borderlessButton).fixedSize().foregroundStyle(game.isEmpty ? Color.secondary : accent).padding(.horizontal, 6)
                    if feed.admin {
                        FeedAction(symbol: asNews ? "megaphone.fill" : "megaphone", text: "News", tint: asNews ? accent : .secondary) { withAnimation(.snappy(duration: 0.2)) { asNews.toggle() } }
                            .help("Post as news from Didim Labs")
                    }
                    if preparing > 0 { ProgressView().controlSize(.small).padding(.leading, 6) }
                    Spacer(minLength: 8)
                    if feed.busy { ProgressView().controlSize(.small) }
                    if !draft.isEmpty {
                        Text(String(draft.count) + " / " + String(asNews ? 4000 : limit)).font(.system(size: 11)).monospacedDigit()
                            .foregroundStyle(draft.count > (asNews ? 3900 : limit - 40) ? Color.orange : Color.secondary).padding(.trailing, 6)
                    }
                    Button("Post") { submit() }.buttonStyle(.borderedProminent).buttonBorderShape(.capsule).tint(accent).disabled(!canPost)
                }
            }
        }.padding(16).frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.primary.opacity(dropping ? 0.09 : 0.045), in: RoundedRectangle(cornerRadius: 16))
            .overlay { RoundedRectangle(cornerRadius: 16).strokeBorder(dropping ? accent : Color.primary.opacity(writing ? 0.16 : 0.06), style: StrokeStyle(lineWidth: dropping ? 2 : 1, dash: dropping ? [6, 4] : [])) }
            .overlay(alignment: .bottom) { if dropping { Text("Drop up to 2 photos").font(.system(size: 11, weight: .semibold)).foregroundStyle(accent).padding(.bottom, 6) } }
            .animation(.easeOut(duration: 0.15), value: dropping).animation(.smooth(duration: 0.2), value: writing)
            .onDrop(of: [.fileURL, .image], isTargeted: $dropping) { dropped($0) }
    }

    private var canPost: Bool {
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard text.count >= 2, !feed.busy, preparing == 0 else { return false }
        return asNews ? !newsTitle.trimmingCharacters(in: .whitespaces).isEmpty : !game.isEmpty
    }

    private func submit() {
        let title = games.first { $0.id == game }?.title ?? ""
        feed.post(body: draft.trimmingCharacters(in: .whitespacesAndNewlines), game: game, gameTitle: title, news: asNews && feed.admin,
                  title: newsTitle.trimmingCharacters(in: .whitespaces), photos: photos) { draft = ""; newsTitle = ""; asNews = false; photos = []; writing = false }
    }

    private func addPhotos() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.image]
        panel.allowsMultipleSelection = true
        panel.canChooseDirectories = false
        panel.message = "Photos are made smaller before they are sent (long side 1600 px)."
        guard panel.runModal() == .OK else { return }
        for url in panel.urls { add { FeedPhoto.make(url) } }
    }

    /// Makes a photo small away from the main thread (a large picture takes a moment) and adds it if there is room.
    private func add(_ make: @escaping () -> FeedPhoto?) {
        guard photos.count + preparing < 2 else { return }
        preparing += 1
        DispatchQueue.global(qos: .userInitiated).async {
            let photo = make()
            DispatchQueue.main.async {
                preparing = max(0, preparing - 1)
                if let photo, photos.count < 2 { withAnimation(.snappy(duration: 0.22)) { photos.append(photo) } }
                else if photo == nil { feed.note = "That photo could not be read. Try a JPEG, PNG or HEIC." }
            }
        }
    }

    /// Pictures dropped on the writing box: files from the Finder, or a picture dragged out of another app.
    private func dropped(_ providers: [NSItemProvider]) -> Bool {
        guard canWrite else { return false }
        var taken = false
        for provider in providers.prefix(2) {
            if provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
                taken = true
                _ = provider.loadObject(ofClass: URL.self) { url, _ in
                    guard let url, let type = UTType(filenameExtension: url.pathExtension), type.conforms(to: .image) else { return }
                    DispatchQueue.main.async { add { FeedPhoto.make(url) } }
                }
            } else if provider.hasItemConformingToTypeIdentifier(UTType.image.identifier) {
                taken = true
                _ = provider.loadDataRepresentation(forTypeIdentifier: UTType.image.identifier) { data, _ in
                    guard let data, data.count < 60_000_000 else { return }
                    DispatchQueue.main.async { add { FeedPhoto.make(data) } }
                }
            }
        }
        return taken
    }

    // MARK: reading

    /// One post in full: all of its text, its photos at their real size, every reply, and a place to answer.
    @ViewBuilder private var detailSheet: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Post").font(.system(size: 15, weight: .bold, design: .rounded))
                Spacer()
                Button("Done") { detail = nil }.keyboardShortcut(.cancelAction)
            }.padding(.horizontal, 22).padding(.vertical, 12)
            Divider()
            if let post = feed.posts.first(where: { $0.id == detail }) {
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        entry(post, parent: nil, size: 40, full: true)
                        if !post.tail.isEmpty {
                            Divider().opacity(0.6)
                            ForEach(post.tail) { reply in entry(reply, parent: post.id, size: 28, full: true).transition(.opacity.combined(with: .move(edge: .top))) }
                        }
                    }.padding(22).frame(maxWidth: .infinity, alignment: .leading)
                }
                if canWrite {
                    Divider()
                    HStack(spacing: 8) {
                        TextField("Reply as " + feed.nick, text: Binding(get: { replyDraft }, set: { replyDraft = String($0.prefix(limit)) }))
                            .textFieldStyle(.roundedBorder).onSubmit { sendReply(post.id) }
                        Button("Reply") { sendReply(post.id) }.buttonStyle(.borderedProminent).tint(accent).disabled(replyDraft.trimmingCharacters(in: .whitespacesAndNewlines).count < 2 || feed.busy)
                    }.padding(.horizontal, 22).padding(.vertical, 12)
                }
            } else {
                Text("This post is gone.").foregroundStyle(.secondary).frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }.frame(width: 660, height: 640)
    }

    private func show(_ post: Int) {
        replyTo = nil; replyDraft = ""
        detail = post
        if !feed.opened.contains(post) { feed.open(post) }
    }

    private func card(_ post: FeedPost) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            entry(post, parent: nil, size: 38)
            if !post.tail.isEmpty {
                VStack(alignment: .leading, spacing: 12) {
                    ForEach(post.tail) { reply in entry(reply, parent: post.id, size: 26).transition(.opacity.combined(with: .move(edge: .top))) }
                }.padding(.leading, 18).overlay(alignment: .leading) { Capsule().fill(Color.primary.opacity(0.12)).frame(width: 2).padding(.vertical, 2) }
                    .padding(.leading, 18)
            }
            if post.replies > post.tail.count, !feed.opened.contains(post.id) {
                Button("View all " + String(post.replies) + " replies") { feed.open(post.id) }.buttonStyle(.link).font(.system(size: 12)).padding(.leading, 50).transition(.opacity)
            }
            if replyTo == post.id {
                HStack(spacing: 8) {
                    TextField("Reply as " + feed.nick, text: Binding(get: { replyDraft }, set: { replyDraft = String($0.prefix(limit)) }))
                        .textFieldStyle(.roundedBorder).onSubmit { sendReply(post.id) }
                    Button("Reply") { sendReply(post.id) }.buttonStyle(.borderedProminent).tint(accent).disabled(replyDraft.trimmingCharacters(in: .whitespacesAndNewlines).count < 2 || feed.busy)
                    Button("Cancel") { withAnimation(.snappy(duration: 0.22)) { replyTo = nil }; replyDraft = "" }
                }.padding(.leading, 50).transition(.opacity.combined(with: .move(edge: .top)))
            }
        }.padding(.vertical, 16).frame(maxWidth: .infinity, alignment: .leading)
    }

    private func sendReply(_ post: Int) {
        let text = replyDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard text.count >= 2 else { return }
        feed.reply(to: post, body: text) { replyDraft = ""; withAnimation(.snappy(duration: 0.22)) { replyTo = nil } }
    }

    /// A post's photos: thumbnails in the list (pressed, they open the post), the full pictures in the post's own sheet.
    @ViewBuilder private func pictures(_ post: FeedPost, full: Bool) -> some View {
        if !post.images.isEmpty {
            if full {
                ForEach(post.images) { image in
                    AsyncImage(url: FeedModel.imageURL(image, full: true)) { loaded in loaded.resizable().scaledToFit() } placeholder: { Color.primary.opacity(0.06) }
                        .aspectRatio(image.ratio, contentMode: .fit).frame(maxWidth: .infinity).clipShape(RoundedRectangle(cornerRadius: 12))
                }
            } else {
                HStack(spacing: 8) {
                    ForEach(post.images) { image in
                        AsyncImage(url: FeedModel.imageURL(image, full: false)) { loaded in loaded.resizable().scaledToFill() } placeholder: { Color.primary.opacity(0.06) }
                            .frame(width: post.images.count == 1 ? min(420, 220 * image.ratio) : min(290, 180 * image.ratio), height: post.images.count == 1 ? 220 : 180)
                            .clipShape(RoundedRectangle(cornerRadius: 12)).overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Color.primary.opacity(0.08)))
                    }
                }.padding(.top, 4)
            }
        }
    }

    private func entry(_ post: FeedPost, parent: Int?, size: CGFloat, full: Bool = false) -> some View {
        HStack(alignment: .top, spacing: 12) {
            avatar(post.nick, news: post.isNews, size: size)
            VStack(alignment: .leading, spacing: 5) {
                HStack(spacing: 7) {
                    Text(post.nick).font(.system(size: 13.5, weight: .semibold))
                    if post.isNews {
                        Text("NEWS").font(.system(size: 9.5, weight: .bold)).tracking(0.7).foregroundStyle(accent)
                            .padding(.horizontal, 6).padding(.vertical, 2).background(accent.opacity(0.14), in: Capsule())
                    }
                    Text(Self.ago.localizedString(for: post.at, relativeTo: Date())).font(.system(size: 12)).foregroundStyle(.tertiary)
                    Spacer(minLength: 0)
                    Menu {
                        if post.mine || feed.admin { Button("Delete", role: .destructive) { deleting = post } }
                        if !post.mine { Button(feed.admin ? "Hide" : "Report") { feed.report(post) }.disabled(!canWrite) }
                    } label: { Image(systemName: "ellipsis").frame(width: 22, height: 18) }.menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize().foregroundStyle(.tertiary)
                }
                if full || parent != nil {
                    if !post.title.isEmpty { Text(post.title).font(.system(size: full ? 21 : 17, weight: .bold, design: .rounded)).padding(.top, 1) }
                    Text(FeedText.shown(post.body, links: post.isNews)).font(.system(size: 14)).lineSpacing(3).fixedSize(horizontal: false, vertical: true).textSelection(.enabled)
                    pictures(post, full: true)
                } else {
                    // The list shows the start of a post; pressing it opens the whole of it.
                    Button { show(post.id) } label: {
                        VStack(alignment: .leading, spacing: 5) {
                            if !post.title.isEmpty { Text(post.title).font(.system(size: 17, weight: .bold, design: .rounded)).padding(.top, 1) }
                            Text(FeedText.shown(post.body, links: false)).font(.system(size: 14)).lineSpacing(3).lineLimit(post.isNews ? 4 : 6).multilineTextAlignment(.leading).fixedSize(horizontal: false, vertical: true)
                            pictures(post, full: false)
                        }.frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
                    }.buttonStyle(.plain).help("Open this post")
                }
                if !post.gameTitle.isEmpty {
                    Label(post.gameTitle, systemImage: "gamecontroller.fill").font(.system(size: 11.5, weight: .medium)).lineLimit(1).foregroundStyle(.secondary)
                        .padding(.horizontal, 9).padding(.vertical, 4).background(Color.primary.opacity(0.06), in: Capsule()).padding(.top, 3)
                }
                HStack(spacing: 2) {
                    FeedAction(symbol: post.liked ? "heart.fill" : "heart", text: String(post.likes), tint: post.liked ? .red : .secondary, bounce: post.liked) { feed.like(post, parent: parent) }
                        .disabled(!canWrite).help(canWrite ? "Like" : "Pick a nickname to like posts")
                    if parent == nil {
                        FeedAction(symbol: replyTo == post.id ? "bubble.left.fill" : "bubble.left", text: String(post.replies)) {
                            if full { return }
                            withAnimation(.snappy(duration: 0.22)) { replyTo = replyTo == post.id ? nil : post.id }; replyDraft = ""
                        }.disabled(!canWrite).help(canWrite ? "Reply" : "Pick a nickname to reply")
                    }
                }.padding(.leading, -9).padding(.top, 1)
            }
        }
    }
}
