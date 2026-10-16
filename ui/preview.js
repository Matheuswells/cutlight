// Lightweight live preview: plays the low-res proxies in <video> elements, composites with CSS,
// and runs HTML animation clips in sandboxed iframes. Also rasterises HTML clips for the final render.

const clamp = (v, a = 0, b = 1) => Math.min(b, Math.max(a, v));
const smooth = (p) => p * p * (3 - 2 * p);

// Runs inside every HTML animation iframe (sandbox: scripts only, opaque origin).
const RUNTIME = `(function(){
function seek(t){
  try{(document.getAnimations?document.getAnimations():[]).forEach(function(a){try{a.pause();a.currentTime=t*1000}catch(e){}})}catch(e){}
  if(typeof window.__seek==='function'){try{window.__seek(t)}catch(e){}}
}
var INH=/^(color$|cursor|direction|font|letter-spacing|line-height|list-style|text-align|text-indent|text-shadow|text-transform|text-rendering|text-anchor|text-orientation|visibility|white-space|word|overflow-wrap|tab-size|fill|stroke|paint-order|shape-rendering|dominant-baseline|quotes|caret-color|accent-color|-webkit-text|-webkit-font|writing-mode|color-scheme|pointer-events|image-rendering|marker|clip-rule|color-interpolation|hyphens|line-break)/;
var SVGNS='http://www.w3.org/2000/svg',REF={},ALW={'width':1,'height':1,'inline-size':1,'block-size':1,'transform-origin':1,'perspective-origin':1};
function ref(el){
  var k=el.namespaceURI+'|'+el.localName;
  if(REF[k])return REF[k];
  var box=document.createElement('div');box.style.cssText='position:absolute;visibility:hidden;left:-9999px;top:0';
  document.body.appendChild(box);
  var r;
  if(el.namespaceURI===SVGNS){var s=document.createElementNS(SVGNS,'svg');box.appendChild(s);r=document.createElementNS(SVGNS,el.localName);s.appendChild(r)}
  else{r=document.createElement(el.localName);box.appendChild(r)}
  var cs=getComputedStyle(r),o={};
  for(var i=0;i<cs.length;i++){o[cs[i]]=cs.getPropertyValue(cs[i])}
  box.remove();REF[k]=o;return o;
}
function snap(W,H){
  var root=document.documentElement,clone=root.cloneNode(true);
  var live=[root].concat([].slice.call(root.querySelectorAll('*')));
  var cl=[clone].concat([].slice.call(clone.querySelectorAll('*')));
  for(var i=0;i<live.length;i++){
    var el=live[i],cs=getComputedStyle(el),txt='',j,p,v;
    if(el===root||el===document.body){
      for(j=0;j<cs.length;j++){p=cs[j];txt+=p+':'+cs.getPropertyValue(p).replace(/"/g,"'")+';'}
    }else{
      // only what differs from the element default (non-inherited) or from the parent (inherited): keeps frames small and fast
      var rf=ref(el),pcs=getComputedStyle(el.parentElement);
      for(j=0;j<cs.length;j++){p=cs[j];v=cs.getPropertyValue(p);
        if(ALW[p]||v!==(INH.test(p)?pcs.getPropertyValue(p):rf[p]))txt+=p+':'+v.replace(/"/g,"'")+';'}
    }
    cl[i].setAttribute('style',txt+'animation:none!important;transition:none!important;');
  }
  // canvas / WebGL (three.js): bake the current pixels into an <img>. Must happen in the same task as the render call in __seek.
  [].slice.call(root.querySelectorAll('canvas')).forEach(function(c){
    var cc=cl[live.indexOf(c)];if(!cc||!cc.parentNode)return;
    var opaque=false,url='';
    try{var g=c.getContext('webgl2')||c.getContext('webgl');if(g){var a=g.getContextAttributes();opaque=!!a&&!a.alpha}}catch(e){}
    try{url=c.toDataURL(opaque?'image/jpeg':'image/png',.92)}catch(e){}
    var im=document.createElement('img');im.setAttribute('style',cc.getAttribute('style')||'');
    if(url)im.setAttribute('src',url);
    cc.parentNode.replaceChild(im,cc);
  });
  [].slice.call(clone.querySelectorAll('script,head')).forEach(function(n){n.remove()});
  // html/body collapse to their content inside a foreignObject: give them the frame size so their backgrounds fill it
  var full=';width:'+W+'px;height:'+H+'px;overflow:hidden';
  clone.setAttribute('style',(clone.getAttribute('style')||'')+full);
  var cb=clone.querySelector('body');if(cb)cb.setAttribute('style',(cb.getAttribute('style')||'')+full);
  clone.setAttribute('xmlns','http://www.w3.org/1999/xhtml');
  var x=new XMLSerializer().serializeToString(clone);
  return '<svg xmlns="http://www.w3.org/2000/svg" width="'+W+'" height="'+H+'"><foreignObject width="100%" height="100%">'+x+'</foreignObject></svg>';
}
addEventListener('message',function(e){
  var m=e.data||{};
  if(m.type==='seek'){seek(m.t)}
  else if(m.type==='snap'){seek(m.t);parent.postMessage({type:'snap',id:m.id,svg:snap(m.w,m.h)},'*')}
});
})();`;

