export function status(game, installed) {
 if (installed?.running) return 'running';
 if (!installed?.installed) return 'available';
 return installed.release === game.package.release ? 'installed' : 'update';
}
export function filterGames(games, installs, {view='discover',query='',origin='',format='',sort='newest',favorites=[]}={}) {
 const q=query.toLowerCase().trim();
 return games.filter(g=>{const s=status(g,installs[g.slug]);return (view!=='library'||s!=='available')&&(view!=='updates'||s==='update')&&(view!=='favorites'||favorites.includes(g.slug))&&(!origin||g.origin===origin)&&(!format||g.presentation===format)&&(!q||[g.name,g.description,g.wiki.overview,g.wiki.story,...g.wiki.rules,...g.wiki.characters.map(c=>c.name)].join(' ').toLowerCase().includes(q));}).sort((a,b)=>sort==='name'?a.name.localeCompare(b.name):sort==='size'?a.package.size-b.package.size:b.created.localeCompare(a.created)||a.name.localeCompare(b.name));
}
export const size=n=>n>=1e9?`${(n/1e9).toFixed(1)} GB`:`${(n/1e6).toFixed(1)} MB`;
