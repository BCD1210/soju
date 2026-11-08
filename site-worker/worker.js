// didimlabs.com: serves the GitHub Pages site (docs/) under a short domain.
//   curl -fsSL didimlabs.com/install.sh | bash
// The old soju.snack-wrap.com address (and www) redirect here so existing links and install commands keep working.
const HOME = "didimlabs.com";
export default {
  async fetch(request) {
    const url = new URL(request.url);
    if (url.hostname !== HOME) {
      return Response.redirect("https://" + HOME + url.pathname + url.search, 301);
    }
    const upstream = new URL("https://bcd1210.github.io/soju" + url.pathname + url.search);
    const res = await fetch(upstream, { headers: { "User-Agent": "soju-site" }, cf: { cacheTtl: 300 } });
    const headers = new Headers(res.headers);
    headers.delete("content-security-policy");
    return new Response(res.body, { status: res.status, headers });
  },
};
