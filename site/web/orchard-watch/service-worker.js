/* A complete, hash-checked version activates atomically. Saves are never stored in asset caches. */
const PREFIX='be2:'+self.registration.scope+':';
const ID='c1bea588f06a3d4370cf792fe8eee256057e8cfb00c2fcba12def27bec547628',CACHE=PREFIX+ID;
const HASHES={"app.webmanifest": "ef587269c15b5b7fbb0da9c6137a06f93b4cdc0d34b768ade053675cfa314398", "build.json": "d4dc0cbafbf39642459b0536445efc64c5f0d76c576f3ed50202adcb8ea70895", "game.wasm": "0f8325041e273a0e21fa2ee2990a4d3d9bdce3efb8a56b514f3859ac6ab9bc57", "index.html": "0e875d9e580597d63aec8989e354083dc2a698339f5f351b2aaeb4bb7eca0b30", "loader.js": "99a6b756e7eb2851e47bdf8d0cbc16a85c0acc14d9ecf9f740f5e1b2a9614261", "mobile.js": "6085597c317f1aea7ece3ca2947b1e03f4be4a8d2ade652b1929bd8b6d1248ae", "platform.js": "a98569b537a863ed1f5d6b12a3b1b3580a4ab11654c91add17462b63f7539ed4", "thumbnail.png": "e4ed6f7f3f7b2dec0940daf28499b4f817f3ea206b1167b41ba10c8aae972913"};
const FILES=[...Object.keys(HASHES),'service-worker.js','manifest.json'];
const sha=async bytes=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),b=>b.toString(16).padStart(2,'0')).join('');
self.addEventListener('install',event=>event.waitUntil((async()=>{
  try {
    const response=await fetch(new URL('manifest.json',self.registration.scope),{cache:'no-store'});
    if(!response.ok)throw new Error('Update manifest unavailable');
    const manifest=await response.clone().json();
    if(manifest.package_id!==ID)throw new Error('Incomplete deployment: manifest/worker version mismatch');
    for(const [name,digest] of Object.entries(HASHES))if(manifest.file_sha256[name]!==digest)throw new Error('Update manifest hash mismatch: '+name);
    const cache=await caches.open(CACHE);
    // No activate/claim/cache replacement until every file has passed its declared hash.
    for(const name of FILES){
      const asset=name==='manifest.json'?response:await fetch(new URL(name,self.registration.scope),{cache:'no-store'});
      if(!asset.ok)throw new Error('Incomplete update: '+name);
      if(name!=='manifest.json'&&await sha(await asset.clone().arrayBuffer())!==manifest.file_sha256[name])throw new Error('Corrupt update: '+name);
      await cache.put(name,asset);
    }
    await self.skipWaiting();
  } catch(error) {
    await caches.delete(CACHE); // failed candidate; the active version survives
    throw error;
  }
})()));
self.addEventListener('activate',event=>event.waitUntil((async()=>{
  const cache=await caches.open(CACHE);
  for(const name of FILES)if(!await cache.match(name))throw new Error('Candidate cache is incomplete');
  await Promise.all((await caches.keys()).filter(k=>k.startsWith(PREFIX)&&k!==CACHE).map(k=>caches.delete(k)));
  await self.clients.claim();
})()));
self.addEventListener('fetch',event=>{
  if(event.request.method!=='GET')return;
  const url=new URL(event.request.url);if(url.origin!==location.origin||!url.href.startsWith(self.registration.scope))return;
  const relative=decodeURIComponent(url.pathname.slice(new URL(self.registration.scope).pathname.length));
  if(relative&&!FILES.includes(relative))return;
  const name=relative||'index.html';
  event.respondWith(caches.open(CACHE).then(async cache=>{
    const cached=await cache.match(name);if(cached)return cached;
    return fetch(event.request); // recover online if the browser evicts cached assets
  }));
});
