// Release news sign-up.
//
// POST /api/notify {"email", "source"} keeps the address in the AUTH KV namespace under "n:<email>"
// (no expiry). Read the list with: wrangler kv key list --binding AUTH --prefix n: --remote

const json = (body, status = 200) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json", "cache-control": "no-store" } });
let minute = 0, notifyCount = 0;
const tick = () => { const m = Math.floor(Date.now() / 60000); if (m !== minute) { minute = m; notifyCount = 0; } };

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
