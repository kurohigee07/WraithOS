import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root=path.dirname(fileURLToPath(import.meta.url));
const port=Number(process.env.PORT||4173);
const types={'.html':'text/html; charset=utf-8','.css':'text/css; charset=utf-8','.js':'text/javascript; charset=utf-8','.json':'application/json; charset=utf-8','.md':'text/plain; charset=utf-8','.svg':'image/svg+xml','.woff2':'font/woff2','.txt':'text/plain; charset=utf-8'};
const server=http.createServer((req,res)=>{
 let pathname;try{pathname=decodeURIComponent(new URL(req.url,'http://docs.local').pathname);}catch{res.writeHead(400);res.end('Bad request');return;}
 if(req.method!=='GET'&&req.method!=='HEAD'){res.writeHead(405,{'Allow':'GET, HEAD'});res.end('Method not allowed');return;}
 const file=path.resolve(root,'.'+pathname);
 if(file!==root&&!file.startsWith(root+path.sep)||pathname.includes('/node_modules/')||pathname.split('/').some(x=>x.startsWith('.'))){res.writeHead(404);res.end('Not found');return;}
 let target=file;try{if(fs.statSync(target).isDirectory())target=path.join(target,'index.html');}catch{res.writeHead(404);res.end('Not found');return;}
 fs.stat(target,(err,stat)=>{if(err||!stat.isFile()){res.writeHead(404);res.end('Not found');return;}res.writeHead(200,{'Content-Type':types[path.extname(target)]||'application/octet-stream','Content-Length':stat.size,'X-Content-Type-Options':'nosniff','Cache-Control':'no-cache'});if(req.method==='HEAD')res.end();else fs.createReadStream(target).pipe(res);});
});
server.listen(port,'0.0.0.0',()=>console.log(`Wraith documentation: http://localhost:${port} (serving only ${root})`));
for(const sig of ['SIGINT','SIGTERM'])process.on(sig,()=>server.close(()=>process.exit(0)));
