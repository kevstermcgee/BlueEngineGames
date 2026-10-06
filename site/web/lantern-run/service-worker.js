/* Build substitutes the exact package cache key. Storage is separate from disposable asset caches. */
const CACHE='be2:'+self.registration.scope+':7b18b59b7736611eae93e7b8d62c906131df6fb02a4e4f128ff8f23d45503ff9';
const FILES=["app.webmanifest", "game.wasm", "index.html", "loader.js", "mobile.js", "platform.js", "thumbnail.png", "service-worker.js", "manifest.json"];
self.addEventListener('install',event=>event.waitUntil(caches.open(CACHE).then(cache=>cache.addAll(FILES)).then(()=>self.skipWaiting())));
self.addEventListener('activate',event=>event.waitUntil(caches.keys().then(keys=>Promise.all(keys.filter(k=>k.startsWith('be2:'+self.registration.scope+':')&&k!==CACHE).map(k=>caches.delete(k)))).then(()=>self.clients.claim())));
self.addEventListener('fetch',event=>{
  if(event.request.method!=='GET')return;
  const url=new URL(event.request.url);if(url.origin!==location.origin||!url.href.startsWith(self.registration.scope))return;
  const relative=decodeURIComponent(url.pathname.slice(new URL(self.registration.scope).pathname.length));
  if(relative&&!FILES.includes(relative))return;
  const name=relative||'index.html';
  event.respondWith(caches.open(CACHE).then(async cache=>{
    const cached=await cache.match(name);if(cached)return cached;
    return fetch(event.request);
  }));
});
