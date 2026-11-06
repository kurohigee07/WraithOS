import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
let failures=0,checked=0;
const fail=(file,url,reason)=>{failures++;console.error(`FAIL ${file}: ${url} (${reason})`);};
const ids=file=>new Set([...fs.readFileSync(file,'utf8').matchAll(/\bid="([^"]+)"/g)].map(m=>m[1]));
for(const file of ['index.html',...fs.readdirSync(path.join(root,'book')).filter(f=>f.endsWith('.html')).map(f=>'book/'+f)]){
 const abs=path.join(root,file),text=fs.readFileSync(abs,'utf8');
 for(const m of text.matchAll(/(?:href|src)="([^"]+)"/g)){
  const url=m[1];if(/^(https?:|mailto:)/.test(url))continue;checked++;
  const [rel,anchor]=url.split('#');const target=rel?path.resolve(path.dirname(abs),rel):abs;
  if(!target.startsWith(root+path.sep)){fail(file,url,'outside docs');continue;}
  if(!fs.existsSync(target)){fail(file,url,'missing file');continue;}
  if(anchor&&target.endsWith('.html')&&!ids(target).has(anchor))fail(file,url,'missing anchor');
 }
}
// Markdown source links must also work on GitHub/local checkout.
const slug=s=>s.toLowerCase().replace(/<[^>]*>/g,'').replace(/[^a-z0-9\s-]/g,'').trim().replace(/\s+/g,'-');
function markdownIds(file){const seen=new Map(),out=new Set();for(const m of fs.readFileSync(file,'utf8').matchAll(/^#{1,6}\s+(.+)$/gm)){const base=slug(m[1]);const n=seen.get(base)||0;seen.set(base,n+1);out.add(base+(n?'-'+n:''));}return out;}
for(const file of ['../README.md',...fs.readdirSync(root).filter(f=>f.endsWith('.md')),'examples/empty-plugin/README.md']){
 const abs=path.resolve(root,file),text=fs.readFileSync(abs,'utf8');
 for(const m of text.matchAll(/\[[^\]]*\]\(([^)]+)\)/g)){
  const url=m[1];if(/^(https?:|mailto:)/.test(url))continue;checked++;
  const [rel,anchor]=url.split('#');const target=rel?path.resolve(path.dirname(abs),rel):abs;
  if(!fs.existsSync(target)){fail(file,url,'missing file');continue;}
  if(anchor&&target.endsWith('.md')&&!markdownIds(target).has(anchor))fail(file,url,'missing anchor');
 }
}
console.log(`Checked ${checked} local links/assets/anchors; ${failures} failures.`);
process.exitCode=failures?1:0;
