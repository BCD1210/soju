import SwiftUI
import AppKit
import ImageIO

/// The Didim Labs feed, the first shelf of the Didim Labs page: game news from Didim Labs and what signed-in players post,
/// one stream, with replies under a post as in a thread. Served by didimlabs.com/api/feed (site-worker/feed.js).
/// A photo on a post: its id on the site and its size, so its place is kept before it loads.
struct FeedImage: Identifiable, Hashable {
    let id: Int
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

    static func make(_ url: URL) -> FeedPhoto? {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
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
            return FeedImage(id: id, w: (i["w"] as? Int) ?? 3, h: (i["h"] as? Int) ?? 2)
        }
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

    static func imageURL(_ image: FeedImage, full: Bool) -> URL? {
        URL(string: site + "/api/feed/image/" + String(image.id) + "?s=" + (full ? "full" : "thumb"))
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
        let asked = filter
        send("GET", query(before: nil)) { status, value in
            guard asked == self.filter else { return }
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
                if let id = value["image"] as? Int { sent.append(FeedImage(id: id, w: photo.w, h: photo.h)) }
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
    private static let ago: RelativeDateTimeFormatter = { let f = RelativeDateTimeFormatter(); f.unitsStyle = .abbreviated; return f }()
    private let limit = 500

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                composer
                HStack(spacing: 10) {
                    Picker("Show", selection: $feed.filter) {
                        Text("Everything").tag("all"); Text("News").tag("news"); Text("Players").tag("player")
                    }.pickerStyle(.segmented).labelsHidden().fixedSize()
                    if !feed.note.isEmpty { Text(feed.note).font(.system(size: 12)).foregroundStyle(.orange).lineLimit(2) }
                    Spacer(minLength: 0)
                }
                ForEach(feed.posts) { post in
                    card(post).transition(.asymmetric(insertion: .move(edge: .top).combined(with: .opacity), removal: .opacity.combined(with: .scale(scale: 0.97))))
                }
                if feed.posts.isEmpty {
                    if feed.loaded || feed.offline {
                        Text(feed.offline ? "The feed is unavailable right now. Check the internet connection." : "Nothing here yet. Be the first to post about a game.")
                            .foregroundStyle(.secondary).padding(.top, 30).frame(maxWidth: .infinity).transition(.opacity)
                    } else {
                        // The shape of what is coming, so the page does not jump when it arrives.
                        ForEach(0..<3, id: \.self) { _ in placeholder }.transition(.opacity)
                    }
                }
                if feed.more { Button("Show older posts") { feed.loadMore() }.frame(maxWidth: .infinity) }
            }.frame(maxWidth: .infinity, alignment: .leading)
        }
        .animation(.smooth(duration: 0.28), value: feed.note)
        .onChange(of: feed.filter) { feed.load() }
        .onChange(of: account.email) { feed.load() }
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

