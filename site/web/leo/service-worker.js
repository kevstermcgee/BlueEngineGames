/* A complete, hash-checked version activates atomically. Saves are never stored in asset caches. */
const PREFIX='be2:'+self.registration.scope+':';
const ID='031c3af81636d900dfc03a84bea34966d879ec054e7d793f987991b19f96d5a5',CACHE=PREFIX+ID;
const HASHES={"AUDIO.md": "476e05189f46d3544ec5630915e0bcada9dc2dfe8cdd3bf3887d1424af9ac694", "app.webmanifest": "82e71b02cc733da758218379689d25d69d58131aeee46004b1707c0e8d9dec66", "assets/audio/music/bank.json": "0ae996df16796e9ebc31f3f28908e37baaeac346bd72f69b0e0cad89d45d4278", "assets/audio/music/music-home.wav": "cf5e16f2728a6fc2f88620fc962823a1c8ab9349da7be6bfeff61a1c895ce169", "assets/audio/music/music-starlight.wav": "1eaba38e13c8866281539718d185dec839b4c9ade5697d9628b85bfc57e542f6", "assets/audio/music/music-wander.wav": "eac4ee99d235968f4068640649c603f2711da5af40c234ad0c431c5f88336283", "assets/audio/nature/bank.json": "f9300dcdc4e1e45089a898bbccc761c33aa90563a37c7db071841abaf17bbac6", "assets/audio/nature/music-birds.wav": "186c96f4e73fba62b698cd900fc206268211b494b6f70262a55a7a9e3d13fb86", "assets/audio/nature/music-leaves.wav": "c8e857e6c23a1439ba169a93aaf9e06efc001617758e437ec9891b75c4f31072", "assets/audio/nature/music-night.wav": "62fd3719945ad7d03ae7e488001ebe453cab9193d7a20ead186bb110c7759ea7", "assets/audio/nature/music-wind.wav": "15dc2e81ce07026d0fe0f2bc29e6f4fe5bfe5e139bb615a92dd3fd86f9704916", "build.json": "86c4edd0bcf9272adc1b8828560b560a972dab843d3c08c14b87c919347d1a8c", "game.wasm": "053dcfb2d2e638f9d5b01a23e583eaa9303d0b2e8b0c2dea51667c1fd62120f3", "index.html": "713a6b5ead9c1fe5d0ed155193ed94c1f0766f5fe79e7991cc6bef62f18f1635", "loader.js": "99a6b756e7eb2851e47bdf8d0cbc16a85c0acc14d9ecf9f740f5e1b2a9614261", "mobile.js": "6085597c317f1aea7ece3ca2947b1e03f4be4a8d2ade652b1929bd8b6d1248ae", "platform.js": "a98569b537a863ed1f5d6b12a3b1b3580a4ab11654c91add17462b63f7539ed4", "thumbnail.png": "e5d547951fe0f2dac4c9ca81e9770959a6fb1514524a4a6348358037d11dd573"};
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
