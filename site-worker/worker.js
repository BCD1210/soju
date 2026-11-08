// didimlabs.com: serves the GitHub Pages site (docs/) under a short domain.
//   curl -fsSL didimlabs.com/install.sh | bash
// The old soju.snack-wrap.com address (and www) redirect here so existing links and install commands keep working.
//
// POST /api/didim/download {"game": "<id>", "key": "<license key>"} is how Soju turns a Didim Labs
// license key (sold through Polar) into a short-lived download link. Needs the POLAR_TOKEN secret
// (organization token with license_keys:read and customer_sessions:write) on the didimlabs.com worker.
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

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.hostname !== HOME) {
      return Response.redirect("https://" + HOME + url.pathname + url.search, 301);
    }
    if (url.pathname === "/api/didim/download") return download(request, env);
    const upstream = new URL("https://bcd1210.github.io/soju" + url.pathname + url.search);
    const res = await fetch(upstream, { headers: { "User-Agent": "soju-site" }, cf: { cacheTtl: 300 } });
    const headers = new Headers(res.headers);
    headers.delete("content-security-policy");
    return new Response(res.body, { status: res.status, headers });
  },
};
