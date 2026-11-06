import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Marked, Parser } from 'marked';

const root=path.dirname(fileURLToPath(import.meta.url));
const guides=[
 ['quickstart','Quick start'],['architecture','Architecture'],['plugins','Plugin development'],['api','HTTP API'],['configuration','Configuration'],['sdk-cli','SDK & CLI'],['operations','Operations'],['wraithos','WraithOS'],['security','Security boundaries'],['benchmarks','Benchmarks'],['testing','Testing & contributing'],['source-map','Source map']
];
const esc=s=>String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const slug=s=>s.toLowerCase().replace(/<[^>]*>/g,'').replace(/[^a-z0-9\s-]/g,'').trim().replace(/\s+/g,'-');
const strip=s=>s.replace(/<[^>]*>/g,' ').replace(/&[^;]+;/g,' ').replace(/\s+/g,' ').trim();
const search=[];
await fs.mkdir(path.join(root,'book'),{recursive:true});
for(const [id,title] of guides){
 const source=await fs.readFile(path.join(root,id+'.md'),'utf8');
 const headings=[];const seen=new Map();let pageGuide=title;
 const marked=new Marked({gfm:true});
 marked.use({renderer:{
  heading({tokens,depth}){const text=this.parser.parseInline(tokens);const base=slug(text);const n=seen.get(base)||0;seen.set(base,n+1);const anchor=base+(n?'-'+n:'');if(depth===1)pageGuide=strip(text);if(depth===2||depth===3)headings.push({text:strip(text),anchor,depth});return `<h${depth} id="${anchor}">${text}${depth>1?` <a class="heading-anchor" href="#${anchor}" aria-label="Link to ${esc(strip(text))}">#</a>`:''}</h${depth}>`;},
  code({text,lang}){if(lang==='mermaid'&&(id==='architecture'||id==='wraithos')){const asset=id==='architecture'?'architecture-flow.svg':'boot-flow.svg';return `<figure class="diagram"><img src="../assets/${asset}" alt="${id==='architecture'?'Wraith request, host-import, event, and observability flow':'WraithOS firmware-to-host boot and persistence flow'}" width="1120" height="${id==='architecture'?650:470}"><figcaption>Source-guided overview. The surrounding text explains exact behavior and limitations.</figcaption></figure>`;}return `<div class="code-block"><button class="copy-code" aria-label="Copy code">Copy</button><pre><code class="language-${esc(lang||'text')}">${esc(text)}</code></pre></div>`;},

  link({href,title:linkTitle,tokens}){const text=this.parser.parseInline(tokens);let url=href;if(!/^(https?:|mailto:|#)/.test(url)){if(url.endsWith('.md')||url.includes('.md#')){url=url.replace('.md','.html');if(url.startsWith('examples/'))url='../'+url;}else if(!url.startsWith('../'))url='../'+url;}return `<a href="${esc(url)}"${linkTitle?` title="${esc(linkTitle)}"`:''}${/^https?:/.test(url)?' rel="noopener noreferrer"':''}>${text}</a>`;}
 }});
 const content=marked.parse(source).replace(/<table>/g,'<div class="table-scroll"><table>').replace(/<\/table>/g,'</table></div>');
 // Search sections include the prose under each heading, not only its title.
 const sections=content.split(/(?=<h[123] id=)/);let current={title:pageGuide,anchor:'',level:1};let buffer='';
 const push=()=>{const text=strip(buffer);if(text)search.push({guide:pageGuide,title:current.title,url:`book/${id}.html${current.anchor?'#'+current.anchor:''}`,text:text.slice(0,6500),level:current.level});buffer='';};
 for(const section of sections){const match=section.match(/^<h([123]) id="([^"]+)">([\s\S]*?)<\/h[123]>/);if(match){push();current={title:strip(match[3]).replace(/#$/,'').trim(),anchor:match[2],level:+match[1]};buffer=section.slice(match[0].length);}else buffer+=section;}push();
 const nav=guides.map(([key,label])=>`<a href="${key}.html" ${key===id?'class="active" aria-current="page"':''}>${label}</a>`).join('');
 const mobileNav=`<button class="menu-toggle" aria-label="Open navigation" aria-expanded="false">☰</button>`;
 const mobileLinks=`<nav class="mobile-nav" aria-label="Mobile guide navigation" hidden><a href="../index.html">Interactive overview</a>${nav}</nav>`;
 const toc=headings.filter(h=>h.depth===2).map(h=>`<a href="#${h.anchor}">${esc(h.text)}</a>`).join('');
 const at=guides.findIndex(g=>g[0]===id);const prev=guides[at-1],next=guides[at+1];
 const minutes=Math.max(2,Math.ceil(source.split(/\s+/).length/230));
 const description=strip(content.replace(/<figure class="diagram">[\s\S]*?<\/figure>/g,'').replace(/<div class="code-block">[\s\S]*?<\/div>/g,'')).slice(0,180);
 const html=`<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="theme-color" content="#0c0b12"><meta name="description" content="${esc(description)}"><title>${esc(title)} — Wraith</title><link rel="icon" href="../assets/favicon.svg" type="image/svg+xml"><link rel="stylesheet" href="../assets/style.css"><script defer src="../assets/search-index.js"></script><script defer src="../assets/app.js"></script></head><body data-base="../"><a class="skip-link" href="#article">Skip to article</a><header class="site-header"><div class="header-inner"><a class="brand" href="../index.html"><img src="../assets/favicon.svg" alt="" width="32" height="32"><span>wraith<span class="brand-dot">.</span></span><span class="brand-tag">DOCUMENTATION</span></a><nav class="desktop-nav" aria-label="Main navigation"><a href="../index.html#runtime">Runtime</a><a href="../index.html#request-lab">Labs</a><a href="../index.html#benchmarks">Benchmarks</a><a href="quickstart.html">Guides</a><a href="source-map.html">Source map</a></nav><div class="header-actions"><button class="search-trigger" data-search-open aria-label="Search documentation"><span>Search docs</span><kbd>⌘ K</kbd></button>${mobileNav}</div></div>${mobileLinks}<div class="reading-progress" aria-hidden="true"></div></header><div class="book-layout"><aside class="book-sidebar" aria-label="Documentation guides"><a class="back-home" href="../index.html">← Interactive overview</a><span class="eyebrow">THE DOCUMENTATION</span>${nav}</aside><main class="book-article" id="article"><div class="article-meta"><span class="pill violet">SOURCE-GUIDED</span><span>${minutes} min read</span><a href="../${id}.md">Markdown source</a></div>${content}<div class="book-bottom">${prev?`<a href="${prev[0]}.html">← ${prev[1]}</a>`:'<a href="../index.html">← Overview</a>'}${next?`<a href="${next[0]}.html">${next[1]} →</a>`:'<a href="../index.html">Interactive overview →</a>'}</div></main><aside class="book-toc" aria-label="On this page"><span class="eyebrow">ON THIS PAGE</span>${toc}</aside></div><dialog id="search-dialog" aria-label="Search documentation"><div class="search-box"><div class="search-input-row"><span>⌕</span><input id="search-input" type="search" placeholder="Search routes, boot slots, quotas…" autocomplete="off" aria-label="Search documentation"><button data-search-close aria-label="Close search">Esc</button></div><div id="search-results" class="search-results"></div><div class="search-footer">↑ ↓ to navigate · Enter to open · Esc to close</div></div></dialog></body></html>`;
 await fs.writeFile(path.join(root,'book',id+'.html'),html);
}
const landing=await fs.readFile(path.join(root,'templates','index.html'),'utf8');
await fs.writeFile(path.join(root,'index.html'),landing);
await fs.writeFile(path.join(root,'assets','search-index.js'),'window.WRAITH_SEARCH='+JSON.stringify(search).replace(/</g,'\\u003c')+';\n');
console.log(`Built ${guides.length} English guides, ${search.length} search sections, and the interactive overview.`);
