Cloudflare Worker that serves the GitHub Pages site (docs/) at soju.snack-wrap.com, so the install command stays short:

    curl -fsSL soju.snack-wrap.com/install.sh | bash

Deploy with `wrangler deploy` from this directory. The custom domain is created by the route entry in wrangler.toml.
