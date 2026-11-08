// Renders content/*.json into ../docs/*.html with json-render. Output is plain static HTML (no client JS),
// so search engines and AI crawlers read the full text. FAQ structured data is generated from Faq blocks.
//   cd site && npm install && npm run build
import { createElement as h } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { JSONUIProvider, Renderer } from '@json-render/react';
import { readdirSync, readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { components, registry } from './catalog.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const docs = join(here, '..', 'docs');
const CSS_VERSION = '20261109c';
const esc = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
const plain = (s) => s.replace(/\[([^\]]+)\]\([^)]+\)/g, '$1').replace(/\*\*([^*]+)\*\*/g, '$1').replace(/`([^`]+)`/g, '$1');

// Nested content tree -> json-render flat spec. Props are checked against the catalog's schemas.
function toSpec(nodes, page) {
  const elements = {};
  let n = 0;
  const add = (node) => {
    const def = components[node.type];
    if (!def) throw new Error(page + ': unknown component ' + node.type);
    const checked = def.props.safeParse(node.props || {});
    if (!checked.success) throw new Error(page + ': bad props for ' + node.type + ': ' + checked.error.message);
    const key = node.type.toLowerCase() + '-' + n++;
    elements[key] = { type: node.type, props: checked.data, children: (node.children || []).map(add) };
    return key;
  };
  const root = add({ type: 'Stack', props: {}, children: nodes });
  return { root, elements };
}

function faqItems(nodes, found = []) {
  for (const node of nodes) {
    if (node.type === 'Faq') found.push(...node.props.items);
    if (node.children) faqItems(node.children, found);
  }
  return found;
}

function page(file) {
  const p = JSON.parse(readFileSync(join(here, 'content', file), 'utf8'));
  const body = renderToStaticMarkup(h(JSONUIProvider, { registry, initialState: {} }, h(Renderer, { spec: toSpec(p.body, file), registry })));
  const faq = faqItems(p.body);
  const graph = [...(p.jsonld || [])];
  if (faq.length) graph.push({ '@type': 'FAQPage', inLanguage: p.lang, mainEntity: faq.map((f) => ({
    '@type': 'Question', name: f.q, acceptedAnswer: { '@type': 'Answer', text: plain(f.a) } })) });
  const b = p.base || '';
  const head = [
    '<meta charset="utf-8">',
    '<meta name="viewport" content="width=device-width, initial-scale=1">',
    `<title>${esc(p.title)}</title>`,
    `<meta name="description" content="${esc(p.description)}">`,
    p.noindex ? '<meta name="robots" content="noindex">' : `<link rel="canonical" href="${esc(p.canonical)}">`,
    ...(p.alternates || []).map((a) => `<link rel="alternate" hreflang="${esc(a.hreflang)}" href="${esc(a.href)}">`),
    '<meta property="og:site_name" content="Didim Labs">',
    `<meta property="og:title" content="${esc(p.og?.title || p.title)}">`,
    `<meta property="og:description" content="${esc(p.og?.description || p.description)}">`,
    '<meta property="og:type" content="website">',
    p.canonical && `<meta property="og:url" content="${esc(p.canonical)}">`,
    p.og?.image && `<meta property="og:image" content="${esc(p.og.image)}">`,
    p.og?.imageAlt && `<meta property="og:image:alt" content="${esc(p.og.imageAlt)}">`,
    p.lang === 'ko' && '<meta property="og:locale" content="ko_KR">',
    '<meta name="twitter:card" content="summary_large_image">',
    '<meta name="theme-color" content="#121413">',
    `<link rel="icon" href="${b}assets/favicon.png">`,
    `<link rel="apple-touch-icon" href="${b}assets/apple-touch-icon.png">`,
    `<link rel="stylesheet" href="${b}style.css?v=${CSS_VERSION}">`,
    graph.length && `<script type="application/ld+json">\n${JSON.stringify({ '@context': 'https://schema.org', '@graph': graph }, null, 1).replace(/</g, '\\u003c')}\n</script>`,
  ].filter(Boolean).join('\n');
  const nav = (p.nav || []).map((l) => `<a href="${esc(l.href)}"${l.lang ? ` lang="${l.lang}" hreflang="${l.lang}"` : ''}${l.current ? ' aria-current="page"' : ''}>${esc(l.label)}</a>`).join('\n  ');
  const footer = (p.footer || []).map((line) => `<p>${renderToStaticMarkup(h('span', null, ...inlineNodes(line)))}</p>`).join('\n');
  const html = `<!DOCTYPE html>
<html lang="${p.lang}">
<head>
${head}
</head>
<body>
<a class="skip" href="#main">${p.lang === 'ko' ? '본문으로' : 'Skip to content'}</a>
<nav aria-label="${p.lang === 'ko' ? '메인' : 'Main'}"><div class="inner">
  <a class="brand" href="${b || './'}"><img src="${b}assets/soju-logo-64.png" width="28" height="28" alt="">Soju<span> by Didim Labs</span></a>
  ${nav}
</div></nav>
<main id="main">
${body}
</main>
<footer><div class="inner">
${footer}
</div></footer>
</body>
</html>
`;
  const out = join(docs, p.out);
  mkdirSync(dirname(out), { recursive: true });
  // Version local asset links so browsers pick up regenerated images.
  writeFileSync(out, html.replace(/((?:src|poster|href)="[^"?:]*assets\/[^"?]+\.(?:png|jpg|mp4))"/g, `$1?v=${CSS_VERSION}"`));
  return p.out;
}

import { inline as inlineNodes } from './catalog.mjs';
const built = readdirSync(join(here, 'content')).filter((f) => f.endsWith('.json')).sort().map(page);
console.log('built', built.join(', '));
