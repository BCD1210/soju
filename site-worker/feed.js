// The Didim Labs feed in the Soju app (app/Feed.swift): game news from Didim Labs and what signed-in players post, one stream.
// Kept in the FEED D1 database. Served at didimlabs.com/api/feed (worker.js).
//
//   GET  /api/feed?before=<id>&kind=news|player   newest first, 30 a page; with a token, also what this account liked and wrote
//   GET  /api/feed/thread?id=<id>                 one post and its replies
//   POST /api/feed/nick   {nick}                  the name shown on posts: one per account, taken once
//   POST /api/feed/post   {body, game, game_title, parent?, title?, news?}   a post about one game, or a reply
//   POST /api/feed/image  {full, thumb, w, h}     one photo for a post about to be made (base64 JPEG, made small by Soju): {image}
//   GET  /api/feed/image/<id>?s=thumb|full        a photo of a post that is shown
//   POST /api/feed/like   {post, on}
//   POST /api/feed/report {post}                  three accounts reporting a post hide it until an operator looks
//   POST /api/feed/delete {post}                  one's own post (an operator: any post)
// Posting needs "Authorization: Bearer <token>": the token Soju gets once when a sign-in finishes (auth.js, issueToken).
// Only the nickname is ever shown; the address stays in the database. Operators are the addresses in FEED_ADMINS
// (comma separated): their posts may be news (shown as Didim Labs), and they may hide any post.
//
// Read: wrangler d1 execute soju-feed --remote --command "select id, nick, kind, body from posts order by id desc limit 50" -c wrangler.didimlabs.toml

const json = (body, status = 200) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json", "cache-control": "no-store" } });
// Photos: Soju sends each one already made small (long side 1600, JPEG) with a 560 px thumbnail, so nothing here resizes and
// nothing is paid for: they sit in the same database as the posts (a D1 row holds 2 MB), and are served to be kept by caches.
export const FEED = { BODY_MAX: 500, NEWS_MAX: 4000, BODY_MIN: 2, TITLE_MAX: 100, PAGE: 30, THREAD: 200, PER_DAY: 30, GAP_MS: 10000, REPORTS_HIDE: 3, TAIL: 2,
  PHOTOS: 2, FULL_MAX: 450 * 1024, THUMB_MAX: 90 * 1024, PHOTOS_PER_DAY: 30 };
export const NICK = /^[a-z][a-z0-9_]{2,19}$/;
const RESERVED = new Set(["didim", "didimlabs", "didim_labs", "soju", "goat", "admin", "administrator", "operator", "official", "support", "staff", "team", "news", "system", "moderator", "mod", "null", "undefined", "anonymous", "everyone"]);
const GAME = /^(steam|battlenet|epic|gog|ea|rockstar|ubisoft|xbox|didim|goat):[A-Za-z0-9_.-]{1,80}$/;
const SCHEMA = [
  "CREATE TABLE IF NOT EXISTS accounts (email TEXT PRIMARY KEY, nick TEXT UNIQUE, created INTEGER NOT NULL)",
  "CREATE TABLE IF NOT EXISTS tokens (hash TEXT PRIMARY KEY, email TEXT NOT NULL, created INTEGER NOT NULL)",
  "CREATE INDEX IF NOT EXISTS tokens_email ON tokens (email, created)",
  "CREATE TABLE IF NOT EXISTS posts (id INTEGER PRIMARY KEY AUTOINCREMENT, parent INTEGER, email TEXT NOT NULL, nick TEXT NOT NULL, kind TEXT NOT NULL, title TEXT NOT NULL DEFAULT '', body TEXT NOT NULL, game TEXT NOT NULL DEFAULT '', game_title TEXT NOT NULL DEFAULT '', created INTEGER NOT NULL, hidden INTEGER NOT NULL DEFAULT 0)",
  "CREATE INDEX IF NOT EXISTS posts_top ON posts (parent, hidden, id)",
  "CREATE INDEX IF NOT EXISTS posts_email ON posts (email, created)",
  "CREATE TABLE IF NOT EXISTS images (id INTEGER PRIMARY KEY AUTOINCREMENT, post INTEGER, email TEXT NOT NULL, full BLOB NOT NULL, thumb BLOB NOT NULL, w INTEGER NOT NULL, h INTEGER NOT NULL, created INTEGER NOT NULL)",
  "CREATE INDEX IF NOT EXISTS images_post ON images (post)",
  "CREATE INDEX IF NOT EXISTS images_email ON images (email, created)",
  "CREATE TABLE IF NOT EXISTS likes (post INTEGER NOT NULL, email TEXT NOT NULL, PRIMARY KEY (post, email))",
  "CREATE TABLE IF NOT EXISTS reports (post INTEGER NOT NULL, email TEXT NOT NULL, created INTEGER NOT NULL, PRIMARY KEY (post, email))",
];
let ready = null, minute = 0, perAddress = new Map();
export function feedReset() { ready = null; minute = 0; perAddress = new Map(); }  // tests

