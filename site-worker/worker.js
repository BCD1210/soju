// didimlabs.com: serves the GitHub Pages site (docs/) under a short domain.
//   curl -fsSL didimlabs.com/install.sh | bash
// The old soju.snack-wrap.com address (and www) redirect here so existing links and install commands keep working.
//
// POST /api/didim/download {"game": "<id>", "key": "<license key>"} is how Soju turns a Didim Labs
// license key (sold through Polar) into a short-lived download link. Needs the POLAR_TOKEN secret
// (organization token with license_keys:read and customer_sessions:write) on the didimlabs.com worker.
//
// In-app purchase without copying a key (the token also needs checkouts:read and checkouts:write):
//   POST /api/didim/checkout {"game": "<id>"} -> {"id", "url"}: Soju opens url in the browser.
//   GET /api/didim/checkout/<id> -> {"status": "open" | "processing" | "ready" | "expired" | "failed", "game"?, "key"?}
//   The website's thanks page links to soju://store/claim?checkout=<id> so a web purchase installs the same way.
//   Soju polls until "ready", then installs with the key. The checkout id is only known to that Soju.
//
// POST /api/sponsor/inquiry (form fields name, email, company, message) mails an advertising inquiry
// through the MAILER send_email binding to the SPONSOR_TO secret (a verified Email Routing address).
import { EmailMessage } from "cloudflare:email";
import { auth } from "./auth.js";
import { notify } from "./community.js";
const HOME = "didimlabs.com";
const POLAR = "https://api.polar.sh/v1";
const ORG = "e7dd3a2e-a0ac-4b52-ad53-b7c7d7064420";
// Game ID (soju-game.json) -> the Polar benefits that grant it.
const GAMES = {
  "dont-sell-the-cat": {
    product: "d1151bb7-78f2-4328-b4b1-bf3a7ae86abc",
    license: "e8fc1b75-2c42-4808-9486-14aab05e3bdb",
    download: "4c51886f-92d5-4620-868e-ce6aeb01adfb",
  },
};

const json = (body, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json", "cache-control": "no-store" } });

async function download(request, env) {
  if (request.method !== "POST") return json({ error: "Use POST." }, 405);
  if (!env.POLAR_TOKEN) return json({ error: "The store is not configured yet." }, 503);
  let input;
  try { input = await request.json(); } catch { return json({ error: "Send JSON." }, 400); }
  const game = GAMES[input && input.game];
  const key = typeof input?.key === "string" ? input.key.trim() : "";
  if (!game) return json({ error: "Unknown game." }, 404);
  if (!/^[A-Za-z0-9-]{8,100}$/.test(key)) return json({ error: "That license key does not look right." }, 400);

  const check = await fetch(POLAR + "/customer-portal/license-keys/validate", {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ key, organization_id: ORG }),
  });
  if (check.status === 404 || check.status === 422) return json({ error: "This license key was not found." }, 403);
  if (!check.ok) return json({ error: "The store could not check the key. Try again shortly." }, 502);
  const license = await check.json();
  if (license.status !== "granted") return json({ error: "This license key is no longer active." }, 403);
  if (license.benefit_id !== game.license) return json({ error: "This key is for a different game." }, 403);

  const session = await fetch(POLAR + "/customer-sessions/", {
    method: "POST", headers: { "content-type": "application/json", authorization: "Bearer " + env.POLAR_TOKEN },
    body: JSON.stringify({ customer_id: license.customer_id }),
  });
  if (!session.ok) return json({ error: "The store could not prepare the download. Try again shortly." }, 502);
  const { token } = await session.json();
  const list = await fetch(POLAR + "/customer-portal/downloadables/?limit=100&benefit_id=" + game.download, {
    headers: { authorization: "Bearer " + token },
  });
  if (!list.ok) return json({ error: "The store could not list the download. Try again shortly." }, 502);
  const files = ((await list.json()).items || []).filter((item) => item.benefit_id === game.download && item.file);
  // The newest upload is the current build.
  files.sort((a, b) => String(b.file.created_at || "").localeCompare(String(a.file.created_at || "")));
  const file = files[0] && files[0].file;
  if (!file) return json({ error: "No download is attached to this game yet." }, 404);
  return json({
    name: file.name, size: file.size, sha256: file.checksum_sha256_hex,
    url: file.download.url, expires_at: file.download.expires_at,
  });
}

const polar = (env, path, init = {}) => fetch(POLAR + path, {
  ...init, headers: { "content-type": "application/json", authorization: "Bearer " + env.POLAR_TOKEN, ...(init.headers || {}) },
});
let checkoutMinute = 0, checkoutsThisMinute = 0;

async function startCheckout(request, env) {
  if (request.method !== "POST") return json({ error: "Use POST." }, 405);
  if (!env.POLAR_TOKEN) return json({ error: "The store is not configured yet." }, 503);
  let input;
  try { input = await request.json(); } catch { return json({ error: "Send JSON." }, 400); }
  const id = input && input.game;
  const game = Object.hasOwn(GAMES, id) ? GAMES[id] : null;
  if (!game || !game.product) return json({ error: "Unknown game." }, 404);
  const minute = Math.floor(Date.now() / 60000);
  if (minute !== checkoutMinute) { checkoutMinute = minute; checkoutsThisMinute = 0; }
  if (++checkoutsThisMinute > 30) return json({ error: "Too many checkouts right now. Try again in a minute." }, 429);
  const res = await polar(env, "/checkouts/", {
    method: "POST",
    body: JSON.stringify({
      products: [game.product],
      success_url: "https://" + HOME + "/store/thanks.html?checkout_id={CHECKOUT_ID}",
      metadata: { source: "soju", game: id },
    }),
  });
  if (!res.ok) return json({ error: "The store could not start the checkout. Try again shortly." }, 502);
  const checkout = await res.json();
  return json({ id: checkout.id, url: checkout.url });
}

