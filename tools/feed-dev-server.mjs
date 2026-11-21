// The Didim Labs feed on this machine, for trying the app's Feed shelf without the deployed site:
//   node tools/feed-dev-server.mjs [port] [email] [database file]
// then start Soju with SOJU_FEED_URL=http://127.0.0.1:<port>. It runs site-worker/feed.js against a local SQLite file,
// prints a token for <email> (an operator here, so news can be posted), and writes it where Soju reads it when asked with --save-token.
import http from "node:http";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";
import { feed, issueToken } from "../site-worker/feed.js";

const args = process.argv.slice(2).filter((a) => !a.startsWith("--")), save = process.argv.includes("--save-token");
const port = Number(args[0]) || 8787, email = (args[1] || "dev@didimlabs.test").toLowerCase(), file = args[2] || path.join(os.tmpdir(), "soju-feed-dev.sqlite");
const db = new DatabaseSync(file);
const val = (v) => v === undefined ? null : typeof v === "boolean" ? (v ? 1 : 0) : v;
const prepare = (sql) => { const make = (a) => ({ sql, bind: (...more) => make(more.map(val)),
  run: async () => ({ meta: { changes: Number(db.prepare(sql).run(...a).changes) } }),
  first: async () => db.prepare(sql).get(...a) || null, all: async () => ({ results: db.prepare(sql).all(...a) }) }); return make([]); };
const env = { FEED: { prepare, batch: async (list) => { const out = []; for (const s of list) out.push(/^\s*SELECT/i.test(s.sql) ? await s.all() : await s.run()); return out; } }, FEED_ADMINS: email };
const token = await issueToken(env, email);
if (save) { const f = path.join(os.homedir(), ".battlenet-macos", "didim-feed-token"); fs.mkdirSync(path.dirname(f), { recursive: true }); fs.writeFileSync(f, token, { mode: 0o600 }); fs.chmodSync(f, 0o600); console.log("token saved for Soju"); }
else console.log("token (pass --save-token to hand it to Soju): written to no file");
http.createServer(async (req, res) => {
  const url = new URL(req.url, "http://127.0.0.1:" + port), chunks = [];
  for await (const c of req) chunks.push(c);
  const headers = new Headers(); for (const [k, v] of Object.entries(req.headers)) if (typeof v === "string") headers.set(k, v);
  headers.set("cf-connecting-ip", "127.0.0.1");
  const answer = url.pathname.startsWith("/api/feed") ? await feed(new Request(url, { method: req.method, headers, body: ["GET", "HEAD"].includes(req.method) ? undefined : Buffer.concat(chunks) }), env, url)
    : new Response("not found", { status: 404 });
  res.writeHead(answer.status, Object.fromEntries(answer.headers)); res.end(Buffer.from(await answer.arrayBuffer()));
}).listen(port, "127.0.0.1", () => console.log(`feed on http://127.0.0.1:${port} as ${email}, database ${file}`));
