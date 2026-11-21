// The Didim Labs feed (site-worker/feed.js) against an in-memory SQLite standing in for D1.
import test from "node:test";
import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";
import { feed, issueToken, feedReset, FEED, jpegSize } from "../site-worker/feed.js";

function d1() {
  const db = new DatabaseSync(":memory:");
  const val = (v) => v === undefined ? null : typeof v === "boolean" ? (v ? 1 : 0) : v;
  const prepare = (sql) => { const make = (a) => ({ sql, bind: (...more) => make(more.map(val)),
    run: async () => ({ meta: { changes: Number(db.prepare(sql).run(...a).changes) } }),
    first: async () => db.prepare(sql).get(...a) || null, all: async () => ({ results: db.prepare(sql).all(...a) }) }); return make([]); };
  return { raw: db, prepare, batch: async (list) => { const out = []; for (const s of list) out.push(/^\s*SELECT/i.test(s.sql) ? await s.all() : await s.run()); return out; } };
}
const setup = async () => { feedReset(); const env = { FEED: d1(), FEED_ADMINS: "op@didimlabs.com" };
  const tok = {}; for (const e of ["a@x.com", "b@x.com", "c@x.com", "d@x.com", "op@didimlabs.com"]) tok[e] = await issueToken(env, e); return { env, tok }; };
const call = async (env, method, path, token, body) => { const url = new URL("https://didimlabs.com/api/feed" + path);
  const sent = body ? JSON.stringify(body) : undefined;
  const r = await feed(new Request(url, { method, headers: { "content-type": "application/json", "cf-connecting-ip": "10.0.0.1", ...(sent ? { "content-length": String(Buffer.byteLength(sent)) } : {}), ...(token ? { authorization: "Bearer " + token } : {}) }, body: sent }), env, url);
  return { status: r.status, ...(await r.json()) }; };
const GAME = { game: "didim:dont-sell-the-cat", game_title: "Don't Sell the Cat" };
const later = (env, ms) => env.FEED.raw.exec(`UPDATE posts SET created = created - ${ms}`);

test("no database: the feed says so and sign-in still works without a token", async () => {
  feedReset();
  assert.equal(await issueToken({}, "a@x.com"), null);
  const url = new URL("https://didimlabs.com/api/feed");
  assert.equal((await feed(new Request(url), {}, url)).status, 503);
});

test("a nickname is needed to post, has one shape, is taken once and never by two people", async () => {
  const { env, tok } = await setup();
  assert.equal((await call(env, "POST", "/post", null, { body: "hello there", ...GAME })).status, 401);
  assert.equal((await call(env, "POST", "/post", "x".repeat(43), { body: "hello there", ...GAME })).status, 401);
  const noNick = await call(env, "POST", "/post", tok["a@x.com"], { body: "hello there", ...GAME });
  assert.equal(noNick.status, 400); assert.equal(noNick.nick, true);
  for (const bad of ["ab", "1abc", "Has Space", "a".repeat(21), "한글", "didimlabs", "admin"]) assert.equal((await call(env, "POST", "/nick", tok["a@x.com"], { nick: bad })).status, 400, bad);
  assert.deepEqual(await call(env, "POST", "/nick", tok["a@x.com"], { nick: "@Nyang_Maker" }), { status: 200, nick: "nyang_maker" });
  assert.equal((await call(env, "POST", "/nick", tok["a@x.com"], { nick: "nyang_maker" })).status, 200);
  assert.equal((await call(env, "POST", "/nick", tok["a@x.com"], { nick: "other_name" })).status, 409);
  assert.equal((await call(env, "POST", "/nick", tok["b@x.com"], { nick: "NYANG_MAKER" })).status, 409);
  assert.equal((await call(env, "GET", "", tok["a@x.com"])).me.nick, "nyang_maker");
  assert.equal((await call(env, "GET", "", null)).me, null);
});