async function checkoutStatus(env, checkoutId) {
  if (!env.POLAR_TOKEN) return json({ error: "The store is not configured yet." }, 503);
  const res = await polar(env, "/checkouts/" + checkoutId);
  if (res.status === 404 || res.status === 422) return json({ error: "Unknown checkout." }, 404);
  if (!res.ok) return json({ error: "The store could not check the payment." }, 502);
  const checkout = await res.json();
  const productIds = [checkout.product_id, ...(checkout.products || []).map((p) => p.id)];
  const gameId = Object.keys(GAMES).find((id) => GAMES[id].product && productIds.includes(GAMES[id].product));
  const game = gameId && GAMES[gameId];
  if (!game) return json({ error: "Unknown checkout." }, 404);
  if (checkout.status === "expired" || checkout.status === "failed") return json({ status: checkout.status });
  if (checkout.status !== "succeeded" || !checkout.customer_id) {
    return json({ status: checkout.status === "confirmed" || checkout.status === "succeeded" ? "processing" : "open" });
  }
  // The license key is granted a few seconds after the payment; until then the answer is "processing".
  for (let page = 1; page <= 20; page++) {
    const list = await polar(env, `/license-keys/?organization_id=${ORG}&benefit_id=${game.license}&limit=100&page=${page}`);
    if (!list.ok) return json({ error: "The store could not find the license key yet." }, 502);
    const body = await list.json();
    const found = (body.items || []).find((k) => k.customer_id === checkout.customer_id && k.status === "granted");
    if (found) return json({ status: "ready", game: gameId, key: found.key });
    if (page >= ((body.pagination && body.pagination.max_page) || 1)) break;
  }
  return json({ status: "processing" });
}

const clean = (v, max) => String(v ?? "").replace(/[\u0000-\u0008\u000b-\u001f\u007f]/g, "").trim().slice(0, max);
const b64 = (text) => btoa(String.fromCharCode(...new TextEncoder().encode(text)));
let lastMinute = 0, sentThisMinute = 0;

async function inquiry(request, env) {
  if (request.method !== "POST") return json({ error: "Use POST." }, 405);
  const site = request.headers.get("sec-fetch-site");
  if (site && site !== "same-origin") return json({ error: "Forbidden." }, 403);
  if (!env.MAILER || !env.SPONSOR_TO) return json({ error: "Please email hello@didimlabs.com." }, 503);
  let form;
  try { form = await request.formData(); } catch { return json({ error: "Send the form." }, 400); }
  if (form.get("website")) return json({ ok: true }); // honeypot
  const name = clean(form.get("name"), 100);
  const email = clean(form.get("email"), 200).replace(/[\r\n]/g, "");
  const company = clean(form.get("company"), 120);
  const message = clean(form.get("message"), 4000);
  if (!name || !/^[^\s@<>]+@[^\s@<>]+\.[^\s@<>]+$/.test(email) || message.length < 5) {
    return json({ error: "Fill in your name, a valid email and a message." }, 400);
  }
  const minute = Math.floor(Date.now() / 60000);
  if (minute !== lastMinute) { lastMinute = minute; sentThisMinute = 0; }
  if (++sentThisMinute > 5) return json({ error: "Too many messages right now. Please email hello@didimlabs.com." }, 429);
  const subject = `Soju sponsor inquiry: ${company || name}`;
  const body = [`Name: ${name}`, `Email: ${email}`, `Company: ${company || "-"}`, "", message].join("\n");
  const raw = [
    "From: Soju website <hello@didimlabs.com>",
    `To: ${env.SPONSOR_TO}`,
    `Reply-To: ${email}`,
    `Subject: =?UTF-8?B?${b64(subject)}?=`,
    `Date: ${new Date().toUTCString()}`,
    `Message-ID: <${crypto.randomUUID()}@didimlabs.com>`,
    "MIME-Version: 1.0",
    "Content-Type: text/plain; charset=utf-8",
    "Content-Transfer-Encoding: base64",
    "",
    b64(body).replace(/.{76}/g, "$&\r\n"),
  ].join("\r\n");
  try {
    await env.MAILER.send(new EmailMessage("hello@didimlabs.com", env.SPONSOR_TO, raw));
  } catch (e) {
    return json({ error: "Sending failed. Please email hello@didimlabs.com." }, 502);
  }
  return json({ ok: true });
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.hostname !== HOME) {
      return Response.redirect("https://" + HOME + url.pathname + url.search, 301);
    }
    if (url.pathname === "/api/didim/download") return download(request, env);
    if (url.pathname === "/api/notify") return notify(request, env);
    if (url.pathname.startsWith("/auth/")) return auth(request, env, url, { home: HOME, games: GAMES, polar, org: ORG });
    if (url.pathname === "/api/didim/checkout") return startCheckout(request, env);
    const checkoutPath = url.pathname.match(/^\/api\/didim\/checkout\/([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$/);
    if (checkoutPath) return request.method === "GET" ? checkoutStatus(env, checkoutPath[1]) : json({ error: "Use GET." }, 405);
    if (url.pathname === "/api/sponsor/inquiry") return inquiry(request, env);
    const upstream = new URL("https://bcd1210.github.io/soju" + url.pathname + url.search);
    const res = await fetch(upstream, { headers: { "User-Agent": "soju-site" }, cf: { cacheTtlByStatus: { "200-299": 300, "300-599": 0 } } });
    const headers = new Headers(res.headers);
    headers.delete("content-security-policy");
    return new Response(res.body, { status: res.status, headers });
  },
};
