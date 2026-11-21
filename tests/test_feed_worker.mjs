// The Didim Labs feed (site-worker/feed.js) against an in-memory SQLite standing in for D1.
import test from "node:test";
import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";
import { feed, issueToken, feedReset, FEED } from "../site-worker/feed.js";

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
  const r = await feed(new Request(url, { method, headers: { "content-type": "application/json", "cf-connecting-ip": "10.0.0.1", ...(token ? { authorization: "Bearer " + token } : {}) }, body: body ? JSON.stringify(body) : undefined }), env, url);
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
  assert.deepEqual({ ...list.posts[0], at: 0 }, { id: p.id, kind: "player", nick: "nyang_maker", title: "", body: "Night four: shine the UV lamp on the ledger.", game: GAME.game, game_title: GAME.game_title, at: 0, likes: 0, liked: false, replies: 0, mine: false, tail: [] });
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