test("a post is about one game, has a length, and shows the nickname and never the address", async () => {
  const { env, tok } = await setup();
  await call(env, "POST", "/nick", tok["a@x.com"], { nick: "nyang_maker" });
  assert.equal((await call(env, "POST", "/post", tok["a@x.com"], { body: "no game on this one" })).status, 400);
  assert.equal((await call(env, "POST", "/post", tok["a@x.com"], { body: "bad game id", game: "https://evil.example.com", game_title: "x" })).status, 400);
  assert.equal((await call(env, "POST", "/post", tok["a@x.com"], { body: "x", ...GAME })).status, 400);
  assert.equal((await call(env, "POST", "/post", tok["a@x.com"], { body: "y".repeat(FEED.BODY_MAX + 1), ...GAME })).status, 400);
  // A member cannot post news, whatever they send.
  const p = await call(env, "POST", "/post", tok["a@x.com"], { body: "Night four: shine the UV lamp on the ledger.\u0000‮", news: true, title: "Fake news", ...GAME });
  assert.equal(p.status, 200);
  const list = await call(env, "GET", "", null);
  assert.equal(list.posts.length, 1);
  assert.deepEqual({ ...list.posts[0], at: 0 }, { id: p.id, kind: "player", nick: "nyang_maker", title: "", body: "Night four: shine the UV lamp on the ledger.", game: GAME.game, game_title: GAME.game_title, at: 0, images: [], likes: 0, liked: false, replies: 0, mine: false, tail: [] });
  assert.ok(!JSON.stringify(list).includes("a@x.com"));
  assert.equal((await call(env, "GET", "", tok["a@x.com"])).posts[0].mine, true);
});

test("news comes from operators, shows as Didim Labs, and the stream can be filtered", async () => {
  const { env, tok } = await setup();
  await call(env, "POST", "/nick", tok["a@x.com"], { nick: "nyang_maker" }); await call(env, "POST", "/nick", tok["op@didimlabs.com"], { nick: "mike_92" });
  await call(env, "POST", "/post", tok["a@x.com"], { body: "Anyone for Sven Co-op tonight?", game: "steam:225840", game_title: "Sven Co-op" });
  assert.equal((await call(env, "POST", "/post", tok["op@didimlabs.com"], { body: "The demo is out.", news: true })).status, 400);   // news has a title
  const n = await call(env, "POST", "/post", tok["op@didimlabs.com"], { body: "The free demo ends after the first night.", news: true, title: "Stakeholders demo 0.2", game: "didim:stakeholders", game_title: "Stakeholders" });
  assert.equal(n.status, 200);
  assert.equal((await call(env, "POST", "/post", tok["op@didimlabs.com"], { body: "Soju 1.15 is out.", news: true, title: "Soju 1.15" })).status, 200);   // news about no game
  const all = await call(env, "GET", "", null);
  assert.deepEqual(all.posts.map((p) => p.kind), ["news", "news", "player"]);
  assert.equal(all.posts[1].nick, "Didim Labs"); assert.equal(all.posts[1].title, "Stakeholders demo 0.2");
  assert.deepEqual((await call(env, "GET", "?kind=news", null)).posts.map((p) => p.kind), ["news", "news"]);
  assert.deepEqual((await call(env, "GET", "?kind=player", null)).posts.map((p) => p.nick), ["nyang_maker"]);
  assert.equal((await call(env, "GET", "", tok["op@didimlabs.com"])).me.admin, true);
});

