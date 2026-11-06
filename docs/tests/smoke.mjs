import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const base=process.env.DOCS_URL||'http://127.0.0.1:4173';
const out=path.join(root,'test-results');await fs.mkdir(out,{recursive:true});
const options={headless:true,args:['--no-sandbox','--disable-dev-shm-usage']};
if(process.env.CHROMIUM_PATH)options.executablePath=process.env.CHROMIUM_PATH;
const browser=await chromium.launch(options);const page=await browser.newPage({viewport:{width:1440,height:1000}});
const errors=[];page.on('pageerror',e=>errors.push(e.message()));
let checks=0;function pass(name){checks++;console.log('PASS',name);}
try{
 await page.goto(base,{waitUntil:'networkidle'});assert.match(await page.title(),/Wraith/);assert.equal(await page.locator('.guide-card').count(),12);pass('Landing and 12 guide cards');
 await page.screenshot({path:path.join(out,'desktop.png'),fullPage:true});
 await page.locator('[data-mode="beginner"]').click();assert.equal(await page.locator('[data-mode="beginner"]').getAttribute('aria-pressed'),'true');
 await page.locator('#request-step').click();assert.match(await page.locator('#request-explanation').textContent(),/front door|server reads/);pass('Beginner mode and request stepping');
 await page.locator('#request-reset').click();await page.locator('#request-run').click();await page.waitForFunction(()=>document.getElementById('request-status').textContent.includes('Complete'),{timeout:15000});assert.match(await page.locator('#request-response').textContent(),/200 OK/);pass('Animated successful request');
 for(const [scenario,status] of [['capability','500'],['rate','429'],['pool','500']]){await page.selectOption('#request-scenario',scenario);await page.locator('#request-step').click();for(let i=0;i<8;i++){if(await page.locator('#request-step').isDisabled())break;await page.locator('#request-step').click();}assert.match(await page.locator('#request-response').textContent(),new RegExp(status));}pass('Capability, rate, and pool failure scenarios');
 await page.locator('#queue-run').click();await page.waitForFunction(()=>document.getElementById('queue-status').textContent.includes('Simulation complete'),{timeout:10000});assert.match(await page.locator('#queue-status').textContent(),/4 completed, 2 timed out/);pass('Default concurrency model: four complete, two timeout');
 await page.locator('#capacity').fill('4');await page.locator('#capacity').dispatchEvent('input');await page.locator('#queue-run').click();await page.waitForFunction(()=>document.getElementById('queue-status').textContent.includes('Simulation complete'),{timeout:10000});assert.match(await page.locator('#queue-status').textContent(),/6 completed, 0 timed out/);pass('Changing permits resets and changes outcome');
 await page.locator('#boot-stage').click();assert.equal(await page.locator('#boot-env dd').nth(1).textContent(),'3');
 for(let i=0;i<4;i++)await page.locator('#boot-reboot').click();
 assert.match(await page.locator('#boot-message').textContent(),/fell back.*A/);assert.match(await page.locator('#slot-a-state').textContent(),/ACTIVE/);pass('ARM three-trial exhaustion and next-boot LKG fallback');
 await page.locator('#boot-stage').click();await page.locator('#boot-reboot').click();await page.locator('#boot-commit').click();assert.match(await page.locator('#slot-b-state').textContent(),/GOOD/);assert.equal(await page.locator('#boot-env dd').nth(1).textContent(),'0');pass('Healthy trial commit updates LKG');
 const linear=await page.locator('#bar-exec').evaluate(el=>el.style.width);await page.locator('[data-scale="log"]').click();const log=await page.locator('#bar-exec').evaluate(el=>el.style.width);assert.notEqual(linear,log);assert.match(await page.locator('#scale-note').textContent(),/no longer proportional/);pass('Benchmark scale switch and scope warning');
 await page.keyboard.press('Control+k');await page.locator('#search-input').fill('signature persistence');assert.ok(await page.locator('.search-result').count()>0);await page.locator('.search-result').filter({hasText:'Security'}).first().click();await page.waitForURL(/book\/security.html/);assert.match(await page.locator('h1').textContent(),/Security/);pass('Full-text search navigates to a matching security section');
 await page.goto(base+'/book/source-map.html');assert.ok(await page.locator('.book-article table tbody tr').count()>=154);assert.ok(await page.locator('.copy-code').count()>0);pass('Complete source map and code-copy controls');
 for(const guide of ['quickstart','architecture','plugins','api','configuration','sdk-cli','operations','wraithos','security','benchmarks','testing']){const resp=await page.goto(base+'/book/'+guide+'.html');assert.equal(resp.status(),200);assert.ok((await page.locator('.book-article').textContent()).length>1000);}pass('All generated guide pages render');
 await page.goto(base);await page.setViewportSize({width:390,height:844});await page.waitForTimeout(200);const widths=await page.evaluate(()=>({scroll:document.documentElement.scrollWidth,client:document.documentElement.clientWidth}));assert.ok(widths.scroll<=widths.client+1,JSON.stringify(widths));await page.locator('.menu-toggle').click();assert.equal(await page.locator('.menu-toggle').getAttribute('aria-expanded'),'true');await page.screenshot({path:path.join(out,'mobile.png'),fullPage:true});pass('390px mobile layout and navigation');
 await page.emulateMedia({reducedMotion:'reduce'});assert.equal(await page.locator('.flow-line i').first().evaluate(el=>getComputedStyle(el).animationName),'none');pass('Reduced-motion preference');
 await page.goto(base+'/book/api.html');const broken=await page.locator('.book-article a[href]').evaluateAll(links=>links.map(a=>a.getAttribute('href')).filter(h=>h.startsWith('#')&&!document.getElementById(h.slice(1))));assert.deepEqual(broken,[]);pass('API guide heading links');
 assert.deepEqual(errors,[]);pass('No browser JavaScript errors');
 // The static reader must also work without the preview server.
 const offline=await browser.newPage();await offline.goto(pathToFileURL(path.join(root,'index.html')).href);await offline.keyboard.press('Control+k');await offline.locator('#search-input').fill('GRUB');assert.ok(await offline.locator('.search-result').count()>0);await offline.close();pass('Local-file search without a backend');
 console.log(`\n${checks} documentation browser checks passed.`);
}finally{await browser.close();}
