(() => {
  'use strict';
  const dialog=document.getElementById('search-dialog');
  const input=document.getElementById('search-input');
  const results=document.getElementById('search-results');
  const prefix=document.body.dataset.base || '';
  const index=window.WRAITH_SEARCH || [];
  let selected=0, matches=[];
  const escape=s=>String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  function searchDocs() {
    const query=input.value.trim().toLowerCase();
    const terms=query.split(/\s+/).filter(Boolean);
    matches=!terms.length?index.filter(x=>x.level===1).slice(0,8):index.map(entry=>{const title=entry.title.toLowerCase(),text=entry.text.toLowerCase();let score=0;for(const term of terms){if(!title.includes(term)&&!text.includes(term))return null;score+=(title.includes(term)?10:0)+(text.split(term).length-1);}return {...entry,score};}).filter(Boolean).sort((a,b)=>b.score-a.score).slice(0,18);
    selected=0;renderResults(query);
  }
  function renderResults(query='') {
    if(!matches.length){results.innerHTML='<div class="search-empty">No matching section. Try “quota”, “signature”, “GRUB”, or “timeout”.</div>';return;}
    results.innerHTML=matches.map((r,i)=>{let text=r.text;const at=query?text.toLowerCase().indexOf(query.toLowerCase()):-1;if(at>50)text='…'+text.slice(at-35);return `<a class="search-result ${i===selected?'selected':''}" href="${prefix}${r.url}" data-result="${i}"><small>${escape(r.guide)}</small><b>${escape(r.title)}</b><p>${escape(text.slice(0,160))}${text.length>160?'…':''}</p></a>`;}).join('');
  }
  function openSearch(){if(!dialog)return;if(!dialog.open)dialog.showModal();searchDocs();input.focus();}
  function closeSearch(){if(dialog?.open)dialog.close();}
  document.querySelectorAll('[data-search-open]').forEach(b=>b.addEventListener('click',openSearch));
  document.querySelector('[data-search-close]')?.addEventListener('click',closeSearch);
  input?.addEventListener('input',searchDocs);
  dialog?.addEventListener('click',e=>{if(e.target===dialog){const r=dialog.getBoundingClientRect();if(e.clientX<r.left||e.clientX>r.right||e.clientY<r.top||e.clientY>r.bottom)closeSearch();}});
  document.addEventListener('keydown',e=>{if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='k'){e.preventDefault();dialog?.open?closeSearch():openSearch();}if(!dialog?.open)return;if(e.key==='ArrowDown'||e.key==='ArrowUp'){e.preventDefault();selected=Math.max(0,Math.min(matches.length-1,selected+(e.key==='ArrowDown'?1:-1)));renderResults(input.value.trim());results.querySelector('.selected')?.scrollIntoView({block:'nearest'});}else if(e.key==='Enter'&&matches[selected]&&document.activeElement===input){e.preventDefault();location.href=prefix+matches[selected].url;}});
  const menu=document.querySelector('.menu-toggle'),mobile=document.querySelector('.mobile-nav');
  menu?.addEventListener('click',()=>{const open=menu.getAttribute('aria-expanded')!=='true';menu.setAttribute('aria-expanded',String(open));mobile.hidden=!open;});
  mobile?.querySelectorAll('a').forEach(a=>a.addEventListener('click',()=>{mobile.hidden=true;menu.setAttribute('aria-expanded','false');}));
  const progress=document.querySelector('.reading-progress');
  function updateProgress(){if(!progress)return;const max=document.documentElement.scrollHeight-innerHeight;progress.style.width=(max>0?scrollY/max*100:0)+'%';}
  addEventListener('scroll',updateProgress,{passive:true});updateProgress();
  document.querySelectorAll('.copy-code').forEach(button=>button.addEventListener('click',async()=>{const code=button.parentElement.querySelector('code')?.textContent||'';try{if(navigator.clipboard&&window.isSecureContext)await navigator.clipboard.writeText(code);else{const area=document.createElement('textarea');area.value=code;area.style.position='fixed';area.style.opacity='0';document.body.append(area);area.select();const ok=document.execCommand('copy');area.remove();if(!ok)throw Error('copy unavailable');}button.textContent='Copied';}catch{button.textContent='Select code to copy';}setTimeout(()=>button.textContent='Copy',1800);}));
})();
