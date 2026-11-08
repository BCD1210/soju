Cloudflare Worker that serves the GitHub Pages site (docs/) at didimlabs.com, so the install command stays short:

    curl -fsSL didimlabs.com/install.sh | bash

It also answers POST /api/didim/download for the Didim Labs Store (needs the POLAR_TOKEN secret).

Two deployments use this code: `wrangler.didimlabs.toml` for didimlabs.com and `wrangler.toml` for the old
soju.snack-wrap.com address, which only redirects to didimlabs.com. They live in different Cloudflare accounts;
the maintainers' private ops notes say which login deploys which.
