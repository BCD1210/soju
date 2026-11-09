// Two small opt-in features. Neither stores anything that names a person except the email
// someone types to get release news.
//
// POST /api/notify {"email", "source"} keeps the address in the AUTH KV namespace under "n:<email>"
// (no expiry). Read the list with: wrangler kv key list --binding AUTH --prefix n: --remote
//
// POST /api/stats {"event", "version", "install"} counts one anonymous event in the STATS D1 database.
// "install" is a random ID Soju makes when the user turns usage counts on; it is not tied to an
// account, email or Mac. Only the events below are accepted.
const EVENTS = new Set(["app_open", "store_open", "demo_download", "demo_play", "purchase_start", "purchase_done", "signin"]);
const json = (body, status = 200) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json", "cache-control": "no-store" } });
let minute = 0, notifyCount = 0, statsCount = 0;
const tick = () => { const m = Math.floor(Date.now() / 60000); if (m !== minute) { minute = m; notifyCount = 0; statsCount = 0; } };

export async function notify(request, env) {
  if (request.method !== "POST") return json({ error: "Use POST." }, 405);
  if (!env.AUTH) return json({ error: "Not available right now." }, 503);
  let input;
  const type = request.headers.get("content-type") || "";
  try { input = type.includes("json") ? await request.json() : Object.fromEntries(await request.formData()); } catch { return json({ error: "Send the form." }, 400); }
  if (input.website) return json({ ok: true }); // honeypot
  const email = String(input.email || "").trim().toLowerCase().slice(0, 200);
  if (!/^[^\s@<>]+@[^\s@<>]+\.[^\s@<>]{2,}$/.test(email)) return json({ error: "That email doesn't look right." }, 400);
  tick();
  if (++notifyCount > 20) return json({ error: "Too many sign-ups right now. Try again in a minute." }, 429);
  const source = String(input.source || "site").replace(/[^a-z0-9-]/g, "").slice(0, 20) || "site";
  await env.AUTH.put("n:" + email, JSON.stringify({ at: new Date().toISOString(), source }));
  return json({ ok: true });
}

export async function stats(request, env) {
  if (request.method !== "POST") return json({ error: "Use POST." }, 405);
  if (!env.STATS) return json({ ok: false }, 503);
  let input;
  try { input = await request.json(); } catch { return json({ error: "Send JSON." }, 400); }
  const event = String(input.event || ""), version = String(input.version || "").slice(0, 20), install = String(input.install || "");
  if (!EVENTS.has(event) || !/^[0-9A-Fa-f-]{36}$/.test(install) || !/^[0-9.]{1,20}$/.test(version)) return json({ error: "Unknown event." }, 400);
  tick();
  if (++statsCount > 600) return json({ ok: false }, 429);
  await env.STATS.prepare(
    "CREATE TABLE IF NOT EXISTS events (day TEXT NOT NULL, event TEXT NOT NULL, version TEXT NOT NULL, install TEXT NOT NULL)"
  ).run();
  await env.STATS.prepare("INSERT INTO events (day, event, version, install) VALUES (date('now'), ?1, ?2, ?3)")
    .bind(event, version, install.toLowerCase()).run();
  return json({ ok: true });
}
