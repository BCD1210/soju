// Anonymous play counts sent by the Soju app (scripts/play-stats.py), kept in the STATS D1 database.
// Served at didimlabs.com/api/stats (worker.js). Soju 1.14.25-1.14.27 send to the older stats-worker.js address.
//
// POST /api/stats {id, kind: "play" | "library", games: [...], mac, chip, macos, soju}
//   play:    games [{game, title, launcher, minutes}]  adds minutes to that install's total for the game
//   library: games [{game, title, launcher}]           replaces that install's list of installed games
//   launch:  games [{game, title, launcher, result, how}] counts one start: result ran | quick_exit | no_start,
//            how soju (the library's Play button) | steam (Steam's own window)
// Nothing is published: the counts are read with wrangler (below).
//
// Read everything: wrangler d1 execute soju-stats --remote --command "select * from plays" -c wrangler.didimlabs.toml

const json = (body, status = 200, extra = {}) => new Response(JSON.stringify(body), {
  status, headers: { "content-type": "application/json", "cache-control": "no-store", ...extra },
});
const LAUNCHERS = new Set(["battlenet", "steam", "epic", "gog", "ea", "rockstar", "ubisoft", "xbox", "didim"]);
const SCHEMA = [
  "CREATE TABLE IF NOT EXISTS plays (install TEXT NOT NULL, game TEXT NOT NULL, title TEXT, launcher TEXT, minutes INTEGER NOT NULL DEFAULT 0, sessions INTEGER NOT NULL DEFAULT 0, mac TEXT, chip TEXT, macos TEXT, soju TEXT, updated TEXT, PRIMARY KEY (install, game))",
  "CREATE TABLE IF NOT EXISTS hits (address TEXT NOT NULL, hour INTEGER NOT NULL, n INTEGER NOT NULL, PRIMARY KEY (address, hour))",
  "CREATE TABLE IF NOT EXISTS launches (install TEXT NOT NULL, game TEXT NOT NULL, title TEXT, launcher TEXT, result TEXT NOT NULL, how TEXT NOT NULL, n INTEGER NOT NULL DEFAULT 0, mac TEXT, chip TEXT, macos TEXT, soju TEXT, updated TEXT, PRIMARY KEY (install, game, result, how))",
  "CREATE TABLE IF NOT EXISTS library (install TEXT NOT NULL, game TEXT NOT NULL, title TEXT, launcher TEXT, mac TEXT, chip TEXT, macos TEXT, soju TEXT, updated TEXT, PRIMARY KEY (install, game))",
];
const RESULTS = new Set(["ran", "quick_exit", "no_start"]);
const HOW = new Set(["soju", "steam"]);
let ready = false, minute = 0, count = 0, perAddress = new Map();

const text = (v, n) => String(v ?? "").replace(/[\u0000-\u001f]/g, " ").trim().slice(0, n);

async function schema(db) {
  if (ready) return;
  await db.batch(SCHEMA.map((s) => db.prepare(s)));
  ready = true;
}

export async function stats(request, env, url) {
  if (!env.STATS) return json({ error: "Not available right now." }, 503);
  if (request.method !== "POST") return json({ error: "Use POST." }, 405);
  const m = Math.floor(Date.now() / 60000);
  if (m !== minute) { minute = m; count = 0; perAddress = new Map(); }
  if (++count > 300) return json({ error: "Busy, try later." }, 429);
  // One Mac sends a few requests a day; more than 10 a minute from one address is not Soju.
  const address = request.headers.get("cf-connecting-ip") || "";
  const sent = (perAddress.get(address) || 0) + 1;
  perAddress.set(address, sent);
  if (sent > 10) return json({ error: "Busy, try later." }, 429);
  if (Number(request.headers.get("content-length") || 0) > 64 * 1024) return json({ error: "Too large." }, 413);
  let input;
  try { input = await request.json(); } catch { return json({ error: "Send JSON." }, 400); }
  const id = text(input?.id, 40).toLowerCase();
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(id)) return json({ error: "Bad id." }, 400);
  const kind = input.kind;
  if (kind !== "play" && kind !== "library" && kind !== "launch") return json({ error: "Bad kind." }, 400);
  const db = env.STATS;
  await schema(db);
  // Exact per-address limit across Cloudflare's machines: a Mac sends a few requests a day.
  const hour = Math.floor(Date.now() / 3600000);
  const hit = await db.prepare("INSERT INTO hits (address, hour, n) VALUES (?, ?, 1) ON CONFLICT (address, hour) DO UPDATE SET n = n + 1 RETURNING n")
    .bind(address || "none", hour).first();
  if (Math.random() < 0.02) await db.prepare("DELETE FROM hits WHERE hour < ?").bind(hour - 1).run();
  if ((hit?.n || 0) > 30) return json({ error: "Busy, try later." }, 429);
  const machine = [text(input.mac, 40), text(input.chip, 60), text(input.macos, 20), text(input.soju, 20)];
  const now = new Date().toISOString();
  const games = (Array.isArray(input.games) ? input.games : []).slice(0, kind === "library" ? 200 : 5)
    .map((g) => ({ game: text(g?.game, 120), title: text(g?.title, 120), launcher: text(g?.launcher, 20), minutes: Math.floor(Number(g?.minutes)),
      result: text(g?.result, 12), how: text(g?.how, 8) }))
    .filter((g) => g.game && LAUNCHERS.has(g.launcher) && (kind === "library" ||
      (kind === "launch" ? RESULTS.has(g.result) && HOW.has(g.how) : g.minutes >= 1 && g.minutes <= 720)));
  if (!games.length) return json({ ok: true });
  if (kind === "launch") {
    await db.batch(games.map((g) => db.prepare(
      "INSERT INTO launches (install, game, title, launcher, result, how, n, mac, chip, macos, soju, updated) VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?) " +
      "ON CONFLICT (install, game, result, how) DO UPDATE SET n = n + 1, title = excluded.title, mac = excluded.mac, chip = excluded.chip, " +
      "macos = excluded.macos, soju = excluded.soju, updated = excluded.updated",
    ).bind(id, g.game, g.title, g.launcher, g.result, g.how, ...machine, now)));
  } else if (kind === "play") {
    await db.batch(games.map((g) => db.prepare(
      "INSERT INTO plays (install, game, title, launcher, minutes, sessions, mac, chip, macos, soju, updated) VALUES (?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?) " +
      "ON CONFLICT (install, game) DO UPDATE SET minutes = minutes + excluded.minutes, sessions = sessions + 1, title = excluded.title, " +
      "mac = excluded.mac, chip = excluded.chip, macos = excluded.macos, soju = excluded.soju, updated = excluded.updated",
    ).bind(id, g.game, g.title, g.launcher, g.minutes, ...machine, now)));
  } else {
    await db.batch([db.prepare("DELETE FROM library WHERE install = ?").bind(id), ...games.map((g) => db.prepare(
      "INSERT OR REPLACE INTO library (install, game, title, launcher, mac, chip, macos, soju, updated) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    ).bind(id, g.game, g.title, g.launcher, ...machine, now))]);
  }
  return json({ ok: true });
}
