// Components for didimlabs.com. Content lives in content/*.json; build.mjs renders it to static HTML.
// Strings support a small inline syntax: [label](url), **bold**, `code`.
import { createElement as h, Fragment } from 'react';
import { defineCatalog } from '@json-render/core';
import { schema } from '@json-render/react/schema';
import { defineRegistry } from '@json-render/react';
import { z } from 'zod';

const str = z.string();
const opt = (t) => t.optional();
const link = z.object({ label: str, href: str, primary: opt(z.boolean()), lang: opt(str) });
const image = z.object({ src: str, alt: str, width: opt(z.number()), height: opt(z.number()) });

export const components = {
    Stack: { props: z.object({}), description: 'Groups children with no markup of its own' },
    Hero: { props: z.object({ title: str, lead: str, logo: opt(str), actions: opt(z.array(link)), meta: opt(str), image: opt(image) }), description: 'Page heading' },
    Section: { props: z.object({ id: opt(str), title: str, intro: opt(str) }), description: 'Titled section' },
    Text: { props: z.object({ text: str }), description: 'Paragraph' },
    Bullets: { props: z.object({ items: z.array(str) }), description: 'Bulleted list' },
    Features: { props: z.object({ items: z.array(z.object({ title: str, text: str })) }), description: 'Grid of short facts' },
    Steps: { props: z.object({ items: z.array(z.object({ title: str, text: opt(str), code: opt(str) })) }), description: 'Numbered steps' },
    Table: { props: z.object({ label: str, head: z.array(str), rows: z.array(z.array(str)) }), description: 'Data table' },
    Code: { props: z.object({ code: str }), description: 'Code block' },
    Note: { props: z.object({ text: str }), description: 'Side note' },
    Buttons: { props: z.object({ items: z.array(link) }), description: 'Row of links styled as buttons' },
    GameCard: { props: z.object({ href: str, image, title: str, meta: str, text: str }), description: 'Game teaser' },
    Gallery: { props: z.object({ images: z.array(image) }), description: 'Screenshot grid' },
    Video: { props: z.object({ src: str, poster: str, captions: opt(str), label: str, caption: opt(str) }), description: 'Video' },
    Faq: { props: z.object({ items: z.array(z.object({ q: str, a: str })) }), description: 'Questions and answers' },
    Links: { props: z.object({ items: z.array(link) }), description: 'List of links' },
    Figure: { props: z.object({ image, caption: opt(str) }), description: 'Single image with caption' },
    ContactForm: {
      props: z.object({
        action: str, name: str, email: str, company: str, message: str, send: str,
        sending: str, sent: str, failed: str, thanks: opt(str),
      }),
      description: 'Inquiry form posted to the site worker',
    },
};

export const catalog = defineCatalog(schema, { components, actions: {} });

