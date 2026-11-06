/* Teaching simulations only. No network calls, runtime execution, or disk writes. */
(() => {
  'use strict';
  const $ = id => document.getElementById(id);
  if (!$('request-track')) return;
  let mode = 'engineer';
  try { mode = localStorage.getItem('wraith-doc-mode') || mode; } catch {}
  const modes = document.querySelectorAll('[data-mode]');
  function setMode(value) {
    mode = value === 'beginner' ? 'beginner' : 'engineer';
    modes.forEach(b => { const active = b.dataset.mode === mode; b.classList.toggle('active', active); b.setAttribute('aria-pressed', String(active)); });
    try { localStorage.setItem('wraith-doc-mode', mode); } catch {}
    if (requestIndex >= 0) renderRequest();
  }
  modes.forEach(b => b.addEventListener('click', () => setMode(b.dataset.mode)));

  const stages = [
    { name: 'HTTP + auth', key: 'router', title: 'The front door checks the request.', beginner: 'The server reads the HTTP request. If authentication is enabled, it checks the token before allowing module operations.', engineer: 'Axum extracts the JSON body. Optional global middleware verifies the default tenant token; the module handler also verifies its tenant. Health and readiness are exempt.', link: 'auth' },
    { name: 'Find module', key: 'registry', title: 'A name becomes a component.', beginner: 'Wraith looks up the module using both its tenant and module ID. A missing module returns 404.', engineer: 'ModuleRegistry.get((tenant, id)) clones an Arc<WasmModule>. The compiled Component and Linker are reused; no guest instance is reused.', link: 'lookup' },
    { name: 'Tenant gates', key: 'tenant', title: 'The tenant gets a budget.', beginner: 'A tenant can have a request-rate budget and a limit on how many calls run together. This example has a rate limit only in the rate-limit scenario.', engineer: 'try_acquire_rate consumes a tenant token, then acquire_concurrency awaits an optional permit. The permit is held by RAII until the handler completes.', link: 'tenant' },
    { name: 'Module gates', key: 'module', title: 'The module gets its own budget.', beginner: 'An individual module can be slower or more restricted than the rest of the tenant. Zero quota fields leave this extra gate open.', engineer: 'execute_quota applies the module token bucket and optional semaphore before entering the global pool. Early rejections update metrics but do not create audit rows.', link: 'module' },
    { name: 'Global pool', key: 'pool', title: 'A permit is not an instance.', beginner: 'Only a limited number of guests may run at once. If no place becomes free before the wait budget ends, the request fails.', engineer: 'WasmPool acquire is timed separately and recorded in wasm_pool_wait_seconds. Timeout currently maps to HTTP 500 through QuotaExecError::Internal.', link: 'pool' },
    { name: 'Fresh Store', key: 'store', title: 'Build a fresh execution context.', beginner: 'Wraith creates a new memory space for this call and sets its limits. It does not keep the previous guest memory alive.', engineer: 'Store<WasmHostState> gets WASI state, StoreLimits, and a 600-tick epoch deadline. instantiate_async links the compiled component to host imports.', link: 'store' },
    { name: 'Guest run', key: 'guest', title: 'Cross the WIT boundary.', beginner: 'The guest receives JSON as text and returns JSON as text. Host requests such as HTTP must pass permission checks.', engineer: 'call_run invokes the exported component function. The empty guest returns Ok("{}"). In the denied scenario, a host import returns Err and this model assumes the guest propagates it.', link: 'guest' },
    { name: 'Return + record', key: 'result', title: 'Make the execution observable.', beginner: 'Wraith checks that the returned text is JSON, sends the response, and records timing and status for inspection.', engineer: 'The result is parsed before the execution timer stops. The host then writes an integer-ms audit row and records the duration histogram; audit insert errors are currently ignored.', link: 'record' }
  ];
  let requestIndex = -1, requestTimer = null, requestScenario = 'success';
  $('request-track').innerHTML = stages.map((s,i) => `<div class="request-node" data-step="${i}"><small>0${i+1}</small><b>${s.name}</b></div>`).join('');
  const nodes = [...document.querySelectorAll('.request-node')];
  function failureAt() { return requestScenario === 'rate' ? 2 : requestScenario === 'pool' ? 4 : requestScenario === 'capability' ? 6 : -1; }
  function stopRequest() { clearInterval(requestTimer); requestTimer = null; $('request-run').textContent = '▶ Run request'; }
  function renderRequest() {
    const fail = failureAt();
    const finished = requestIndex === stages.length - 1 || (fail >= 0 && requestIndex >= fail);
    const failed = fail >= 0 && requestIndex >= fail;
    nodes.forEach((n,i) => { n.className = 'request-node' + (i < requestIndex ? ' done' : '') + (i === requestIndex ? (failed ? ' error' : ' active') : ''); });
    if (requestIndex < 0) return;
    const s = stages[requestIndex];
    $('request-step-label').textContent = `STEP ${requestIndex+1} / ${stages.length}${failed ? ' · REJECTED' : finished ? ' · COMPLETE' : ''}`;
    $('request-title').textContent = s.title;
    let explanation = s[mode];
    if (failed) explanation += requestScenario === 'capability' ? ' The allowlist rejects this host. A guest may propagate the error (500 here) or catch it and return a successful report.' : requestScenario === 'pool' ? ' No global permit arrived before the timeout. This is capacity backpressure, not a successful guest execution.' : ' The tenant token bucket is empty. The client receives 429 with a retry hint.';
    $('request-explanation').textContent = explanation;
    $('request-source').href = `book/architecture.html#${s.link}`;
    const responses = {
      rate: 'HTTP/1.1 429 Too Many Requests\nRetry-After: 1\n\n{"error":"rate limited","retry_after_ms":250}',
      pool: 'HTTP/1.1 500 Internal Server Error\n\n{"error":"execute: pool: … acquire timeout"}',
      capability: 'HTTP/1.1 500 Internal Server Error\n\n{"error":"execute: guest error: capability denied: http host …"}'
    };
    $('request-response').textContent = failed ? responses[requestScenario] : finished ? 'HTTP/1.1 200 OK\nContent-Type: application/json\n\n{}\n\nSimulation complete. No latency claimed.' : `→ ${s.key}\n${requestScenario === 'success' ? 'Scenario: empty guest; no I/O' : 'Scenario: ' + requestScenario}\nAwaiting next transition…`;
    $('request-status').textContent = failed ? `Stopped at ${s.name}. See the response panel for the modeled status.` : finished ? 'Complete. Eight source-guided stages; zero real requests sent.' : `Step ${requestIndex+1}: ${s.name}. ${requestTimer ? 'Playing.' : 'Paused.'}`;
    if (finished || failed) stopRequest();
    $('request-step').disabled = finished || failed;
  }
  function stepRequest() { if (requestIndex < stages.length-1) { requestIndex++; renderRequest(); } }
  function resetRequest() { stopRequest(); requestIndex = -1; requestScenario = $('request-scenario').value; nodes.forEach(n => n.className='request-node'); $('request-step-label').textContent='READY TO BEGIN'; $('request-title').textContent='A request is waiting at the door.'; $('request-explanation').textContent='Press Run to follow the gates, or Step to inspect one transition at a time.'; $('request-response').textContent='Waiting for a simulated request…'; $('request-status').textContent='Ready. Choose a scenario and run or step.'; $('request-step').disabled=false; $('request-source').href='book/architecture.html'; }
  $('request-reset').addEventListener('click', resetRequest);
  $('request-scenario').addEventListener('change', resetRequest);
  $('request-step').addEventListener('click', () => { stopRequest(); stepRequest(); renderRequest(); });
  $('request-run').addEventListener('click', () => { if (requestTimer) { stopRequest(); renderRequest(); return; } const fail=failureAt(); if (requestIndex >= stages.length-1 || (fail>=0 && requestIndex>=fail)) resetRequest(); $('request-run').textContent='Ⅱ Pause'; stepRequest(); if (!$('request-step').disabled) requestTimer=setInterval(stepRequest,1100); });
  setMode(mode); resetRequest();

  // A discrete-event FIFO model. All arrivals at t=0; no claim about Tokio fairness.
  let queueModel = [], queueTime = 0, queueFrame = null, queueLast = 0, queuePlaying = false, queueEnd = 0;
  const queueInputs = ['capacity','arrivals','service','wait'];
  function readQueue() { return { capacity:+$('capacity').value, arrivals:+$('arrivals').value, service:+$('service').value, wait:+$('wait').value }; }
  function buildQueue() {
    const q=readQueue();
    $('capacity-out').textContent=q.capacity; $('arrivals-out').textContent=q.arrivals; $('service-out').textContent=q.service.toFixed(1)+' s'; $('wait-out').textContent=q.wait.toFixed(1)+' s';
    let available=0;
    queueModel=Array.from({length:q.arrivals},(_,i)=>{ const start=available<q.capacity ? 0 : Math.ceil((available-q.capacity+1)/q.capacity)*q.service; available++; return {id:i+1,start:Math.round(start*1000)/1000,end:Math.round((start+q.service)*1000)/1000,status:start>=q.wait?'timeout':'waiting'}; });
    queueEnd=Math.max(q.wait,...queueModel.filter(r=>r.status!=='timeout').map(r=>r.end),q.service);
    queueTime=0; renderQueue();
  }
  function stopQueue() { if(queueFrame) cancelAnimationFrame(queueFrame); queueFrame=null; queuePlaying=false; $('queue-run').textContent='▶ Simulate'; }
  function resetQueue() { stopQueue(); buildQueue(); $('queue-status').textContent='All requests arrive at t=0. This model uses FIFO permit assignment and ignores HTTP/module/tenant overhead.'; }
  function renderQueue() {
    const q=readQueue(); const running=[], counts={waiting:0,running:0,done:0,timeout:0};
    queueModel.forEach(r=>{ if(queueTime>=r.start && r.start<q.wait){ if(queueTime>=r.end)r.status='done'; else {r.status='running';running.push(r);} } else if(r.start>=q.wait && queueTime>=q.wait)r.status='timeout'; else r.status='waiting'; counts[r.status]++; });
    $('queue-clock').textContent=`t = ${queueTime.toFixed(1)} s`;
    $('queue-counts').textContent=`${counts.waiting} waiting · ${counts.running} running · ${counts.done} completed · ${counts.timeout} timed out`;
    $('queue-slots').innerHTML=Array.from({length:q.capacity},(_,i)=>`<div class="queue-slot ${running[i]?'occupied':''}"><small>PERMIT ${i+1}</small>${running[i]?'Request #'+running[i].id+' running':'Available'}</div>`).join('');
    $('queue-rows').innerHTML=queueModel.map(r=>`<div class="queue-row ${r.status}"><i></i>Request #${r.id}<span>${r.status}${r.status==='done'?' @ '+r.end.toFixed(1)+'s':r.status==='timeout'?' @ '+q.wait.toFixed(1)+'s':''}</span></div>`).join('');
    $('queue-progress').style.width=Math.min(100,queueTime/queueEnd*100)+'%';
  }
  function tickQueue(now) { if(!queuePlaying)return; queueTime=Math.min(queueEnd,queueTime+(now-queueLast)/1000);queueLast=now;renderQueue();if(queueTime>=queueEnd){stopQueue();const ok=queueModel.filter(r=>r.status==='done').length;$('queue-status').textContent=`Simulation complete: ${ok} completed, ${queueModel.length-ok} timed out. Teaching model only—not a benchmark.`;return;}queueFrame=requestAnimationFrame(tickQueue); }
  queueInputs.forEach(id=>$(id).addEventListener('input',resetQueue));
  $('queue-reset').addEventListener('click',resetQueue);
  $('queue-run').addEventListener('click',()=>{if(queuePlaying){stopQueue();$('queue-status').textContent='Paused at '+queueTime.toFixed(1)+' simulated seconds.';return;}if(queueTime>=queueEnd)buildQueue();queuePlaying=true;queueLast=performance.now();$('queue-run').textContent='Ⅱ Pause';$('queue-status').textContent='Running the deterministic FIFO simulation…';queueFrame=requestAnimationFrame(tickQueue);});
  resetQueue();

  // Models grub-arm.cfg only. Watchdog grace and storage failure injection are excluded.
  let boot={slot:'A',tries:0,good:1,seq:0,booted:'A',lkg:'A'}, bootCount=0, trialLive=false;
  function renderBoot(message) {
    $('boot-env').innerHTML=Object.entries(boot).map(([k,v])=>`<div><dt>${k}</dt><dd>${v}</dd></div>`).join('');
    ['A','B'].forEach(s=>{const active=boot.booted===s;const trial=active&&boot.good===0;const el=$('slot-'+s.toLowerCase());el.className='root-slot'+(active?' active':'')+(trial?' trial':'');$('slot-'+s.toLowerCase()+'-state').textContent=active?(trial?'TRIAL BOOT':'ACTIVE · '+(boot.good===1?'GOOD':'UNCOMMITTED')):(boot.lkg===s?'LAST KNOWN GOOD':'INACTIVE');});
    $('boot-attempt').textContent='REBOOTS MODELED: '+bootCount;
    if(message)$('boot-message').textContent=message;
    $('boot-commit').disabled=!trialLive||boot.good===1;
    $('boot-stage').textContent='Stage trial '+(boot.booted==='A'?'B':'A');
  }
  $('boot-stage').addEventListener('click',()=>{const current=boot.booted;const next=current==='A'?'B':'A';boot={slot:next,tries:3,good:0,seq:boot.seq+1,booted:next,lkg:current};trialLive=false;renderBoot(`Trial ${next} is armed with three attempts. LKG remains ${current}. Reboot to enter the candidate; no health proof is assumed yet.`);});
  $('boot-reboot').addEventListener('click',()=>{bootCount++;trialLive=false;if(boot.tries!==0){boot.booted=boot.slot;boot.tries--;boot.seq++;trialLive=true;renderBoot(`GRUB selected ${boot.slot} and decremented tries to ${boot.tries}. Commit after health passes, or reboot without committing to consume another attempt.`);}else if(boot.good!==1){boot.slot=boot.lkg;boot.good=1;boot.booted=boot.lkg;boot.seq++;renderBoot(`Trial attempts were exhausted. GRUB fell back to last-known-good slot ${boot.lkg}. This is the next-boot fallback path, not an instantaneous rollback.`);}else{boot.booted=boot.slot;renderBoot(`Rebooted committed slot ${boot.slot}. No trial counter is active.`);}});
  $('boot-commit').addEventListener('click',()=>{if(!trialLive||boot.good===1)return;boot={slot:boot.booted,tries:0,good:1,seq:boot.seq+1,booted:boot.booted,lkg:boot.booted};trialLive=false;renderBoot(`Health proof passed. Slot ${boot.slot} is committed and becomes the new last-known-good slot.`);});
  $('boot-reset').addEventListener('click',()=>{boot={slot:'A',tries:0,good:1,seq:0,booted:'A',lkg:'A'};bootCount=0;trialLive=false;renderBoot('Slot A is committed. Stage a trial to begin.');});
  renderBoot();

  function setScale(scale) {
    const values={wait:1.450478,exec:180.854,wall:6100};
    Object.entries(values).forEach(([key,value])=>{const width=scale==='log'?Math.max(2,Math.log10(value+1)/Math.log10(6101)*100):value/6100*100;$('bar-'+key).style.width=width+'%';});
    document.querySelectorAll('[data-scale]').forEach(b=>{const active=b.dataset.scale===scale;b.classList.toggle('active',active);b.setAttribute('aria-pressed',String(active));});
    $('scale-note').textContent=scale==='log'?'Logarithmic scale compresses differences: bar lengths are no longer proportional to latency. Exact values remain in the labels.':'Linear scale preserves ratios. Tiny bars may be difficult to see; switch to logarithmic to inspect their labels.';
  }
  document.querySelectorAll('[data-scale]').forEach(b=>b.addEventListener('click',()=>setScale(b.dataset.scale)));setScale('linear');
  document.addEventListener('visibilitychange',()=>{if(document.hidden){if(requestTimer){stopRequest();renderRequest();}if(queuePlaying)stopQueue();}});
})();