test("replies thread under a post, likes count once per account", async () => {
  const { env, tok } = await setup();
  for (const [e, n] of [["a@x.com", "aaa_one"], ["b@x.com", "bbb_two"], ["c@x.com", "ccc_three"]]) await call(env, "POST", "/nick", tok[e], { nick: n });
  const p = await call(env, "POST", "/post", tok["a@x.com"], { body: "Is ending seven real?", ...GAME });
  assert.equal((await call(env, "POST", "/post", tok["b@x.com"], { body: "Reply to nothing", parent: 9999 })).status, 404);
  const r1 = await call(env, "POST", "/post", tok["b@x.com"], { body: "Yes. Watch what the cat looks at.", parent: p.id });
  await call(env, "POST", "/post", tok["c@x.com"], { body: "No spoilers please.", parent: p.id });
  later(env, 60000);
  await call(env, "POST", "/post", tok["b@x.com"], { body: "Sorry!", parent: p.id });
  assert.equal((await call(env, "POST", "/post", tok["c@x.com"], { body: "A reply to a reply is not a thing", parent: r1.id })).status, 404);
  const list = await call(env, "GET", "", tok["b@x.com"]);
  assert.equal(list.posts.length, 1); assert.equal(list.posts[0].replies, 3);
  assert.deepEqual(list.posts[0].tail.map((t) => t.nick), ["bbb_two", "ccc_three"]); assert.equal(list.posts[0].tail[0].mine, true);
  const th = await call(env, "GET", "/thread?id=" + p.id, null);
  assert.deepEqual(th.replies.map((t) => t.body), ["Yes. Watch what the cat looks at.", "No spoilers please.", "Sorry!"]);
  assert.equal((await call(env, "GET", "/thread?id=" + r1.id, null)).status, 404);
  assert.deepEqual(await call(env, "POST", "/like", tok["b@x.com"], { post: p.id, on: true }), { status: 200, likes: 1, liked: true });
  assert.equal((await call(env, "POST", "/like", tok["b@x.com"], { post: p.id, on: true })).likes, 1);
  assert.equal((await call(env, "POST", "/like", tok["c@x.com"], { post: p.id })).likes, 2);
  assert.equal((await call(env, "GET", "", tok["b@x.com"])).posts[0].liked, true);
  assert.equal((await call(env, "GET", "", tok["a@x.com"])).posts[0].liked, false);
  assert.deepEqual(await call(env, "POST", "/like", tok["b@x.com"], { post: p.id, on: false }), { status: 200, likes: 1, liked: false });
  assert.equal((await call(env, "POST", "/like", null, { post: p.id })).status, 401);
});

test("deleting is for one's own post (or an operator) and takes the replies; three reports hide a post", async () => {
  const { env, tok } = await setup();
  for (const [e, n] of [["a@x.com", "aaa_one"], ["b@x.com", "bbb_two"], ["c@x.com", "ccc_three"], ["d@x.com", "ddd_four"], ["op@didimlabs.com", "mike_92"]]) await call(env, "POST", "/nick", tok[e], { nick: n });
  const p = await call(env, "POST", "/post", tok["a@x.com"], { body: "Mine to delete", ...GAME });
  await call(env, "POST", "/post", tok["b@x.com"], { body: "A reply", parent: p.id }); await call(env, "POST", "/like", tok["b@x.com"], { post: p.id });
  assert.equal((await call(env, "POST", "/delete", tok["b@x.com"], { post: p.id })).status, 403);
  assert.equal((await call(env, "POST", "/delete", tok["a@x.com"], { post: p.id })).status, 200);
  assert.equal((await call(env, "GET", "", null)).posts.length, 0);
  assert.equal(env.FEED.raw.prepare("SELECT COUNT(*) AS n FROM posts").get().n, 0); assert.equal(env.FEED.raw.prepare("SELECT COUNT(*) AS n FROM likes").get().n, 0);
  later(env, 60000);
  const q = await call(env, "POST", "/post", tok["a@x.com"], { body: "Something three people report", ...GAME });
  await call(env, "POST", "/report", tok["a@x.com"], { post: q.id });   // one's own report counts for nothing
  await call(env, "POST", "/report", tok["b@x.com"], { post: q.id }); await call(env, "POST", "/report", tok["b@x.com"], { post: q.id }); await call(env, "POST", "/report", tok["c@x.com"], { post: q.id });
  assert.equal((await call(env, "GET", "", null)).posts.length, 1);
  await call(env, "POST", "/report", tok["d@x.com"], { post: q.id });
  assert.equal((await call(env, "GET", "", null)).posts.length, 0);
  assert.equal((await call(env, "POST", "/like", tok["b@x.com"], { post: q.id })).status, 404);
  // An operator takes a post down with one report, and may delete anyone's.
  later(env, 60000);
  const r = await call(env, "POST", "/post", tok["b@x.com"], { body: "An operator hides this", ...GAME });
  await call(env, "POST", "/report", tok["op@didimlabs.com"], { post: r.id });
  assert.equal((await call(env, "GET", "", null)).posts.length, 0);
  assert.equal((await call(env, "POST", "/delete", tok["op@didimlabs.com"], { post: r.id })).status, 200);
});

