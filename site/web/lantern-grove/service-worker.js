/* Build substitutes the exact package cache key. Storage is separate from disposable asset caches. */
const CACHE='be2:'+self.registration.scope+':d20b9952a876836ad8d8c0b0f8dfdf8a3f65235d76b3456cd0445c7f5906de0b';
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
