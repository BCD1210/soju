import SwiftUI
import AppKit

/// The Didim Labs feed, the first shelf of the Didim Labs page: game news from Didim Labs and what signed-in players post,
/// one stream, with replies under a post as in a thread. Served by didimlabs.com/api/feed (site-worker/feed.js).
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
    var isNews: Bool { kind == "news" }

    init?(_ d: [String: Any]) {
        guard let id = d["id"] as? Int, let body = d["body"] as? String, let nick = d["nick"] as? String else { return nil }
        self.id = id; self.body = body; self.nick = nick
        kind = (d["kind"] as? String) ?? "player"; title = (d["title"] as? String) ?? ""
        game = (d["game"] as? String) ?? ""; gameTitle = (d["game_title"] as? String) ?? ""
        at = Date(timeIntervalSince1970: ((d["at"] as? Double) ?? 0) / 1000)
        likes = (d["likes"] as? Int) ?? 0; liked = (d["liked"] as? Bool) ?? false; replies = (d["replies"] as? Int) ?? 0
        mine = (d["mine"] as? Bool) ?? false
        tail = ((d["tail"] as? [[String: Any]]) ?? []).compactMap(FeedPost.init)
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
            self.posts = list.compactMap(FeedPost.init); self.more = (value["more"] as? Bool) ?? false
            self.opened = []; self.offline = false; self.loaded = true
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
            self.posts += list.compactMap(FeedPost.init); self.more = (value["more"] as? Bool) ?? false
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

    func post(body: String, game: String, gameTitle: String, news: Bool, title: String, done: @escaping () -> Void) {
        busy = true
        var payload: [String: Any] = ["body": body, "game": game, "game_title": gameTitle]
        if news { payload["news"] = true; payload["title"] = title }
        send("POST", "/post", payload) { status, value in
            self.busy = false
            if !self.failed(status, value) { done(); self.load() }
        }
    }

    func reply(to post: Int, body: String, done: @escaping () -> Void) {
        busy = true
        send("POST", "/post", ["body": body, "parent": post]) { status, value in
            self.busy = false
            if !self.failed(status, value) { done(); self.open(post) }
        }
    }

    /// Fetches the whole thread of a post and shows it under the post.
    func open(_ post: Int) {
        send("GET", "/thread?id=" + String(post)) { status, value in
            guard status == 200, let list = value["replies"] as? [[String: Any]], let i = self.posts.firstIndex(where: { $0.id == post }) else { return }
            self.posts[i].tail = list.compactMap(FeedPost.init); self.posts[i].replies = self.posts[i].tail.count; self.opened.insert(post)
        }
    }

    func like(_ post: FeedPost, parent: Int?) {
        send("POST", "/like", ["post": post.id, "on": !post.liked]) { status, value in
            guard !self.failed(status, value) else { return }
            let likes = (value["likes"] as? Int) ?? post.likes, liked = (value["liked"] as? Bool) ?? !post.liked
            if let parent, let i = self.posts.firstIndex(where: { $0.id == parent }), let j = self.posts[i].tail.firstIndex(where: { $0.id == post.id }) {
                self.posts[i].tail[j].likes = likes; self.posts[i].tail[j].liked = liked
            } else if let i = self.posts.firstIndex(where: { $0.id == post.id }) {
                self.posts[i].likes = likes; self.posts[i].liked = liked
            }
        }
    }

    func report(_ post: FeedPost) {
        send("POST", "/report", ["post": post.id]) { status, value in
            if !self.failed(status, value) { self.note = "Reported. Thank you; we'll take a look." }
        }
    }

    func delete(_ post: FeedPost) {
        send("POST", "/delete", ["post": post.id]) { status, value in
            if !self.failed(status, value) { self.load() }
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
                    if !feed.note.isEmpty { Text(feed.note).font(.system(size: 11)).foregroundStyle(.orange).lineLimit(2) }
                    Spacer(minLength: 0)
                }
                ForEach(feed.posts) { card($0) }
                if feed.posts.isEmpty {
                    Text(feed.offline ? "The feed is unavailable right now. Check the internet connection."
                         : feed.loaded ? "Nothing here yet. Be the first to post about a game." : "Loading the feed…")
                        .foregroundStyle(.secondary).padding(.top, 30).frame(maxWidth: .infinity)
                }
                if feed.more { Button("Show older posts") { feed.loadMore() }.frame(maxWidth: .infinity) }
            }.frame(maxWidth: .infinity, alignment: .leading)
        }
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

    // MARK: writing

    @ViewBuilder private var composer: some View {
        VStack(alignment: .leading, spacing: 9) {
            if account.email.isEmpty || !feed.hasToken {
                Text(account.email.isEmpty ? "Sign in to post about the games you play." : "Sign in once more to post: posting was added after you signed in.")
                    .font(.system(size: 12)).foregroundStyle(.secondary)
                HStack(spacing: 10) {
                    if account.signingIn != nil {
                        ProgressView().controlSize(.small)
                        Text("Finish signing in in your browser.").font(.system(size: 12))
                        Button("Cancel") { account.cancel() }
                    } else {
                        if account.providers.contains("apple") { Button { account.signIn("apple") } label: { Label("Sign in with Apple", systemImage: "apple.logo") } }
                        if account.providers.contains("google") { Button { account.signIn("google") } label: { Label("Sign in with Google", systemImage: "g.circle") } }
                    }
                }
            } else if feed.nick.isEmpty {
                Text("Pick a nickname").font(.system(size: 13, weight: .semibold))
                HStack(spacing: 8) {
                    TextField("nickname (e.g. nyang_maker)", text: $nickDraft).textFieldStyle(.roundedBorder).frame(maxWidth: 260)
                        .onSubmit { if nickOK { confirmNick = true } }
                    Button("Set nickname") { confirmNick = true }.disabled(!nickOK || feed.busy)
                }
                Text("It is the name on your posts. Start with a letter; lower-case letters, digits and _ only, 3 to 20 long. It can't be changed once set. Your email is never shown.")
                    .font(.system(size: 11)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            } else {
                HStack(alignment: .top, spacing: 12) {
                    avatar(feed.nick, news: asNews, size: 34)
                    VStack(alignment: .leading, spacing: 8) {
                        Text(asNews ? "Didim Labs · news" : feed.nick).font(.system(size: 13, weight: .semibold))
                        if asNews { TextField("Title", text: $newsTitle).textFieldStyle(.roundedBorder) }
                        ZStack(alignment: .topLeading) {
                            TextEditor(text: Binding(get: { draft }, set: { draft = String($0.prefix(limit)) }))
                                .font(.system(size: 13)).scrollContentBackground(.hidden).frame(minHeight: 44, maxHeight: 110)
                            if draft.isEmpty {
                                Text("What are you playing? Share a tip, a moment, a question.").font(.system(size: 13)).foregroundStyle(.tertiary)
                                    .padding(.leading, 5).allowsHitTesting(false)
                            }
                        }
                        HStack(spacing: 10) {
                            Picker("Game", selection: $game) {
                                Text(asNews ? "No game" : "Tag a game").tag("")
                                ForEach(games) { Text($0.title).tag($0.id) }
                            }.labelsHidden().frame(maxWidth: 260)
                            if feed.admin { Toggle("Post as news", isOn: $asNews).toggleStyle(.checkbox).font(.system(size: 12)) }
                            Spacer(minLength: 0)
                            Text(String(draft.count) + " / " + String(limit)).font(.system(size: 11)).monospacedDigit().foregroundStyle(.secondary)
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
                  title: newsTitle.trimmingCharacters(in: .whitespaces)) { draft = ""; newsTitle = ""; asNews = false }
    }

    // MARK: reading

    private func card(_ post: FeedPost) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            entry(post, parent: nil, size: 34)
            if !post.tail.isEmpty {
                VStack(alignment: .leading, spacing: 10) {
                    ForEach(post.tail) { entry($0, parent: post.id, size: 24) }
                }.padding(.leading, 16).overlay(alignment: .leading) { Rectangle().fill(Color.primary.opacity(0.14)).frame(width: 2).padding(.vertical, 2) }
                    .padding(.leading, 16)
            }
            if post.replies > post.tail.count, !feed.opened.contains(post.id) {
                Button("View all " + String(post.replies) + " replies") { feed.open(post.id) }.buttonStyle(.link).font(.system(size: 12)).padding(.leading, 46)
            }
            if replyTo == post.id {
                HStack(spacing: 8) {
                    TextField("Reply as " + feed.nick, text: Binding(get: { replyDraft }, set: { replyDraft = String($0.prefix(limit)) }))
                        .textFieldStyle(.roundedBorder).onSubmit { sendReply(post.id) }
                    Button("Reply") { sendReply(post.id) }.disabled(replyDraft.trimmingCharacters(in: .whitespacesAndNewlines).count < 2 || feed.busy)
                    Button("Cancel") { replyTo = nil; replyDraft = "" }
                }.padding(.leading, 46)
            }
        }.padding(16).frame(maxWidth: .infinity, alignment: .leading)
            .background(post.isNews ? accent.opacity(0.1) : Color.primary.opacity(0.04), in: RoundedRectangle(cornerRadius: 14))
    }

    private func sendReply(_ post: Int) {
        let text = replyDraft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard text.count >= 2 else { return }
        feed.reply(to: post, body: text) { replyDraft = ""; replyTo = nil }
    }

    private var canWrite: Bool { !account.email.isEmpty && feed.hasToken && !feed.nick.isEmpty }

    private func entry(_ post: FeedPost, parent: Int?, size: CGFloat) -> some View {
        HStack(alignment: .top, spacing: 12) {
            avatar(post.nick, news: post.isNews, size: size)
            VStack(alignment: .leading, spacing: 5) {
                HStack(spacing: 7) {
                    Text(post.nick).font(.system(size: 13, weight: .semibold))
                    if post.isNews { Text("NEWS").font(.system(size: 10, weight: .bold)).tracking(0.8).foregroundStyle(accent) }
                    Text(Self.ago.localizedString(for: post.at, relativeTo: Date())).font(.system(size: 11)).foregroundStyle(.secondary)
                    Spacer(minLength: 0)
                    Menu {
                        if post.mine || feed.admin { Button("Delete", role: .destructive) { deleting = post } }
                        if !post.mine { Button(feed.admin ? "Hide" : "Report") { feed.report(post) }.disabled(!canWrite) }
                    } label: { Image(systemName: "ellipsis") }.menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize().foregroundStyle(.secondary)
                }
                if !post.title.isEmpty { Text(post.title).font(.system(size: 17, weight: .bold, design: .rounded)) }
                Text(post.body).font(.system(size: 13)).fixedSize(horizontal: false, vertical: true).textSelection(.enabled)
                if !post.gameTitle.isEmpty {
                    Label(post.gameTitle, systemImage: "gamecontroller.fill").font(.system(size: 11)).lineLimit(1)
                        .padding(.horizontal, 9).padding(.vertical, 3).background(Color.primary.opacity(0.07), in: Capsule())
                }
                HStack(spacing: 16) {
                    Button { feed.like(post, parent: parent) } label: {
                        Label(String(post.likes), systemImage: post.liked ? "heart.fill" : "heart").foregroundStyle(post.liked ? Color.red : Color.secondary)
                    }.buttonStyle(.plain).disabled(!canWrite).help(canWrite ? "Like" : "Sign in and pick a nickname to like posts")
                    if parent == nil {
                        Button { replyTo = replyTo == post.id ? nil : post.id; replyDraft = "" } label: {
                            Label(String(post.replies), systemImage: "bubble.left").foregroundStyle(.secondary)
                        }.buttonStyle(.plain).disabled(!canWrite).help(canWrite ? "Reply" : "Sign in and pick a nickname to reply")
                    }
                }.font(.system(size: 12)).monospacedDigit().padding(.top, 2)
            }
        }
    }
}
