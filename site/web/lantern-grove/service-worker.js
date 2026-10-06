/* A complete, hash-checked version activates atomically. Saves are never stored in asset caches. */
const PREFIX='be2:'+self.registration.scope+':';
const ID='1a5d414be0df9e98c2a81808281141bbc5b15d335972624cf89bc3b7101f3faf',CACHE=PREFIX+ID;
const HASHES={"app.webmanifest": "98359077a4263524130043c1b16c9a1c61a8ccc1e322c62f7311d92b7087ff3c", "build.json": "a1908011a791297d96862e2f1d378a01df8649e838132559ea7dd747e4ba3566", "game.wasm": "52d7d77c20c91e7d8f8ace6bb440e4d07bed3399e2fd4d354e18e787df3ee5c9", "index.html": "e9d7bd1a5e79a229832ad79a15f1aa1c79417f3a807e4cb9c65bc9cb6f7811e7", "loader.js": "99a6b756e7eb2851e47bdf8d0cbc16a85c0acc14d9ecf9f740f5e1b2a9614261", "mobile.js": "6085597c317f1aea7ece3ca2947b1e03f4be4a8d2ade652b1929bd8b6d1248ae", "platform.js": "a98569b537a863ed1f5d6b12a3b1b3580a4ab11654c91add17462b63f7539ed4", "thumbnail.png": "8de1c950957c0ad5fe1b7106d7b6453acd656d2f3990a41f6d9aa55b42bce201"};
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
