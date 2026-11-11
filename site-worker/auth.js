// Sign in with Apple or Google so Soju can find what someone bought with that email.
//   GET  /auth/start?provider=apple|google&state=<random from Soju>  -> provider sign-in page
//   GET  /auth/callback/google, POST /auth/callback/apple             -> look up purchases, keep the result
//   GET  /auth/poll/<state>                                            -> Soju picks the result up once
//   GET  /auth/providers                                               -> which buttons Soju should show
// The result (email and license keys) sits in the AUTH KV namespace for 10 minutes under the state
// that only that Soju knows. Secrets: GOOGLE_CLIENT_ID, GOOGLE_CLIENT_SECRET, APPLE_SERVICES_ID,
// APPLE_TEAM_ID, APPLE_KEY_ID, APPLE_PRIVATE_KEY (the .p8 text).
const STATE = /^[A-Za-z0-9_-]{32,64}$/;
const TTL = 600;

const page = (title, text) => new Response(`<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>${title}</title><style>body{font:16px/1.5 -apple-system,system-ui,sans-serif;background:#121413;color:#e7e9e6;display:grid;place-items:center;min-height:90vh;margin:0 16px}
main{max-width:440px;text-align:center}img{width:72px;height:72px}h1{font-size:22px;margin:12px 0 6px}p{color:#a6ada7}</style>
<main><img src="/assets/soju-logo.png" alt=""><h1>${title}</h1><p>${text}</p></main>`, { headers: { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" } });

const b64url = (bytes) => btoa(String.fromCharCode(...new Uint8Array(bytes))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
const b64urlText = (text) => b64url(new TextEncoder().encode(text));
const claims = (jwt) => JSON.parse(new TextDecoder().decode(Uint8Array.from(atob(jwt.split(".")[1].replace(/-/g, "+").replace(/_/g, "/")), (c) => c.charCodeAt(0))));

async function appleSecret(env) {
  const pem = env.APPLE_PRIVATE_KEY.replace(/-----[^-]+-----/g, "").replace(/\s+/g, "");
  const key = await crypto.subtle.importKey("pkcs8", Uint8Array.from(atob(pem), (c) => c.charCodeAt(0)),
    { name: "ECDSA", namedCurve: "P-256" }, false, ["sign"]);
  const now = Math.floor(Date.now() / 1000);
  const head = b64urlText(JSON.stringify({ alg: "ES256", kid: env.APPLE_KEY_ID }));
  const body = b64urlText(JSON.stringify({ iss: env.APPLE_TEAM_ID, iat: now, exp: now + 300, aud: "https://appleid.apple.com", sub: env.APPLE_SERVICES_ID }));
  const sig = await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, key, new TextEncoder().encode(head + "." + body));
  return head + "." + body + "." + b64url(sig);
}

const providers = {
  google: {
    ready: (env) => env.GOOGLE_CLIENT_ID && env.GOOGLE_CLIENT_SECRET,
    authorize: (env, state, redirect) => "https://accounts.google.com/o/oauth2/v2/auth?" + new URLSearchParams({
      client_id: env.GOOGLE_CLIENT_ID, redirect_uri: redirect, response_type: "code", scope: "openid email",
      state, prompt: "select_account",
    }),
    token: async (env, code, redirect) => fetch("https://oauth2.googleapis.com/token", {
      method: "POST", headers: { "content-type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({ code, client_id: env.GOOGLE_CLIENT_ID, client_secret: env.GOOGLE_CLIENT_SECRET, redirect_uri: redirect, grant_type: "authorization_code" }),
    }),
    clientId: (env) => env.GOOGLE_CLIENT_ID,
    issuers: ["https://accounts.google.com", "accounts.google.com"],
  },
  apple: {
    ready: (env) => env.APPLE_SERVICES_ID && env.APPLE_TEAM_ID && env.APPLE_KEY_ID && env.APPLE_PRIVATE_KEY,
    authorize: (env, state, redirect) => "https://appleid.apple.com/auth/authorize?" + new URLSearchParams({
      client_id: env.APPLE_SERVICES_ID, redirect_uri: redirect, response_type: "code", scope: "email",
      response_mode: "form_post", state,
    }),
    token: async (env, code, redirect) => fetch("https://appleid.apple.com/auth/token", {
      method: "POST", headers: { "content-type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({ code, client_id: env.APPLE_SERVICES_ID, client_secret: await appleSecret(env), redirect_uri: redirect, grant_type: "authorization_code" }),
    }),
    clientId: (env) => env.APPLE_SERVICES_ID,
    issuers: ["https://appleid.apple.com"],
  },
};

// Every granted license key for the given games whose customer has this email.
async function purchases(env, email, games, polar, org) {
  const found = [];
  for (const [id, game] of Object.entries(games)) {
    for (let page = 1; page <= 50; page++) {
      const res = await polar(env, `/license-keys/?organization_id=${org}&benefit_id=${game.license}&limit=100&page=${page}`);
      if (!res.ok) throw new Error("license lookup failed");
      const body = await res.json();
      const mine = (body.items || []).find((k) => k.status === "granted" && String(k.customer?.email || "").toLowerCase() === email);
      if (mine) { found.push({ game: id, key: mine.key }); break; }
      if (page >= (body.pagination?.max_page || 1)) break;
    }
  }
  return found;
}

export async function auth(request, env, url, { home, games, polar, org }) {
  const json = (body, status = 200) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json", "cache-control": "no-store" } });
  if (url.pathname === "/auth/providers") {
    return json({ providers: env.AUTH ? Object.keys(providers).filter((name) => providers[name].ready(env)) : [] });
  }
  if (!env.AUTH) return page("Sign-in is not set up yet", "Please use the license key from your receipt email for now.");

  if (url.pathname === "/auth/start") {
    const name = url.searchParams.get("provider"), state = url.searchParams.get("state") || "";
    const p = Object.hasOwn(providers, name) ? providers[name] : null;
    if (!p || !STATE.test(state)) return page("That sign-in link is broken", "Start again from Soju.");
    if (!p.ready(env)) return page("This sign-in is not available yet", "Please use the other sign-in button or your license key.");
    await env.AUTH.put("s:" + state, JSON.stringify({ status: "pending", provider: name }), { expirationTtl: TTL });
    return Response.redirect(p.authorize(env, state, `https://${home}/auth/callback/${name}`), 302);
  }

  const cb = url.pathname.match(/^\/auth\/callback\/(apple|google)$/);
  if (cb) {
    const name = cb[1], p = providers[name];
    const params = request.method === "POST" ? new URLSearchParams(await request.text()) : url.searchParams;
    const state = params.get("state") || "", code = params.get("code") || "";
    if (!STATE.test(state)) return page("That sign-in link is broken", "Start again from Soju.");
    const saved = await env.AUTH.get("s:" + state, "json");
    if (!saved || saved.status !== "pending" || saved.provider !== name) return page("This sign-in has expired", "Start again from Soju.");
    const fail = async (title, text, reason) => {
      await env.AUTH.put("s:" + state, JSON.stringify({ status: "failed", error: reason }), { expirationTtl: TTL });
      return page(title, text);
    };
    if (!code) return fail("Sign-in was cancelled", "You can close this tab.", "Sign-in was cancelled.");
    const res = await p.token(env, code, `https://${home}/auth/callback/${name}`);
    if (!res.ok) return fail("Sign-in didn't work", "Close this tab and try again from Soju.", "The sign-in could not be confirmed.");
    const token = await res.json();
    let c;
    try { c = claims(token.id_token); } catch { return fail("Sign-in didn't work", "Close this tab and try again from Soju.", "The sign-in could not be confirmed."); }
    const verified = c.email_verified === true || c.email_verified === "true";
    if (c.aud !== p.clientId(env) || !p.issuers.includes(c.iss) || !c.email || !verified || c.exp * 1000 < Date.now()) {
      return fail("Sign-in didn't work", "Close this tab and try again from Soju.", "That account has no confirmed email.");
    }
    const email = String(c.email).toLowerCase();
    let owned;
    try { owned = await purchases(env, email, games, polar, org); }
    catch { return fail("The store is busy", "Close this tab and try again from Soju in a minute.", "The store could not look up purchases."); }
    // The result goes only to the Soju on this Mac: the browser hands it a one-time code through
    // soju://auth/done, and the poll must show that code. Someone who sent this person a sign-in
    // link with their own state never sees the code, so they cannot read the email or keys.
    const handoff = b64url(crypto.getRandomValues(new Uint8Array(32)));
    await env.AUTH.put("s:" + state, JSON.stringify({ status: "done", handoff, email, private: c.is_private_email === true || c.is_private_email === "true", owned }), { expirationTtl: TTL });
    const open = `soju://auth/done?${new URLSearchParams({ state, code: handoff })}`;
    return new Response(`<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>You're signed in</title><style>body{font:16px/1.5 -apple-system,system-ui,sans-serif;background:#121413;color:#e7e9e6;display:grid;place-items:center;min-height:90vh;margin:0 16px}
main{max-width:440px;text-align:center}img{width:72px;height:72px}h1{font-size:22px;margin:12px 0 6px}p{color:#a6ada7}a{color:#e7e9e6}</style>
<main><img src="/assets/soju-logo.png" alt=""><h1>You're signed in</h1><p>Allow the browser to open Soju. If nothing happens, <a href="${open}">open Soju</a>. Then you can close this tab.</p></main>
<script>location.href=${JSON.stringify(open)};</script>`, { headers: { "content-type": "text/html; charset=utf-8", "cache-control": "no-store", "referrer-policy": "no-referrer" } });
  }

  const poll = url.pathname.match(/^\/auth\/poll\/([A-Za-z0-9_-]{32,64})$/);
  if (poll) {
    const saved = await env.AUTH.get("s:" + poll[1], "json");
    if (!saved) return json({ status: "expired" });
    if (saved.status === "done") {
      const code = url.searchParams.get("code") || "";
      if (!code || code !== saved.handoff) {
        // Soju 1.13.1 and later send v=2 and wait for the code; older versions are told to update.
        return url.searchParams.get("v") === "2" ? json({ status: "pending" })
          : json({ status: "failed", error: "Update Soju to sign in: restart it and choose Update and Restart, or download the new version from didimlabs.com." });
      }
      await env.AUTH.delete("s:" + poll[1]);
      const { handoff, ...result } = saved;
      return json(result);
    }
    if (saved.status === "failed") await env.AUTH.delete("s:" + poll[1]);
    return json(saved);
  }
  return json({ error: "Not found." }, 404);
}
