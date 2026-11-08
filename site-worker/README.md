Cloudflare Worker that serves the GitHub Pages site (docs/) at didimlabs.com, so the install command stays short:

    curl -fsSL didimlabs.com/install.sh | bash

didimlabs.com is in the fdmk.noreply Cloudflare account: deploy with

    HOME=~/.tagmatch-cloudflare npx wrangler@4.150.0 deploy -c wrangler.didimlabs.toml

The old soju.snack-wrap.com address (mikey9220 account, `wrangler deploy`) redirects every path to didimlabs.com.
