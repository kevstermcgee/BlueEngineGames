import test from "node:test";
import assert from "node:assert/strict";
import { webcrypto } from "node:crypto";
import { mkdtemp, writeFile, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import {
  canonicalMetadata,
  readGameMetadata,
  validateGameMetadata,
  MAX_METADATA_BYTES,
  METADATA_MAGIC,
} from "./game-metadata.mjs";
globalThis.crypto ||= webcrypto;
const encode = (s) => new TextEncoder().encode(s);
export const document = {
  format: "blueengine.game",
  schema_version: 1,
  game: {
    slug: "metadata-orbit",
    title: "Embedded Orbit",
    version: "0.2.0",
    description: "A game from its own metadata.",
    controls: "WASD moves; Space jumps.",
    authors: ["A cousin"],
    tags: ["puzzle"],
  },
  source: {
    repository: "kevstermcgee/BlueEngine",
    commit: "a".repeat(40),
    game_path: "games/metadata-orbit",
  },
  engine: null,
};
export async function envelope(value = document) {
  const payload = encode(canonicalMetadata(value));
  const result = new Uint8Array(payload.length + 68);
  result.set(payload);
  result.set(
    new Uint8Array(await crypto.subtle.digest("SHA-256", payload)),
    payload.length,
  );
  new DataView(result.buffer).setBigUint64(
    payload.length + 32,
    BigInt(payload.length),
    true,
  );
  result.set(encode(METADATA_MAGIC), payload.length + 40);
  return result;
}
export async function executable(value = document, signed = false) {
  const data = new Uint8Array(1024),
    v = new DataView(data.buffer);
  data.set([77, 90]);
  v.setUint32(60, 128, true);
  v.setUint32(128, 0x4550, true);
  v.setUint16(134, 1, true);
  v.setUint16(148, 224, true);
  v.setUint16(152, 0x10b, true);
  v.setUint32(244, 16, true);
  v.setUint32(392, 512, true);
  v.setUint32(396, 512, true);
  const metadata = await envelope(value),
    pad = signed ? (8 - ((data.length + metadata.length) % 8)) % 8 : 0;
  if (signed) {
    v.setUint32(280, data.length + metadata.length + pad, true);
    v.setUint32(284, 16, true);
  }
  const cert = new Uint8Array(signed ? 16 : 0);
  if (signed) {
    new DataView(cert.buffer).setUint32(0, 16, true);
    new DataView(cert.buffer).setUint16(4, 0x200, true);
  }
  return new File(
    [data, metadata, new Uint8Array(pad), cert],
    "misleading-name.exe",
  );
}
const crc = (data) => {
  let value = 0xffffffff;
  for (const byte of data) {
    value ^= byte;
    for (let i = 0; i < 8; i++)
      value = (value >>> 1) ^ (0xedb88320 & -(value & 1));
  }
  return (value ^ 0xffffffff) >>> 0;
};
export async function zip(value = document, options = {}) {
  const payload = encode(canonicalMetadata(value)),
    records = [],
    central = [];
  let offset = 0;
  for (const name of options.names || ["blueengine.game.json"]) {
    const n = encode(name),
      local = new Uint8Array(30 + n.length + payload.length),
      l = new DataView(local.buffer);
    l.setUint32(0, 0x04034b50, true);
    l.setUint32(14, crc(payload), true);
    l.setUint32(18, payload.length, true);
    l.setUint32(22, payload.length, true);
    l.setUint16(26, n.length, true);
    local.set(n, 30);
    local.set(payload, 30 + n.length);
    const entry = new Uint8Array(46 + n.length),
      c = new DataView(entry.buffer);
    c.setUint32(0, 0x02014b50, true);
    c.setUint16(10, options.method || 0, true);
    c.setUint32(16, crc(payload), true);
    c.setUint32(20, payload.length, true);
    c.setUint32(24, options.expandedSize || payload.length, true);
    c.setUint16(28, n.length, true);
    c.setUint32(42, offset, true);
    if (options.attributes !== undefined)
      c.setUint32(38, options.attributes, true);
    entry.set(n, 46);
    records.push(local);
    central.push(entry);
    offset += local.length;
  }
  const comment = options.noComment
    ? new Uint8Array()
    : await envelope(options.comment || value);
  const end = new Uint8Array(22),
    e = new DataView(end.buffer);
  e.setUint32(0, 0x06054b50, true);
  e.setUint16(8, central.length, true);
  e.setUint16(10, central.length, true);
  e.setUint32(
    12,
    central.reduce((n, p) => n + p.length, 0),
    true,
  );
  e.setUint32(16, offset, true);
  e.setUint16(20, comment.length, true);
  return new File(
    [...records, ...central, end, comment],
    "different-title.zip",
  );
}
test("EXE metadata supplies real identity independently of its filename", async () => {
  const result = await readGameMetadata(await executable());
  assert.deepEqual(result.document, document);
  assert.equal(result.document.game.title, "Embedded Orbit");
});
test("signed PE reads metadata before the structurally validated certificate table", async () => {
  assert.deepEqual(
    (await readGameMetadata(await executable(document, true))).document,
    document,
  );
  const file = await executable(document, true),
    bytes = new Uint8Array(await file.arrayBuffer());
  new DataView(bytes.buffer).setUint32(284, 8, true);
  await assert.rejects(
    readGameMetadata(new File([bytes], "bad.exe")),
    /certificate/,
  );
});
test("checksums, truncation, forged lengths and PE section overlap fail closed", async () => {
  const file = await executable(),
    bytes = new Uint8Array(await file.arrayBuffer());
  const tampered = bytes.slice();
  tampered[1030] ^= 1;
  await assert.rejects(
    readGameMetadata(new File([tampered], "bad.exe")),
    /checksum/,
  );
  await assert.rejects(
    readGameMetadata(new File([bytes.slice(0, -1)], "bad.exe")),
    /metadata/,
  );
  const length = bytes.slice();
  new DataView(length.buffer).setBigUint64(length.length - 36, 999999n, true);
  await assert.rejects(
    readGameMetadata(new File([length], "bad.exe")),
    /oversized/,
  );
  const overlap = bytes.slice();
  new DataView(overlap.buffer).setUint32(392, bytes.length, true);
  await assert.rejects(
    readGameMetadata(new File([overlap], "bad.exe")),
    /sections/,
  );
});
test("EXE inspection reads bounded slices even when the file is several gigabytes", async () => {
  const original = await executable(),
    bytes = new Uint8Array(await original.arrayBuffer());
  const metadata = await envelope(),
    size = 6 * 1024 ** 3,
    reads = [];
  const file = {
    name: "large.exe",
    size,
    slice(start, end) {
      reads.push(end - start);
      const data = new Uint8Array(end - start);
      for (const [position, content] of [
        [0, bytes.subarray(0, 1024)],
        [size - metadata.length, metadata],
      ]) {
        const from = Math.max(start, position),
          to = Math.min(end, position + content.length);
        if (to > from)
          data.set(
            content.subarray(from - position, to - position),
            from - start,
          );
      }
      return new Blob([data]);
    },
  };
  assert.equal(
    (await readGameMetadata(file)).document.game.slug,
    document.game.slug,
  );
  assert.ok(Math.max(...reads) <= MAX_METADATA_BYTES + 68);
});
test("ZIP manifest and comment must agree; compressed manifests and archive aliases are refused", async () => {
  assert.deepEqual((await readGameMetadata(await zip())).document, document);
  const changed = structuredClone(document);
  changed.game.title = "Conflicting title";
  await assert.rejects(
    readGameMetadata(await zip(document, { comment: changed })),
    /disagree/,
  );
  await assert.rejects(
    readGameMetadata(await zip(document, { noComment: true })),
    /envelope/,
  );
  await assert.rejects(
    readGameMetadata(await zip(document, { method: 8 })),
    /stored/,
  );
  await assert.rejects(
    readGameMetadata(await zip(document, { expandedSize: 2 ** 30 })),
    /stored/,
  );
  for (const name of [
    "BLUEENGINE.GAME.JSON",
    "../blueengine.game.json",
    "aux.json",
  ])
    await assert.rejects(
      readGameMetadata(await zip(document, { names: [name] })),
      /manifest|unsafe/,
    );
  await assert.rejects(
    readGameMetadata(
      await zip(document, {
        names: ["blueengine.game.json", "BLUEENGINE.GAME.JSON"],
      }),
    ),
    /duplicate/,
  );
  for (const mode of [0o120000, 0o040000, 0o020000])
    await assert.rejects(
      readGameMetadata(await zip(document, { attributes: (mode << 16) >>> 0 })),
      /regular/,
    );
});
test("BAT reads a strict inert initial base64 comment block without executing its body", async () => {
  const base64 = Buffer.from(canonicalMetadata(document)).toString("base64");
  const body = "@echo Would execute only if launched\r\n";
  const content =
    "@rem BLUEENGINE_GAME_METADATA_V1_BEGIN\r\n" +
    base64
      .match(/.{1,76}/g)
      .map((line) => "@rem " + line + "\r\n")
      .join("") +
    "@rem BLUEENGINE_GAME_METADATA_V1_END\r\n" +
    body;
  assert.deepEqual(
    (await readGameMetadata(new File([content], "game.bat"))).document,
    document,
  );
  await assert.rejects(
    readGameMetadata(new File(["\ufeff" + content], "game.cmd")),
    /no portable/,
  );
  await assert.rejects(
    readGameMetadata(new File([content + content], "game.bat")),
    /duplicated/,
  );
});
test("generic and publishing hints enforce limits without trusting authorship or HTML", () => {
  const value = structuredClone(document);
  value.game.title = '<img src=x onerror="steal()">';
  assert.equal(validateGameMetadata(value).game.title, value.game.title);
  value.source.game_path = "../secrets";
  assert.throws(() => validateGameMetadata(value), /source pin/);
  for (const edit of [
    (d) => d.game.authors.push(""),
    (d) => (d.game.controls = "a".repeat(8001)),
    (d) => (d.source.commit = "main"),
    (d) => (d.source.tree = "main"),
    (d) => (d.source.tree = null),
    (d) => (d.source.game_path = "games/NUL"),
    (d) => (d.game.title = "\ud800"),
    (d) => (d.hint = 1.5),
    (d) => (d.hint = "\0"),
  ]) {
    const next = structuredClone(document);
    edit(next);
    assert.throws(() => validateGameMetadata(next));
  }
  const draft = structuredClone(document);
  draft.source = null;
  assert.equal(validateGameMetadata(draft).source, null);
  const unicode = structuredClone(document);
  unicode.game.title = "🎮".repeat(160);
  assert.equal(validateGameMetadata(unicode).game.title, unicode.game.title);
});
test("unknown formats and files with no metadata have a useful export next step", async () => {
  await assert.rejects(
    readGameMetadata(new File(["data"], "game.rar")),
    /export as ZIP/,
  );
  await assert.rejects(
    readGameMetadata(new File(["@echo off"], "legacy.bat")),
    /Re-export/,
  );
});
test("actual canonical Python writer interoperates with JS for EXE, signed EXE, CP1252 BAT and ZIP", async (t) => {
  const directory = await mkdtemp(
    join(tmpdir(), "blueengine-metadata-interop-"),
  );
  t.after(() => rm(directory, { recursive: true, force: true }));
  const unicode = structuredClone(document);
  unicode.game.title = "🎮".repeat(160);
  unicode.game.controls = "é".repeat(8000);
  unicode.game.description = "é".repeat(16000);
  unicode.game.authors = ["Cousin · 作者 🎮"];
  unicode.hints = {
    10: "numeric keys keep canonical lexical order",
    2: "after ten",
  };
  unicode.source.commit = "A".repeat(40);
  unicode.source.tree = "B".repeat(40);
  await writeFile(join(directory, "metadata.json"), canonicalMetadata(unicode));
  for (const [name, signed] of [
    ["native.exe", false],
    ["signed.exe", false],
  ])
    await writeFile(
      join(directory, name),
      new Uint8Array(await (await executable(document, signed)).arrayBuffer()),
    );
  await writeFile(
    join(directory, "launcher.bat"),
    Buffer.from([64, 101, 99, 104, 111, 32, 0x96, 13, 10]),
  );
  const module = fileURLToPath(
    new URL("../distribution/game_metadata.py", import.meta.url),
  );
  const script = `import importlib.util,json,pathlib,struct,sys,zipfile\nspec=importlib.util.spec_from_file_location('metadata',sys.argv[1]);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)\nroot=pathlib.Path(sys.argv[2]);document=json.loads((root/'metadata.json').read_text(encoding='utf-8'))\nwith zipfile.ZipFile(root/'game.zip','w') as archive: archive.writestr('payload.dat',b'game data',compress_type=zipfile.ZIP_DEFLATED)\nfor name in ('native.exe','signed.exe','launcher.bat','game.zip'): m.embed(root/name,document)\n# Genuine ZIP64 records, with a small payload to keep this test bounded.\ndata=bytearray((root/'game.zip').read_bytes());at=data.rfind(b'PK\\x05\\x06');count=struct.unpack_from('<H',data,at+10)[0];size,offset=struct.unpack_from('<II',data,at+12);end=bytearray(data[at:at+22]);struct.pack_into('<HHII',end,8,65535,65535,0xffffffff,0xffffffff);record=struct.pack('<IQHHIIQQQQ',0x06064b50,44,45,45,0,0,count,count,size,offset);locator=struct.pack('<IIQI',0x07064b50,0,at,1);(root/'zip64.zip').write_bytes(data[:at]+record+locator+end+data[at+22:]);assert m.extract(root/'zip64.zip')==document\n# Model the certificate layout added AFTER embedding; writers must never modify signed EXEs.\npath=root/'signed.exe';data=bytearray(path.read_bytes());data.extend(b'\\0'*(-len(data)%8));struct.pack_into('<II',data,280,len(data),16);data.extend(struct.pack('<IHH',16,0x200,2)+b'certificate'[:8]);path.write_bytes(data)\n`;
  const result = spawnSync(
    process.platform === "win32" ? "python" : "python3",
    ["-c", script, module, directory],
    { encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr || result.stdout);
  for (const name of [
    "native.exe",
    "signed.exe",
    "launcher.bat",
    "game.zip",
    "zip64.zip",
  ])
    assert.deepEqual(
      (
        await readGameMetadata(
          new File([await readFile(join(directory, name))], name),
        )
      ).document,
      unicode,
      name,
    );
  assert.equal((await readFile(join(directory, "launcher.bat"))).at(-3), 0x96);
});
