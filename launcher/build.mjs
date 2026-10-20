import { build } from "esbuild";
import {
  mkdir,
  copyFile,
  cp,
  readFile,
  writeFile,
  rm,
  access,
} from "node:fs/promises";
import { createHash } from "node:crypto";
await rm("dist", { recursive: true, force: true });
await mkdir("dist/images", { recursive: true });
await build({
  entryPoints: ["src/app.js"],
  outfile: "dist/app.js",
  bundle: true,
  format: "esm",
  target: "es2022",
});
await copyFile("src/index.html", "dist/index.html");
await copyFile("src/app.css", "dist/app.css");
await copyFile("src/updater.css", "dist/updater.css");
await copyFile("../site/feedback.css", "dist/feedback.css");
await copyFile("../site/collaboration.css", "dist/collaboration.css");
await cp("public", "dist", { recursive: true });
const catalog = JSON.parse(await readFile("public/catalog.json", "utf8")),
  images = {};
async function capture(source, url) {
  const bytes = await readFile(source);
  const name =
    createHash("sha256").update(bytes).digest("hex").slice(0, 20) + ".png";
  await copyFile(source, "dist/images/" + name);
  images[url] = "./images/" + name;
}
for (const g of catalog.games) {
  await capture(`../site/thumbs/${g.slug}.png`, g.image);
  let dir = `../site/wiki/${g.slug}`;
  try {
    await access(`${dir}/wiki.json`);
  } catch {
    dir = `../games/${g.slug}`;
  }
  const doc = JSON.parse(await readFile(`${dir}/wiki.json`, "utf8"));
  for (const p of [...doc.gallery, ...(doc.characters || [])]) {
    if (!p.image) continue;
    const bytes = await readFile(`${dir}/${p.image}`),
      hash = createHash("sha256").update(bytes).digest("hex").slice(0, 20);
    await capture(
      `${dir}/${p.image}`,
      `https://kevstermcgee.github.io/BlueEngineGames/games/${g.slug}/images/${hash}.png`,
    );
  }
}
await writeFile("dist/images.json", JSON.stringify(images));
console.log(
  `Bundled ${catalog.games.length} guides and ${Object.keys(images).length} offline images.`,
);
