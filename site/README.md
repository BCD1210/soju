Builds the didimlabs.com pages (docs/index.html, ko/, games/, windows.html, support.html, 404.html) from
content/*.json with [json-render](https://github.com/vercel-labs/json-render). The output is static HTML with no
client-side JavaScript, so search engines and AI crawlers read the full page. Guides under docs/guides/ are still
hand-written.

    cd site && npm install && npm run build

Edit text in content/*.json, run the build, and commit both the JSON and the generated docs/ files.
Components and their props live in catalog.mjs; FAQ structured data is generated from Faq blocks.
Strings accept [label](url), **bold** and `code`.