// [label](url), **bold**, `code` -> React nodes. Everything else is plain text (React escapes it).
export function inline(text) {
  const out = [];
  const re = /\[([^\]]+)\]\(([^)\s]+)\)|\*\*([^*]+)\*\*|`([^`]+)`/g;
  let at = 0, m, k = 0;
  while ((m = re.exec(text))) {
    if (m.index > at) out.push(text.slice(at, m.index));
    if (m[1] !== undefined) out.push(h('a', { key: k++, href: m[2] }, m[1]));
    else if (m[3] !== undefined) out.push(h('strong', { key: k++ }, m[3]));
    else out.push(h('code', { key: k++ }, m[4]));
    at = re.lastIndex;
  }
  if (at < text.length) out.push(text.slice(at));
  return out;
}

const img = (i, extra = {}) => h('img', { src: i.src, alt: i.alt, width: i.width, height: i.height, ...extra });
const button = (l, k) => h('a', { key: k, className: l.primary ? 'btn' : 'btn secondary', href: l.href, lang: l.lang }, l.label);

export const { registry } = defineRegistry(catalog, {
  components: {
    Stack: ({ children }) => h(Fragment, null, children),
    Hero: ({ props }) => h('header', { className: 'hero' },
      props.logo ? h('h1', { className: 'with-logo' }, h('img', { src: props.logo, width: 72, height: 72, alt: '' }), props.title) : h('h1', null, props.title),
      h('p', { className: 'lead' }, inline(props.lead)),
      props.actions && h('p', { className: 'actions' }, props.actions.map(button), props.meta && h('span', { className: 'meta' }, props.meta)),
      props.image && img(props.image, { className: 'shot', fetchPriority: 'high' })),
    Section: ({ props, children }) => h('section', { id: props.id, 'aria-labelledby': props.id && props.id + '-title' },
      h('h2', { id: props.id && props.id + '-title' }, props.title),
      props.intro && h('p', { className: 'intro' }, inline(props.intro)),
      children),
    Text: ({ props }) => h('p', null, inline(props.text)),
    Bullets: ({ props }) => h('ul', null, props.items.map((t, k) => h('li', { key: k }, inline(t)))),
    Features: ({ props }) => h('div', { className: 'features' }, props.items.map((f, k) =>
      h('div', { key: k, className: 'feature' }, h('h3', null, f.title), h('p', null, inline(f.text))))),
    Steps: ({ props }) => h('ol', { className: 'steps' }, props.items.map((s, k) =>
      h('li', { key: k }, h('b', null, s.title), s.text && h('p', null, inline(s.text)), s.code && h('pre', null, h('code', null, s.code))))),
    Table: ({ props }) => h('div', { className: 'table-scroll', role: 'region', 'aria-label': props.label, tabIndex: 0 },
      h('table', null,
        h('thead', null, h('tr', null, props.head.map((c, k) => h('th', { key: k, scope: 'col' }, c)))),
        h('tbody', null, props.rows.map((r, i) => h('tr', { key: i }, r.map((c, k) => k === 0
          ? h('th', { key: k, scope: 'row' }, inline(c)) : h('td', { key: k }, inline(c)))))))),
    Code: ({ props }) => h('pre', null, h('code', null, props.code)),
    Note: ({ props }) => h('p', { className: 'note' }, inline(props.text)),
    Buttons: ({ props }) => h('p', { className: 'actions' }, props.items.map(button)),
    GameCard: ({ props }) => h('a', { className: 'game', href: props.href },
      img(props.image, { loading: 'lazy' }),
      h('span', { className: 'game-body' }, h('b', null, props.title), h('span', { className: 'game-meta' }, props.meta), h('span', null, props.text))),
    Gallery: ({ props }) => h('div', { className: 'gallery' }, props.images.map((i, k) => img(i, { key: k, loading: 'lazy' }))),
    Video: ({ props }) => h('figure', { className: 'video' },
      h('video', { controls: true, playsInline: true, preload: 'none', poster: props.poster, 'aria-label': props.label, width: 1280, height: 720 },
        h('source', { src: props.src, type: 'video/mp4' }),
        props.captions && h('track', { kind: 'captions', src: props.captions, srcLang: 'en', label: 'English', default: true })),
      props.caption && h('figcaption', null, props.caption)),
    Faq: ({ props }) => h('div', { className: 'faq' }, props.items.map((f, k) =>
      h('details', { key: k, open: k === 0 }, h('summary', null, f.q), h('p', null, inline(f.a))))),
    Links: ({ props }) => h('ul', { className: 'links' }, props.items.map((l, k) =>
      h('li', { key: k }, h('a', { href: l.href, lang: l.lang }, l.label)))),
    Figure: ({ props }) => h('figure', { className: 'figure' }, img(props.image, { loading: 'lazy' }),
      props.caption && h('figcaption', null, props.caption)),
    ContactForm: ({ props }) => h(Fragment, null,
      props.thanks && h('p', { className: 'note thanks', id: 'thanks', hidden: true }, props.thanks),
      h('form', { className: 'contact', method: 'post', action: props.action, id: 'contact' },
        h('label', null, props.name, h('input', { name: 'name', required: true, maxLength: 100, autoComplete: 'name' })),
        h('label', null, props.email, h('input', { name: 'email', type: 'email', required: true, maxLength: 200, autoComplete: 'email' })),
        h('label', null, props.company, h('input', { name: 'company', maxLength: 120, autoComplete: 'organization' })),
        h('label', null, props.message, h('textarea', { name: 'message', required: true, minLength: 5, maxLength: 4000, rows: 5 })),
        h('input', { name: 'website', tabIndex: -1, autoComplete: 'off', className: 'hp', 'aria-hidden': 'true' }),
        h('p', { className: 'actions' }, h('button', { className: 'btn', type: 'submit' }, props.send),
          h('span', { className: 'form-status', role: 'status', 'aria-live': 'polite' }))),
      h('script', { dangerouslySetInnerHTML: { __html: `(function(){
var f=document.getElementById('contact'),s=f.querySelector('.form-status'),b=f.querySelector('button');
if(/[?&]thanks=1/.test(location.search)){var t=document.getElementById('thanks');if(t)t.hidden=false;}
f.addEventListener('submit',function(e){e.preventDefault();b.disabled=true;s.textContent=${JSON.stringify(props.sending)};
fetch(f.action,{method:'POST',body:new FormData(f)}).then(function(r){return r.json().then(function(j){return {ok:r.ok,j:j};});})
.then(function(x){if(x.ok){f.reset();s.textContent=${JSON.stringify(props.sent)};}else{s.textContent=(x.j&&x.j.error)||${JSON.stringify(props.failed)};b.disabled=false;}})
.catch(function(){s.textContent=${JSON.stringify(props.failed)};b.disabled=false;});});})();` } })),
  },
});