test("photos: small JPEGs sent before the post, shown only with a post that is shown, gone with it", async () => {
  const { env, tok } = await setup();
  for (const [e, n] of [["a@x.com", "aaa_one"], ["b@x.com", "bbb_two"], ["op@didimlabs.com", "mike_92"]]) await call(env, "POST", "/nick", tok[e], { nick: n });
  // A JPEG as far as its frame header (which says how big the picture is), then filler.
  const jpg = (n, w, h) => Buffer.concat([Buffer.from([0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 0x08, h >> 8, h & 255, w >> 8, w & 255, 0x03]), Buffer.alloc(n - 14, 7), Buffer.from([0xff, 0xd9])]).toString("base64");
  const pic = { full: jpg(5006, 1600, 900), thumb: jpg(906, 560, 315), w: 1600, h: 900 };
  assert.deepEqual(jpegSize(Buffer.from(pic.full, "base64")), { w: 1600, h: 900 });
  assert.equal((await call(env, "POST", "/image", null, pic)).status, 401);
  assert.equal((await call(env, "POST", "/image", tok["a@x.com"], { ...pic, full: Buffer.from("<svg onload=alert(1)>".repeat(40)).toString("base64") })).status, 400);   // not a JPEG
  assert.equal((await call(env, "POST", "/image", tok["a@x.com"], { ...pic, full: jpg(FEED.FULL_MAX + 10, 1600, 900) })).status, 400);
  assert.equal((await call(env, "POST", "/image", tok["a@x.com"], { ...pic, w: 99999 })).status, 400);
  // A small file that says it is enormous (it would take gigabytes to open in everyone's Soju), or lies about its size: refused.
  assert.equal((await call(env, "POST", "/image", tok["a@x.com"], { full: jpg(5006, 65535, 65535), thumb: pic.thumb, w: 65535, h: 65535 })).status, 400);
  assert.equal((await call(env, "POST", "/image", tok["a@x.com"], { full: jpg(5006, 65535, 65535), thumb: pic.thumb, w: 1600, h: 900 })).status, 400);
  assert.equal((await call(env, "POST", "/image", tok["a@x.com"], { ...pic, thumb: jpg(906, 4000, 4000) })).status, 400);
  const i1 = await call(env, "POST", "/image", tok["a@x.com"], pic), i2 = await call(env, "POST", "/image", tok["a@x.com"], pic), i3 = await call(env, "POST", "/image", tok["a@x.com"], pic);
  assert.equal(i1.status, 200);
  const get = async (id, s) => { const url = new URL(`https://didimlabs.com/api/feed/image/${id}?s=${s}`); return feed(new Request(url, { headers: { "cf-connecting-ip": "10.0.0.9" } }), env, url); };
  assert.equal((await get(i1.image, "full")).status, 404);   // waiting for its post
  assert.equal((await call(env, "POST", "/post", tok["a@x.com"], { body: "Three is one too many", images: [i1.image, i2.image, i3.image], ...GAME })).status, 400);
  assert.equal((await call(env, "POST", "/post", tok["b@x.com"], { body: "Someone else's photo", images: [i1.image], ...GAME })).status, 400);
  const p = await call(env, "POST", "/post", tok["a@x.com"], { body: "The ledger under the UV lamp", images: [i1.image, i2.image], ...GAME });
  assert.equal(p.status, 200);
  later(env, 60000);
  assert.equal((await call(env, "POST", "/post", tok["a@x.com"], { body: "A photo on a reply", parent: p.id, images: [i3.image] })).status, 400);
  later(env, 60000);
  assert.equal((await call(env, "POST", "/post", tok["a@x.com"], { body: "The same photo twice", images: [i1.image], ...GAME })).status, 400);   // already on a post
  const list = await call(env, "GET", "", null);
  assert.deepEqual(list.posts[0].images, [{ id: i1.image, w: 1600, h: 900 }, { id: i2.image, w: 1600, h: 900 }]);
  assert.deepEqual((await call(env, "GET", "/thread?id=" + p.id, null)).post.images.map((x) => x.id), [i1.image, i2.image]);
  const full = await get(i1.image, "full"), thumb = await get(i1.image, "thumb");
  assert.equal(full.status, 200); assert.equal(full.headers.get("content-type"), "image/jpeg"); assert.match(full.headers.get("cache-control"), /immutable/);
  assert.equal((await full.arrayBuffer()).byteLength, 5006); assert.equal((await thumb.arrayBuffer()).byteLength, 906);
  // Hidden with its post, gone with its post.
  await call(env, "POST", "/report", tok["op@didimlabs.com"], { post: p.id });
  assert.equal((await get(i1.image, "thumb")).status, 404);
  env.FEED.raw.exec("UPDATE posts SET hidden = 0");
  assert.equal((await call(env, "POST", "/delete", tok["a@x.com"], { post: p.id })).status, 200);
  assert.equal(env.FEED.raw.prepare("SELECT COUNT(*) AS n FROM images WHERE post IS NOT NULL").get().n, 0);
  // News may run long; a member's post may not.
  assert.equal((await call(env, "POST", "/post", tok["op@didimlabs.com"], { body: "n".repeat(3000), news: true, title: "A long read" })).status, 200);
  later(env, 60000);
  assert.equal((await call(env, "POST", "/post", tok["a@x.com"], { body: "n".repeat(3000), news: true, title: "x", ...GAME })).status, 400);
});

