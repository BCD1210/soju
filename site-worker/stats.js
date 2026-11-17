// Anonymous play counts sent by the Soju app (scripts/play-stats.py), kept in the STATS D1 database.
// Served by stats-worker.js at soju-stats.mikey9220.workers.dev (wrangler.stats.toml).
//
// POST /api/stats {id, kind: "play" | "library", games: [...], mac, chip, macos, soju}
//   play:    games [{game, title, launcher, minutes}]  adds minutes to that install's total for the game
//   library: games [{game, title, launcher}]           replaces that install's list of installed games
// Nothing is published: the counts are read with wrangler (below).
//
// Read everything: wrangler d1 execute soju-stats --remote --command "select * from plays" -c wrangler.stats.toml

const json = (body, status = 200, extra = {}) => new Response(JSON.stringify(body), {
  status, headers: { "content-type": "application/json", "cache-control": "no-store", ...extra },
});
const LAUNCHERS = new Set(["battlenet", "steam", "epic", "gog", "ea", "rockstar", "ubisoft", "xbox", "didim"]);
const SCHEMA = [
  "CREATE TABLE IF NOT EXISTS plays (install TEXT NOT NULL, game TEXT NOT NULL, title TEXT, launcher TEXT, minutes INTEGER NOT NULL DEFAULT 0, sessions INTEGER NOT NULL DEFAULT 0, mac TEXT, chip TEXT, macos TEXT, soju TEXT, updated TEXT, PRIMARY KEY (install, game))",
  "CREATE TABLE IF NOT EXISTS library (install TEXT NOT NULL, game TEXT NOT NULL, title TEXT, launcher TEXT, mac TEXT, chip TEXT, macos TEXT, soju TEXT, updated TEXT, PRIMARY KEY (install, game))",
];
let ready = false, minute = 0, count = 0;

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
  if (m !== minute) { minute = m; count = 0; }
  if (++count > 300) return json({ error: "Busy, try later." }, 429);
  let input;
  try { input = await request.json(); } catch { return json({ error: "Send JSON." }, 400); }
  const id = text(input?.id, 40).toLowerCase();
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(id)) return json({ error: "Bad id." }, 400);
  const kind = input.kind;
  if (kind !== "play" && kind !== "library") return json({ error: "Bad kind." }, 400);
  const machine = [text(input.mac, 40), text(input.chip, 60), text(input.macos, 20), text(input.soju, 20)];
  const now = new Date().toISOString();
  const games = (Array.isArray(input.games) ? input.games : []).slice(0, kind === "play" ? 5 : 300)
    .map((g) => ({ game: text(g?.game, 120), title: text(g?.title, 120), launcher: text(g?.launcher, 20), minutes: Math.floor(Number(g?.minutes)) }))
    .filter((g) => g.game && LAUNCHERS.has(g.launcher) && (kind === "library" || (g.minutes >= 1 && g.minutes <= 720)));
  if (!games.length) return json({ ok: true });
  const db = env.STATS;
  await schema(db);
  if (kind === "play") {
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