export function injectRuntime(html) {
  const s = '<script>' + RUNTIME + '<\/script>';
  return /<\/head>/i.test(html) ? html.replace(/<\/head>/i, s + '</head>') : s + html;
}

export class Preview {
  constructor(stage, convert) {
    this.stage = stage;
    this.convert = convert;
    this.els = new Map();
    this.p = null;
  }

  setProject(p) {
    this.p = p;
    this.stage.style.setProperty('--ar', p.settings.width / p.settings.height);
    const ids = new Set(p.tracks.flatMap((t) => t.clips.map((c) => c.id)));
    for (const [id, e] of this.els) {
      if (!ids.has(id)) this.drop(id, e);
    }
  }

  drop(id, e) {
    if (e.el.pause) e.el.pause();
    e.el.remove();
    this.els.delete(id);
  }

  make(c) {
    const m = c.mediaId && this.p.media.find((x) => x.id === c.mediaId);
    let el;
    if (c.kind === 'video' || c.kind === 'audio') {
      el = document.createElement('video');
      el.preload = 'auto';
      el.playsInline = true;
      const f = m && (m.proxy || m.path);
      if (f) el.src = this.convert(f);
    } else if (c.kind === 'image') {
      el = document.createElement('img');
      if (m) el.src = this.convert(m.path);
    } else {
      el = document.createElement('iframe');
      el.setAttribute('sandbox', 'allow-scripts');
      el.srcdoc = injectRuntime(this.resolve ? this.resolve(c.html || '') : (c.html || ''));
    }
    this.stage.appendChild(el);
    const e = { el, key: (c.kind === 'html' ? c.html : (m && (m.proxy || m.path))) || '' };
    this.els.set(c.id, e);
    return e;
  }