test("what an unfriendly visitor would try", async () => {
  const { env, tok } = await setup();
  for (const [e, n] of [["a@x.com", "aaa_one"], ["b@x.com", "bbb_two"], ["c@x.com", "ccc_three"], ["d@x.com", "ddd_four"], ["op@didimlabs.com", "mike_92"]]) await call(env, "POST", "/nick", tok[e], { nick: n });
  // Names that pass for the house.
  const { env: e2, tok: t2 } = await setup();
  for (const bad of ["didimlabs_news", "soju_team", "the_official", "xadminx", "goat_staff"]) assert.equal((await call(e2, "POST", "/nick", t2["a@x.com"], { nick: bad })).status, 400, bad);
  // Three throwaway accounts cannot take news down, nor an operator's post; they can take a member's.
  const news = await call(env, "POST", "/post", tok["op@didimlabs.com"], { body: "The demo is out.", news: true, title: "Demo" });
  const plain = await call(env, "POST", "/post", tok["op@didimlabs.com"], { body: "An operator's ordinary post", ...GAME });
  for (const e of ["a@x.com", "b@x.com", "c@x.com", "d@x.com"]) { await call(env, "POST", "/report", tok[e], { post: news.id }); await call(env, "POST", "/report", tok[e], { post: plain.id }); }
  assert.equal((await call(env, "GET", "", null)).posts.length, 2);
  // A body without a stated length, and one that is too large.
  const url = new URL("https://didimlabs.com/api/feed/post");
  const noLength = await feed(new Request(url, { method: "POST", headers: { authorization: "Bearer " + tok["a@x.com"], "cf-connecting-ip": "10.0.0.2" }, body: "{}" }), env, url);
  assert.equal(noLength.status, 411);
  assert.equal((await call(env, "POST", "/post", tok["a@x.com"], { body: "x".repeat(20000), ...GAME })).status, 413);
  // Text that tries to look like something else: control and direction-override characters are dropped, markup stays text.
  const odd = await call(env, "POST", "/post", tok["a@x.com"], { body: "<script>alert(1)</script>\u202Egnp.exe\u0007", game: "steam:1", game_title: "  A\n\nGame  \u202E" });
  const shown = (await call(env, "GET", "", null)).posts.find((p) => p.id === odd.id);
  assert.equal(shown.body, "<script>alert(1)</script>gnp.exe"); assert.equal(shown.game_title, "A Game");
  // Someone else's post: not theirs to delete; a like or report on a post that does not exist is nothing.
  assert.equal((await call(env, "POST", "/delete", tok["b@x.com"], { post: odd.id })).status, 403);
  assert.equal((await call(env, "POST", "/like", tok["b@x.com"], { post: 424242 })).status, 404);
  assert.equal((await call(env, "POST", "/like", tok["b@x.com"], { post: "1; DROP TABLE posts" })).status, 404);
  // Signing out forgets the token; the dev shortcut is not there without its variable, nor off this machine with it.
  assert.equal((await call(env, "POST", "/signout", tok["b@x.com"], {})).status, 200);
  assert.equal((await call(env, "POST", "/like", tok["b@x.com"], { post: odd.id })).status, 401);
  assert.equal((await call(env, "GET", "/dev-token", null)).status, 404);
  assert.equal((await call({ ...env, FEED_DEV_EMAIL: "x@y.z" }, "GET", "/dev-token", null)).status, 404);
  // Photos do not count against the per-minute limit; everything else does.
  for (let i = 0; i < 130; i++) { const u = new URL("https://didimlabs.com/api/feed/image/999"); assert.equal((await feed(new Request(u, { headers: { "cf-connecting-ip": "10.9.9.9" } }), env, u)).status, 404); }
  let last = 200; for (let i = 0; i < 125; i++) { const u = new URL("https://didimlabs.com/api/feed"); last = (await feed(new Request(u, { headers: { "cf-connecting-ip": "10.9.9.9" } }), env, u)).status; }
  assert.equal(last, 429);
});

