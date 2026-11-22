// The play-count endpoint (site-worker/stats.js) against a stand-in D1 that records statements.
import test from "node:test";
import assert from "node:assert/strict";
import { stats } from "../site-worker/stats.js";

function fakeDb() {
  const ran = [];
  const stmt = (sql) => ({ sql, args: [], bind(...a) { this.args = a; return this; }, async first() { ran.push(this); return { n: 1 }; }, async run() { ran.push(this); return {}; } });
  return { ran, prepare: stmt, async batch(list) { ran.push(...list); return []; } };
}
const id = "3f2b8a10-1111-4222-8333-944455556666";
const post = (body, ip) => new Request("https://didimlabs.com/api/stats", {
  method: "POST", headers: { "content-type": "application/json", "cf-connecting-ip": ip }, body: JSON.stringify(body) });

test("a launch is counted once per install, game, result and how", async () => {
  const db = fakeDb();
  const res = await stats(post({ id, kind: "launch", soju: "1.14.32", games: [
    { game: "steam:1", title: "A", launcher: "steam", result: "quick_exit", how: "soju" }] }, "10.0.0.1"), { STATS: db });
  assert.equal(res.status, 200);
  const insert = db.ran.find((s) => s.sql.startsWith("INSERT INTO launches"));
  assert.ok(insert, "launch row written");
  assert.deepEqual(insert.args.slice(0, 6), [id, "steam:1", "A", "steam", "quick_exit", "soju"]);
});

test("a start that did not run carries a reason: one known word, one plain name, a known anti-cheat", async () => {
  const db = fakeDb();
  await stats(post({ id, kind: "launch", games: [
    { game: "steam:1", title: "A", launcher: "steam", result: "quick_exit", how: "steam", why: "missing_dll", detail: "vcruntime140.dll", ac: "eac" },
    { game: "steam:2", title: "B", launcher: "steam", result: "no_start", how: "soju", why: "/Users/someone", detail: "C:\\Users\\me\\x.dll", ac: "mine" }] }, "10.0.0.9"), { STATS: db });
  const rows = db.ran.filter((s) => s.sql.startsWith("INSERT INTO launches"));
  assert.deepEqual(rows[0].args.slice(-3), ["missing_dll", "vcruntime140.dll", "eac"]);
  assert.deepEqual(rows[1].args.slice(-3), ["", "", ""]);
});

test("unknown results, ways of starting and kinds are refused or ignored", async () => {
  const db = fakeDb();
  await stats(post({ id, kind: "launch", games: [{ game: "steam:1", launcher: "steam", result: "exploded", how: "soju" },
    { game: "steam:2", launcher: "steam", result: "ran", how: "terminal" }] }, "10.0.0.2"), { STATS: db });
  assert.equal(db.ran.filter((s) => s.sql.startsWith("INSERT INTO launches")).length, 0);
  const res = await stats(post({ id, kind: "crash", games: [] }, "10.0.0.3"), { STATS: fakeDb() });
  assert.equal(res.status, 400);
});

test("play minutes still need to be between 1 and 720", async () => {
  const db = fakeDb();
  await stats(post({ id, kind: "play", games: [{ game: "steam:1", launcher: "steam", minutes: 0 },
    { game: "steam:2", launcher: "steam", minutes: 5 }] }, "10.0.0.4"), { STATS: db });
  const plays = db.ran.filter((s) => s.sql.startsWith("INSERT INTO plays"));
  assert.deepEqual(plays.map((s) => s.args[1]), ["steam:2"]);
});
