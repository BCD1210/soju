// didimlabs.com: serves the GitHub Pages site (docs/) under a short domain.
//   curl -fsSL didimlabs.com/install.sh | bash
// The old soju.snack-wrap.com address (and www) redirect here so existing links and install commands keep working.
//
// POST /api/didim/download {"game": "<id>", "key": "<license key>"} is how Soju turns a Didim Labs
// license key (sold through Polar) into a short-lived download link. Needs the POLAR_TOKEN secret
// (organization token with license_keys:read and customer_sessions:write) on the didimlabs.com worker.
//
// POST /api/sponsor/inquiry (form fields name, email, company, message) mails an advertising inquiry
// through the MAILER send_email binding to the SPONSOR_TO secret (a verified Email Routing address).
import { EmailMessage } from "cloudflare:email";
const HOME = "didimlabs.com";
const POLAR = "https://api.polar.sh/v1";
const ORG = "e7dd3a2e-a0ac-4b52-ad53-b7c7d7064420";
// Game ID (soju-game.json) -> the Polar benefits that grant it.
const GAMES = {
  "dont-sell-the-cat": {
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
    if (url.pathname === "/api/sponsor/inquiry") return inquiry(request, env);
    const upstream = new URL("https://bcd1210.github.io/soju" + url.pathname + url.search);
    const res = await fetch(upstream, { headers: { "User-Agent": "soju-site" }, cf: { cacheTtlByStatus: { "200-299": 300, "300-599": 0 } } });
    const headers = new Headers(res.headers);
    headers.delete("content-security-policy");
    return new Response(res.body, { status: res.status, headers });
  },
};