const text = (v, n) => String(v ?? "").replace(/[\u0000-\u0008\u000b-\u001f\u007f​-‏‪-‮⁦-⁩]/g, "").trim().slice(0, n);
const oneLine = (v, n) => text(v, n * 2).replace(/\s+/g, " ").trim().slice(0, n);
const sha = async (s) => [...new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(s)))].map((b) => b.toString(16).padStart(2, "0")).join("");
const admins = (env) => new Set(String(env.FEED_ADMINS || "").split(",").map((x) => x.trim().toLowerCase()).filter(Boolean));

async function schema(db) {
  if (ready !== db) { await db.batch(SCHEMA.map((s) => db.prepare(s))); ready = db; }
}

// Called by auth.js when a sign-in finishes: the one time the token is given out. Null when the feed has no database.
export async function issueToken(env, email) {
  if (!env.FEED) return null;
  try {
    await schema(env.FEED);
    const raw = btoa(String.fromCharCode(...crypto.getRandomValues(new Uint8Array(32)))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
    const now = Date.now();
    await env.FEED.batch([
      env.FEED.prepare("INSERT INTO accounts (email, created) VALUES (?, ?) ON CONFLICT (email) DO NOTHING").bind(email, now),
      env.FEED.prepare("INSERT INTO tokens (hash, email, created) VALUES (?, ?, ?)").bind(await sha(raw), email, now),
      // A Mac signs in again now and then: an account keeps its ten newest tokens.
      env.FEED.prepare("DELETE FROM tokens WHERE email = ?1 AND hash NOT IN (SELECT hash FROM tokens WHERE email = ?1 ORDER BY created DESC LIMIT 10)").bind(email),
    ]);
    return raw;
  } catch { return null; }
}

async function who(env, request) {
  const m = /^Bearer ([A-Za-z0-9_-]{40,64})$/.exec(request.headers.get("authorization") || "");
  if (!m) return null;
  const row = await env.FEED.prepare("SELECT a.email, a.nick FROM tokens t JOIN accounts a ON a.email = t.email WHERE t.hash = ?").bind(await sha(m[1])).first();
  return row ? { email: row.email, nick: row.nick || "", admin: admins(env).has(row.email) } : null;
}

// A JPEG and nothing else: its first bytes and its last, whatever the sender calls it.
const jpeg = (b64, max) => {
  if (typeof b64 !== "string" || b64.length > Math.ceil(max * 4 / 3) + 8 || !/^[A-Za-z0-9+/]+={0,2}$/.test(b64)) return null;
  let raw; try { raw = atob(b64); } catch { return null; }
  if (raw.length < 200 || raw.length > max) return null;
  const bytes = Uint8Array.from(raw, (c) => c.charCodeAt(0));
  return bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff && bytes[bytes.length - 2] === 0xff && bytes[bytes.length - 1] === 0xd9 ? bytes : null;
};
const photosOf = async (db, ids) => {
  const by = new Map();
  if (!ids.length) return by;
  const rows = (await db.prepare(`SELECT id, post, w, h FROM images WHERE post IN (${ids.map((_, i) => "?" + (i + 1)).join(",")}) ORDER BY id`).bind(...ids).all()).results;
  for (const r of rows) { if (!by.has(r.post)) by.set(r.post, []); by.get(r.post).push({ id: r.id, w: r.w, h: r.h }); }
  return by;
};

const shape = (r, me) => ({ id: r.id, kind: r.kind, nick: r.kind === "news" ? "Didim Labs" : r.nick, title: r.title || "", body: r.body, game: r.game || "", game_title: r.game_title || "",
  at: r.created, images: [], likes: r.likes || 0, liked: !!r.liked, replies: r.replies || 0, mine: !!(me && r.email === me.email) });
const COLS = (mail) => `p.id, p.parent, p.email, p.nick, p.kind, p.title, p.body, p.game, p.game_title, p.created,
  (SELECT COUNT(*) FROM likes l WHERE l.post = p.id) AS likes,
  (SELECT COUNT(*) FROM posts c WHERE c.parent = p.id AND c.hidden = 0) AS replies,
  ${mail ? "(SELECT 1 FROM likes l WHERE l.post = p.id AND l.email = ?1)" : "0"} AS liked`;

export async function feed(request, env, url) {
  if (!env.FEED) return json({ error: "Not available right now." }, 503);
  const m = Math.floor(Date.now() / 60000);
  if (m !== minute) { minute = m; perAddress = new Map(); }
  const address = request.headers.get("cf-connecting-ip") || "";
  const sent = (perAddress.get(address) || 0) + 1;
  perAddress.set(address, sent);
  if (sent > 120) return json({ error: "Busy, try later." }, 429);
  const db = env.FEED, path = url.pathname.replace(/^\/api\/feed\/?/, "");
  await schema(db);
  // For trying the feed on one's own machine (wrangler dev with FEED_DEV_EMAIL in .dev.vars): a token without a sign-in.
  // The variable is never set on the deployed worker.
  if (path === "dev-token" && env.FEED_DEV_EMAIL) return json({ token: await issueToken(env, String(env.FEED_DEV_EMAIL).toLowerCase()), email: String(env.FEED_DEV_EMAIL).toLowerCase() });
  const me = await who(env, request);

  if (request.method === "GET" && path === "") {
    const before = Math.floor(Number(url.searchParams.get("before"))) || 0, kind = url.searchParams.get("kind");
    const where = ["p.parent IS NULL", "p.hidden = 0"], args = [me ? me.email : ""];
    if (before > 0) { where.push("p.id < ?" + (args.length + 1)); args.push(before); }
    if (kind === "news" || kind === "player") { where.push("p.kind = ?" + (args.length + 1)); args.push(kind); }
    const rows = (await db.prepare(`SELECT ${COLS(true)} FROM posts p WHERE ${where.join(" AND ")} ORDER BY p.id DESC LIMIT ${FEED.PAGE + 1}`).bind(...args).all()).results;
    const page = rows.slice(0, FEED.PAGE), posts = page.map((r) => ({ ...shape(r, me), tail: [] }));
    // The first replies of each post shown under it, as a thread does.
    const withReplies = posts.filter((p) => p.replies > 0).map((p) => p.id);
    if (withReplies.length) {
      const marks = withReplies.map((_, i) => "?" + (i + 2)).join(",");
      const tails = (await db.prepare(`SELECT ${COLS(true)} FROM posts p WHERE p.parent IN (${marks}) AND p.hidden = 0 ORDER BY p.id`).bind(me ? me.email : "", ...withReplies).all()).results;
      for (const t of tails) { const p = posts.find((x) => x.id === t.parent); if (p && p.tail.length < FEED.TAIL) p.tail.push(shape(t, me)); }
    }
    const photos = await photosOf(db, posts.map((p) => p.id));
    for (const p of posts) p.images = photos.get(p.id) || [];
    return json({ posts, more: rows.length > FEED.PAGE, me: me ? { nick: me.nick, admin: me.admin } : null });
  }
  if (request.method === "GET" && path === "thread") {
    const id = Math.floor(Number(url.searchParams.get("id"))) || 0;
    const post = await db.prepare(`SELECT ${COLS(true)} FROM posts p WHERE p.id = ?2 AND p.parent IS NULL AND p.hidden = 0`).bind(me ? me.email : "", id).first();
    if (!post) return json({ error: "That post is gone." }, 404);
    const replies = (await db.prepare(`SELECT ${COLS(true)} FROM posts p WHERE p.parent = ?2 AND p.hidden = 0 ORDER BY p.id LIMIT ${FEED.THREAD}`).bind(me ? me.email : "", id).all()).results;
    const photos = await photosOf(db, [id]);
    return json({ post: { ...shape(post, me), images: photos.get(id) || [] }, replies: replies.map((r) => shape(r, me)), me: me ? { nick: me.nick, admin: me.admin } : null });
  }
  const pic = request.method === "GET" ? /^image\/(\d{1,12})$/.exec(path) : null;
  if (pic) {
    // Only a photo of a post that is shown; one waiting for its post, or of a hidden post, is nobody's to see.
    const col = url.searchParams.get("s") === "full" ? "full" : "thumb";
    const row = await db.prepare(`SELECT i.${col} AS bytes FROM images i JOIN posts p ON p.id = i.post WHERE i.id = ? AND p.hidden = 0`).bind(Number(pic[1])).first();
    if (!row || !row.bytes) return new Response("not found", { status: 404, headers: { "cache-control": "no-store" } });
    return new Response(row.bytes instanceof ArrayBuffer ? row.bytes : new Uint8Array(row.bytes), { headers: { "content-type": "image/jpeg", "cache-control": "public, max-age=31536000, immutable",
      "x-content-type-options": "nosniff", "content-security-policy": "default-src 'none'; sandbox" } });
  }
  if (request.method !== "POST") return json({ error: "Not found." }, 404);
  if (!me) return json({ error: "Sign in to do that.", signin: true }, 401);
  if (Number(request.headers.get("content-length") || 0) > (path === "image" ? 800 : 16) * 1024) return json({ error: "Too large." }, 413);
  let input;
  try { input = await request.json(); } catch { return json({ error: "Send JSON." }, 400); }
  const now = Date.now();

  if (path === "nick") {
    const nick = String(input?.nick ?? "").trim().replace(/^@/, "").toLowerCase();
    if (!NICK.test(nick)) return json({ error: "A nickname starts with a letter and has 3 to 20 lower-case letters, digits or _." }, 400);
    if (RESERVED.has(nick)) return json({ error: "That nickname can't be used. Pick another." }, 400);
    if (me.nick) return me.nick === nick ? json({ nick }) : json({ error: "Your nickname is @" + me.nick + " and can't be changed." }, 409);
    const r = await db.prepare("UPDATE accounts SET nick = ?1 WHERE email = ?2 AND nick IS NULL AND NOT EXISTS (SELECT 1 FROM accounts WHERE nick = ?1)").bind(nick, me.email).run();
    if (r.meta.changes) return json({ nick });
    const mine = await db.prepare("SELECT nick FROM accounts WHERE email = ?").bind(me.email).first();
    return mine && mine.nick === nick ? json({ nick }) : json({ error: mine && mine.nick ? "Your nickname is @" + mine.nick + " and can't be changed." : "Someone already has that nickname." }, 409);
  }
  if (path === "image") {
    if (!me.nick) return json({ error: "Pick a nickname first.", nick: true }, 400);
    const full = jpeg(input?.full, FEED.FULL_MAX), thumb = jpeg(input?.thumb, FEED.THUMB_MAX);
    const w = Math.floor(Number(input?.w)), h = Math.floor(Number(input?.h));
    if (!full || !thumb || !(w >= 16 && w <= 4096 && h >= 16 && h <= 4096)) return json({ error: "That photo could not be used. Try another one." }, 400);
    if (!me.admin) {
      const day = await db.prepare("SELECT COUNT(*) AS n FROM images WHERE email = ? AND created > ?").bind(me.email, now - 864e5).first();
      if (day && day.n >= FEED.PHOTOS_PER_DAY) return json({ error: "That's enough photos for today. Come back tomorrow." }, 429);
    }
    // Photos whose post never came are dropped after a day.
    if (Math.random() < 0.05) await db.prepare("DELETE FROM images WHERE post IS NULL AND created < ?").bind(now - 864e5).run();
    const row = await db.prepare("INSERT INTO images (email, full, thumb, w, h, created) VALUES (?, ?, ?, ?, ?, ?) RETURNING id").bind(me.email, full, thumb, w, h, now).first();
    return json({ image: row.id });
  }
  if (path === "post") {
    if (!me.nick) return json({ error: "Pick a nickname first.", nick: true }, 400);
    const longOK = input?.news === true && me.admin && input?.parent == null, bodyMax = longOK ? FEED.NEWS_MAX : FEED.BODY_MAX;
    const body = text(input?.body, bodyMax + 50).replace(/\n{3,}/g, "\n\n");
    if (body.length < FEED.BODY_MIN || body.length > bodyMax) return json({ error: `A post has ${FEED.BODY_MIN} to ${bodyMax} characters.` }, 400);
    // Its photos: sent just before, by this account, not on a post yet.
    const images = [...new Set((Array.isArray(input?.images) ? input.images : []).map((x) => Math.floor(Number(x))).filter((x) => x > 0))];
    if (images.length > FEED.PHOTOS) return json({ error: `A post has up to ${FEED.PHOTOS} photos.` }, 400);
    if (images.length) {
      const mine = await db.prepare(`SELECT COUNT(*) AS n FROM images WHERE email = ?1 AND post IS NULL AND id IN (${images.map((_, i) => "?" + (i + 2)).join(",")})`).bind(me.email, ...images).first();
      if (!mine || mine.n !== images.length) return json({ error: "A photo did not arrive. Add it again." }, 400);
    }
    const parentId = input?.parent == null ? null : Math.floor(Number(input.parent)) || 0;
    let game = "", gameTitle = "", kind = "player", title = "";
    if (parentId !== null) {
      const parent = await db.prepare("SELECT id FROM posts WHERE id = ? AND parent IS NULL AND hidden = 0").bind(parentId).first();
      if (!parent) return json({ error: "That post is gone." }, 404);
    } else {
      game = String(input?.game ?? ""); gameTitle = oneLine(input?.game_title, 80);
      const news = input?.news === true && me.admin;
      // A post is about a game: that is what keeps the feed on games. News from Didim Labs may be about none.
      if (!news && (!GAME.test(game) || !gameTitle)) return json({ error: "Tag the game this post is about." }, 400);
      if (game && !GAME.test(game)) return json({ error: "Tag the game this post is about." }, 400);
      if (news) { kind = "news"; title = oneLine(input?.title, FEED.TITLE_MAX); if (!title) return json({ error: "News needs a title." }, 400); }
    }
    if (!me.admin) {
      const day = await db.prepare("SELECT COUNT(*) AS n, MAX(created) AS last FROM posts WHERE email = ? AND created > ?").bind(me.email, now - 864e5).first();
      if (day && day.last && now - day.last < FEED.GAP_MS) return json({ error: "Wait a few seconds before posting again." }, 429);
      if (day && day.n >= FEED.PER_DAY) return json({ error: "That's enough posts for today. Come back tomorrow." }, 429);
    }
    const row = await db.prepare("INSERT INTO posts (parent, email, nick, kind, title, body, game, game_title, created) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id")
      .bind(parentId, me.email, me.nick, kind, title, body, game, gameTitle, now).first();
    if (images.length) await db.prepare(`UPDATE images SET post = ?1 WHERE email = ?2 AND post IS NULL AND id IN (${images.map((_, i) => "?" + (i + 3)).join(",")})`).bind(row.id, me.email, ...images).run();
    return json({ id: row.id });
  }
  const postId = Math.floor(Number(input?.post)) || 0;
  const post = postId ? await db.prepare("SELECT id, email, hidden FROM posts WHERE id = ?").bind(postId).first() : null;
  if (!post || (post.hidden && !me.admin)) return json({ error: "That post is gone." }, 404);
  if (path === "like") {
    if (input?.on === false) await db.prepare("DELETE FROM likes WHERE post = ? AND email = ?").bind(postId, me.email).run();
    else await db.prepare("INSERT INTO likes (post, email) VALUES (?, ?) ON CONFLICT DO NOTHING").bind(postId, me.email).run();
    const n = await db.prepare("SELECT COUNT(*) AS n FROM likes WHERE post = ?").bind(postId).first();
    return json({ likes: n.n, liked: input?.on !== false });
  }
  if (path === "report") {
    if (post.email === me.email) return json({ ok: true });
    await db.prepare("INSERT INTO reports (post, email, created) VALUES (?, ?, ?) ON CONFLICT DO NOTHING").bind(postId, me.email, now).run();
    const n = await db.prepare("SELECT COUNT(*) AS n FROM reports WHERE post = ?").bind(postId).first();
    if (n.n >= FEED.REPORTS_HIDE || me.admin) await db.prepare("UPDATE posts SET hidden = 1 WHERE id = ?").bind(postId).run();
    return json({ ok: true });
  }
  if (path === "delete") {
    if (post.email !== me.email && !me.admin) return json({ error: "That isn't your post." }, 403);
    // Its replies go with it; so do the likes and reports on all of them.
    await db.batch([
      db.prepare("DELETE FROM images WHERE post = ?1 OR post IN (SELECT id FROM posts WHERE parent = ?1)").bind(postId),
      db.prepare("DELETE FROM likes WHERE post = ?1 OR post IN (SELECT id FROM posts WHERE parent = ?1)").bind(postId),
      db.prepare("DELETE FROM reports WHERE post = ?1 OR post IN (SELECT id FROM posts WHERE parent = ?1)").bind(postId),
      db.prepare("DELETE FROM posts WHERE id = ?1 OR parent = ?1").bind(postId),
    ]);
    return json({ ok: true });
  }
  return json({ error: "Not found." }, 404);
}
