// soju-stats: the anonymous play counts (stats.js) on its own workers.dev address, so it does not
// depend on the account that serves didimlabs.com. Deploy: wrangler deploy -c wrangler.stats.toml
import { stats } from "./stats.js";

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname === "/api/stats" || url.pathname === "/api/stats/summary") return stats(request, env, url);
    return new Response("Soju play counts: https://didimlabs.com/privacy.html\n", { status: 404 });
  },
};