test("a member posts a few seconds apart and a set number a day; pages go back by id", async () => {
  const { env, tok } = await setup();
  await call(env, "POST", "/nick", tok["a@x.com"], { nick: "aaa_one" });
  assert.equal((await call(env, "POST", "/post", tok["a@x.com"], { body: "First post", ...GAME })).status, 200);
  assert.equal((await call(env, "POST", "/post", tok["a@x.com"], { body: "Too soon", ...GAME })).status, 429);
  feedReset(); env.FEED = Object.assign(env.FEED, {});   // the per-minute counter starts again; the database stays
  for (let i = 1; i < FEED.PER_DAY; i++) { later(env, FEED.GAP_MS + 1); assert.equal((await call(env, "POST", "/post", tok["a@x.com"], { body: "Post number " + i, ...GAME })).status, 200, "post " + i); }
  later(env, FEED.GAP_MS + 1);
  assert.equal((await call(env, "POST", "/post", tok["a@x.com"], { body: "One too many", ...GAME })).status, 429);
  const first = await call(env, "GET", "", null);
  assert.equal(first.posts.length, FEED.PAGE); assert.equal(first.more, false);
  const older = await call(env, "GET", "?before=" + first.posts[9].id, null);
  assert.equal(older.posts.length, FEED.PAGE - 10); assert.ok(older.posts.every((p) => p.id < first.posts[9].id));
});