    /// One post in full: all of its text, its photos at their real size, every reply, and a place to answer.
    @ViewBuilder private var detailSheet: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Post").font(.system(size: 16, weight: .bold, design: .rounded))
                Spacer()
                Button("Done") { detail = nil }.keyboardShortcut(.cancelAction)
            }.padding(.horizontal, 20).padding(.vertical, 12)
            Divider()
            if let post = feed.posts.first(where: { $0.id == detail }) {
                ScrollView {
                    VStack(alignment: .leading, spacing: 14) {
                        entry(post, parent: nil, size: 38, full: true)
                        if !post.tail.isEmpty {
                            Divider()
                            ForEach(post.tail) { reply in entry(reply, parent: post.id, size: 26, full: true).transition(.opacity.combined(with: .move(edge: .top))) }
                        }
                    }.padding(20).frame(maxWidth: .infinity, alignment: .leading)
                }
                if canWrite {
                    Divider()
                    HStack(spacing: 8) {
                        TextField("Reply as " + feed.nick, text: Binding(get: { replyDraft }, set: { replyDraft = String($0.prefix(limit)) }))
                            .textFieldStyle(.roundedBorder).onSubmit { sendReply(post.id) }
                        Button("Reply") { sendReply(post.id) }.disabled(replyDraft.trimmingCharacters(in: .whitespacesAndNewlines).count < 2 || feed.busy)
                    }.padding(.horizontal, 20).padding(.vertical, 12)
                }
            } else {
                Text("This post is gone.").foregroundStyle(.secondary).frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }.frame(width: 640, height: 620)
    }

    private func show(_ post: Int) {
        replyTo = nil; replyDraft = ""
        detail = post
        if !feed.opened.contains(post) { feed.open(post) }
    }

    private func addPhotos() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.image]
        panel.allowsMultipleSelection = true
        panel.canChooseDirectories = false
        panel.message = "Photos are made smaller before they are sent (long side 1600 px)."
        guard panel.runModal() == .OK else { return }
        for url in panel.urls.prefix(max(0, 2 - photos.count)) {
            if let photo = FeedPhoto.make(url) { withAnimation(.snappy(duration: 0.22)) { photos.append(photo) } }
            else { feed.note = "That photo could not be read. Try a JPEG, PNG or HEIC." }
        }
    }

    private var cleanNick: String {
        nickDraft.trimmingCharacters(in: .whitespacesAndNewlines).replacingOccurrences(of: "@", with: "").lowercased()
    }
    private var nickOK: Bool { cleanNick.range(of: "^[a-z][a-z0-9_]{2,19}$", options: .regularExpression) != nil }

    private func avatar(_ name: String, news: Bool, size: CGFloat) -> some View {
        Text(news ? "D" : String(name.prefix(1)).uppercased()).font(.system(size: size * 0.42, weight: .bold, design: news ? .rounded : .default))
            .foregroundStyle(news ? Color(red: 0.88, green: 0.7, blue: 0.35) : .white)
            .frame(width: size, height: size)
            .background(news ? Color.black.opacity(0.55) : Self.tint(name), in: Circle())
    }
    /// One steady colour per nickname.
    private static func tint(_ name: String) -> Color {
        let hues: [Double] = [0.03, 0.42, 0.58, 0.75, 0.12, 0.5, 0.9, 0.33]
        let sum = name.unicodeScalars.reduce(0) { ($0 + Int($1.value)) % 9973 }
        return Color(hue: hues[sum % hues.count], saturation: 0.5, brightness: 0.62)
    }

    private var placeholder: some View {
        HStack(alignment: .top, spacing: 12) {
            Circle().fill(Color.primary.opacity(0.08)).frame(width: 34, height: 34)
            VStack(alignment: .leading, spacing: 8) {
                RoundedRectangle(cornerRadius: 4).fill(Color.primary.opacity(0.08)).frame(width: 120, height: 11)
                RoundedRectangle(cornerRadius: 4).fill(Color.primary.opacity(0.06)).frame(height: 11)
                RoundedRectangle(cornerRadius: 4).fill(Color.primary.opacity(0.06)).frame(width: 260, height: 11)
            }
        }.padding(16).frame(maxWidth: .infinity, alignment: .leading).background(Color.primary.opacity(0.04), in: RoundedRectangle(cornerRadius: 14))
    }

    // MARK: writing

    @ViewBuilder private var composer: some View {
        VStack(alignment: .leading, spacing: 9) {
            if account.email.isEmpty || !feed.hasToken {
                Text(account.email.isEmpty ? "Sign in to post about the games you play." : "Sign in once more to post: posting was added after you signed in.")
                    .font(.system(size: 13)).foregroundStyle(.secondary)
                HStack(spacing: 10) {
                    if account.signingIn != nil {
                        ProgressView().controlSize(.small)
                        Text("Finish signing in in your browser.").font(.system(size: 13))
                        Button("Cancel") { account.cancel() }
                    } else {
                        if account.providers.contains("apple") { Button { account.signIn("apple") } label: { Label("Sign in with Apple", systemImage: "apple.logo") } }
                        if account.providers.contains("google") { Button { account.signIn("google") } label: { Label("Sign in with Google", systemImage: "g.circle") } }
                    }
                }
            } else if feed.nick.isEmpty {
                Text("Pick a nickname").font(.system(size: 14, weight: .semibold))
                HStack(spacing: 8) {
                    TextField("nickname (e.g. nyang_maker)", text: $nickDraft).textFieldStyle(.roundedBorder).frame(maxWidth: 260)
                        .onSubmit { if nickOK { confirmNick = true } }
                    Button("Set nickname") { confirmNick = true }.disabled(!nickOK || feed.busy)
                }
                Text("It is the name on your posts. Start with a letter; lower-case letters, digits and _ only, 3 to 20 long. It can't be changed once set. Your email is never shown.")
                    .font(.system(size: 12)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            } else {
                HStack(alignment: .top, spacing: 12) {
                    avatar(feed.nick, news: asNews, size: 34)
                    VStack(alignment: .leading, spacing: 8) {
                        Text(asNews ? "Didim Labs · news" : feed.nick).font(.system(size: 14, weight: .semibold))
                        if asNews { TextField("Title", text: $newsTitle).textFieldStyle(.roundedBorder) }
                        ZStack(alignment: .topLeading) {
                            TextEditor(text: Binding(get: { draft }, set: { draft = String($0.prefix(asNews ? 4000 : limit)) }))
                                .font(.system(size: 14)).scrollContentBackground(.hidden).frame(minHeight: 44, maxHeight: 110)
                            if draft.isEmpty {
                                Text("What are you playing? Share a tip, a moment, a question.").font(.system(size: 14)).foregroundStyle(.tertiary)
                                    .padding(.leading, 5).allowsHitTesting(false)
                            }
                        }
                        if !photos.isEmpty {
                            HStack(spacing: 8) {
                                ForEach(photos) { photo in
                                    Image(nsImage: photo.preview).resizable().scaledToFill().frame(width: 96, height: 64).clipShape(RoundedRectangle(cornerRadius: 8))
                                        .overlay(alignment: .topTrailing) {
                                            Button { withAnimation(.snappy(duration: 0.22)) { photos.removeAll { $0.id == photo.id } } } label: {
                                                Image(systemName: "xmark.circle.fill").font(.system(size: 16)).foregroundStyle(.white, .black.opacity(0.6))
                                            }.buttonStyle(.plain).padding(3).help("Remove this photo")
                                        }.transition(.scale(scale: 0.9).combined(with: .opacity))
                                }
                            }
                        }
                        HStack(spacing: 10) {
                            Button { addPhotos() } label: { Image(systemName: "photo") }.disabled(photos.count >= 2 || feed.busy).help("Add up to 2 photos")
                            Picker("Game", selection: $game) {
                                Text(asNews ? "No game" : "Tag a game").tag("")
                                ForEach(games) { Text($0.title).tag($0.id) }
                            }.labelsHidden().frame(maxWidth: 230)
                            if feed.admin { Toggle("Post as news", isOn: $asNews).toggleStyle(.checkbox).font(.system(size: 13)) }
                            Spacer(minLength: 0)
                            if feed.busy { ProgressView().controlSize(.small) }
                            Text(String(draft.count) + " / " + String(asNews ? 4000 : limit)).font(.system(size: 12)).monospacedDigit().foregroundStyle(.secondary)
                            Button("Post") { submit() }.buttonStyle(.borderedProminent).tint(accent).disabled(!canPost)
                        }
                    }
                }
            }
        }.padding(16).frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.primary.opacity(0.04), in: RoundedRectangle(cornerRadius: 14))
    }

    private var canPost: Bool {
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard text.count >= 2, !feed.busy else { return false }
        return asNews ? !newsTitle.trimmingCharacters(in: .whitespaces).isEmpty : !game.isEmpty
    }

    private func submit() {
        let title = games.first { $0.id == game }?.title ?? ""
        feed.post(body: draft.trimmingCharacters(in: .whitespacesAndNewlines), game: game, gameTitle: title, news: asNews && feed.admin,
                  title: newsTitle.trimmingCharacters(in: .whitespaces), photos: photos) { draft = ""; newsTitle = ""; asNews = false; photos = [] }
    }

    // MARK: reading

    private func card(_ post: FeedPost) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            entry(post, parent: nil, size: 34)
            if !post.tail.isEmpty {
                VStack(alignment: .leading, spacing: 10) {
                    ForEach(post.tail) { reply in entry(reply, parent: post.id, size: 24).transition(.opacity.combined(with: .move(edge: .top))) }
                }.padding(.leading, 16).overlay(alignment: .leading) { Rectangle().fill(Color.primary.opacity(0.14)).frame(width: 2).padding(.vertical, 2) }
                    .padding(.leading, 16)
            }
            if post.replies > post.tail.count, !feed.opened.contains(post.id) {
                Button("View all " + String(post.replies) + " replies") { feed.open(post.id) }.buttonStyle(.link).font(.system(size: 13)).padding(.leading, 46).transition(.opacity)
            }
            if replyTo == post.id {
                HStack(spacing: 8) {
                    TextField("Reply as " + feed.nick, text: Binding(get: { replyDraft }, set: { replyDraft = String($0.prefix(limit)) }))
                        .textFieldStyle(.roundedBorder).onSubmit { sendReply(post.id) }
                    Button("Reply") { sendReply(post.id) }.disabled(replyDraft.trimmingCharacters(in: .whitespacesAndNewlines).count < 2 || feed.busy)
                    Button("Cancel") { withAnimation(.snappy(duration: 0.22)) { replyTo = nil }; replyDraft = "" }
                }.padding(.leading, 46).transition(.opacity.combined(with: .move(edge: .top)))
            }
        }.padding(16).frame(maxWidth: .infinity, alignment: .leading)
            .background(post.isNews ? accent.opacity(0.1) : Color.primary.opacity(0.04), in: RoundedRectangle(cornerRadius: 14))
    }

    private func sendReply(_ post: Int) {
        let text = replyDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard text.count >= 2 else { return }
        feed.reply(to: post, body: text) { replyDraft = ""; withAnimation(.snappy(duration: 0.22)) { replyTo = nil } }
    }

    private var canWrite: Bool { !account.email.isEmpty && feed.hasToken && !feed.nick.isEmpty }

    /// A post's photos: thumbnails in the list (pressed, they open the post), the full pictures in the post's own sheet.
    @ViewBuilder private func pictures(_ post: FeedPost, full: Bool) -> some View {
        if !post.images.isEmpty {
            if full {
                ForEach(post.images) { image in
                    AsyncImage(url: FeedModel.imageURL(image, full: true)) { loaded in loaded.resizable().scaledToFit() } placeholder: { Color.primary.opacity(0.06) }
                        .aspectRatio(image.ratio, contentMode: .fit).frame(maxWidth: .infinity).clipShape(RoundedRectangle(cornerRadius: 10))
                }
            } else {
                HStack(spacing: 8) {
                    ForEach(post.images) { image in
                        AsyncImage(url: FeedModel.imageURL(image, full: false)) { loaded in loaded.resizable().scaledToFill() } placeholder: { Color.primary.opacity(0.06) }
                            .frame(width: min(300, 150 * image.ratio), height: 150).clipShape(RoundedRectangle(cornerRadius: 10))
                    }
                }
            }
        }
    }

    private func entry(_ post: FeedPost, parent: Int?, size: CGFloat, full: Bool = false) -> some View {
        HStack(alignment: .top, spacing: 12) {
            avatar(post.nick, news: post.isNews, size: size)
            VStack(alignment: .leading, spacing: 5) {
                HStack(spacing: 7) {
                    Text(post.nick).font(.system(size: 14, weight: .semibold))
                    if post.isNews { Text("NEWS").font(.system(size: 11, weight: .bold)).tracking(0.8).foregroundStyle(accent) }
                    Text(Self.ago.localizedString(for: post.at, relativeTo: Date())).font(.system(size: 12)).foregroundStyle(.secondary)
                    Spacer(minLength: 0)
                    Menu {
                        if post.mine || feed.admin { Button("Delete", role: .destructive) { deleting = post } }
                        if !post.mine { Button(feed.admin ? "Hide" : "Report") { feed.report(post) }.disabled(!canWrite) }
                    } label: { Image(systemName: "ellipsis") }.menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize().foregroundStyle(.secondary)
                }
                if full || parent != nil {
                    if !post.title.isEmpty { Text(post.title).font(.system(size: full ? 22 : 18, weight: .bold, design: .rounded)) }
                    Text(post.body).font(.system(size: 14)).fixedSize(horizontal: false, vertical: true).textSelection(.enabled)
                    pictures(post, full: true)
                } else {
                    // The list shows the start of a post; pressing it opens the whole of it.
                    Button { show(post.id) } label: {
                        VStack(alignment: .leading, spacing: 5) {
                            if !post.title.isEmpty { Text(post.title).font(.system(size: 18, weight: .bold, design: .rounded)) }
                            Text(post.body).font(.system(size: 14)).lineLimit(post.isNews ? 4 : 6).multilineTextAlignment(.leading).fixedSize(horizontal: false, vertical: true)
                            pictures(post, full: false)
                        }.frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
                    }.buttonStyle(.plain).help("Open this post")
                }
                if !post.gameTitle.isEmpty {
                    Label(post.gameTitle, systemImage: "gamecontroller.fill").font(.system(size: 12)).lineLimit(1)
                        .padding(.horizontal, 9).padding(.vertical, 3).background(Color.primary.opacity(0.07), in: Capsule())
                }
                HStack(spacing: 16) {
                    Button { feed.like(post, parent: parent) } label: {
                        HStack(spacing: 5) {
                            Image(systemName: post.liked ? "heart.fill" : "heart").symbolEffect(.bounce, value: post.liked)
                            Text(String(post.likes)).contentTransition(.numericText(value: Double(post.likes)))
                        }.foregroundStyle(post.liked ? Color.red : Color.secondary).contentShape(Rectangle())
                    }.buttonStyle(.plain).disabled(!canWrite).help(canWrite ? "Like" : "Sign in and pick a nickname to like posts")
                    if parent == nil {
                        Button { withAnimation(.snappy(duration: 0.22)) { replyTo = replyTo == post.id ? nil : post.id }; replyDraft = "" } label: {
                            HStack(spacing: 5) {
                                Image(systemName: replyTo == post.id ? "bubble.left.fill" : "bubble.left")
                                Text(String(post.replies)).contentTransition(.numericText(value: Double(post.replies)))
                            }.foregroundStyle(.secondary).contentShape(Rectangle())
                        }.buttonStyle(.plain).disabled(!canWrite).help(canWrite ? "Reply" : "Sign in and pick a nickname to reply")
                    }
                }.font(.system(size: 13)).monospacedDigit().padding(.top, 2)
            }
        }
    }
}