  update(t, playing) {
    const p = this.p;
    if (!p) return;
    const W = p.settings.width, H = p.settings.height;
    const sw = this.stage.clientWidth || 1, sh = this.stage.clientHeight || 1;
    const want = new Set();
    p.tracks.forEach((tr, ti) => {
      const clips = [...tr.clips].sort((a, b) => a.start - b.start);
      clips.forEach((c, ci) => {
        const end = c.start + c.duration;
        const active = t >= c.start && t < end;
        const near = !active && c.start > t && c.start - t < 1.5 && c.kind !== 'html';
        if (!active && !near) return;
        want.add(c.id);
        let e = this.els.get(c.id);
        const m = c.mediaId && p.media.find((x) => x.id === c.mediaId);
        const key = (c.kind === 'html' ? c.html : (m && (m.proxy || m.path))) || '';
        if (e && e.key !== key) { this.drop(c.id, e); e = null; }
        if (!e) e = this.make(c);
        const el = e.el;
        const lt = t - c.start;
        const next = clips[ci + 1];
        const nextTr = next && next.transition && next.start < end + 0.05 ? next.transition : null;
        const fadeKind = (x) => x && (x.kind === 'dissolve' || x.kind === 'fade-black');
        const fi = Math.max(c.fadeIn, fadeKind(c.transition) ? c.transition.duration : 0);
        const fo = Math.max(c.fadeOut, nextTr && nextTr.kind === 'fade-black' ? nextTr.duration : 0);
        const fade = (fi > 0 ? clamp(lt / fi) : 1) * (fo > 0 ? clamp((c.duration - lt) / fo) : 1);

        if (c.kind !== 'audio') {
          const vis = active && !tr.hidden;
          let dx = 0, dy = 0;
          if (c.transition && c.transition.kind.startsWith('slide') && lt < c.transition.duration) {
            const q = 1 - smooth(clamp(lt / c.transition.duration));
            const k = c.transition.kind;
            if (k === 'slide-left') dx = q * sw;
            if (k === 'slide-right') dx = -q * sw;
            if (k === 'slide-up') dy = q * sh;
            if (k === 'slide-down') dy = -q * sh;
          }
          const s = el.style;
          s.visibility = vis ? 'visible' : 'hidden';
          s.zIndex = (p.tracks.length - ti) * 1000 + ci;
          s.opacity = c.opacity * fade;
          if (c.kind === 'html') {
            s.left = '0'; s.top = '0';
            s.width = W + 'px'; s.height = H + 'px';
            s.transformOrigin = '0 0';
            s.transform = `scale(${sw / W})`;
          } else {
            s.left = c.x * 100 + '%'; s.top = c.y * 100 + '%';
            s.width = c.scale * 100 + '%'; s.height = c.scale * 100 + '%';
            s.objectFit = c.fit === 'fill' ? 'cover' : 'contain';
            s.transform = `translate(-50%,-50%) translate(${dx}px,${dy}px)`;
          }
        } else {
          el.style.display = 'none';
        }

        if (c.kind === 'html') {
          if (e.lastT !== lt && el.contentWindow) {
            e.lastT = lt;
            el.contentWindow.postMessage({ type: 'seek', t: lt }, '*');
          }
        } else if (c.kind === 'video' || c.kind === 'audio') {
          const target = c.inPoint + (active ? lt * c.speed : 0);
          el.playbackRate = Math.min(16, Math.max(0.0625, c.speed));
          el.muted = !active || tr.muted || (c.kind === 'video' && !c.audio);
          const ai = Math.max(c.fadeIn, c.transition ? c.transition.duration : 0);
          const ao = Math.max(c.fadeOut, nextTr ? nextTr.duration : 0);
          const afade = (ai > 0 ? clamp(lt / ai) : 1) * (ao > 0 ? clamp((c.duration - lt) / ao) : 1);
          el.volume = clamp(c.volume * afade);
          if (playing && active) {
            if (el.paused) {
              el.currentTime = target;
              el.play().catch(() => {});
            } else if (Math.abs(el.currentTime - target) > 0.3) {
              el.currentTime = target;
            }
          } else {
            if (!el.paused) el.pause();
            if (el.readyState > 0 && Math.abs(el.currentTime - target) > 0.04) el.currentTime = target;
          }
        }
      });
    });
    for (const [id, e] of this.els) {
      if (!want.has(id)) this.drop(id, e);
    }
  }

  stopAll() {
    for (const [, e] of this.els) if (e.el.pause) e.el.pause();
  }
}

// ---------- frame-exact rasterisation of HTML clips (used by the Rust renderer) ----------
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function snapFrame(frame, t, w, h) {
  return new Promise((res, rej) => {
    const id = Math.random();
    const on = (e) => {
      if (e.source === frame.contentWindow && e.data && e.data.type === 'snap' && e.data.id === id) {
        removeEventListener('message', on);
        clearTimeout(to);
        res(e.data.svg);
      }
    };
    const to = setTimeout(() => { removeEventListener('message', on); rej(new Error('animation did not respond')); }, 15000);
    addEventListener('message', on);
    frame.contentWindow.postMessage({ type: 'snap', id, t, w, h }, '*');
  });
}

export async function renderHtmlFrames({ html, width, height, fps, frames }, send, onProgress) {
  const f = document.createElement('iframe');
  f.setAttribute('sandbox', 'allow-scripts');
  f.style.cssText = `position:fixed;left:-100000px;top:0;width:${width}px;height:${height}px;border:0`;
  f.srcdoc = injectRuntime(html);
  const loaded = new Promise((r) => (f.onload = r));
  document.body.appendChild(f);
  await loaded;
  await sleep(450);
  const cv = document.createElement('canvas');
  cv.width = width;
  cv.height = height;
  const ctx = cv.getContext('2d');
  try {
    for (let i = 0; i < frames; i++) {
      const svg = await snapFrame(f, i / fps, width, height);
      // data: URL (not blob:) keeps the canvas untainted
      const img = new Image();
      img.src = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);
      await img.decode();
      ctx.clearRect(0, 0, width, height);
      ctx.drawImage(img, 0, 0);
      const blob = await new Promise((r) => cv.toBlob(r, 'image/png'));
      await send(new Uint8Array(await blob.arrayBuffer()), i);
      if (onProgress) onProgress(i + 1, frames);
    }
  } finally {
    f.remove();
  }
}
