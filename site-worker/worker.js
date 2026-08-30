// soju.snack-wrap.com: serves the GitHub Pages site (docs/) under a short domain.
//   curl -fsSL soju.snack-wrap.com/install.sh | bash
export default {
  async fetch(request) {
    const url = new URL(request.url);
    const upstream = new URL("https://bcd1210.github.io/soju" + url.pathname + url.search);
    const res = await fetch(upstream, { headers: { "User-Agent": "soju-site" }, cf: { cacheTtl: 300 } });
    const headers = new Headers(res.headers);
    headers.delete("content-security-policy");
    return new Response(res.body, { status: res.status, headers });
  },
};
