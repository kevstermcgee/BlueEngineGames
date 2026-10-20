// Portable game details are local draft hints. Account and release authority
// always come from the collaboration service, never from an imported file.
export const MAX_METADATA_BYTES = 60 * 1024;
export const METADATA_MAGIC = "BLUEENGINE_GAME_METADATA_V1\n";
export const METADATA_MANIFEST = "blueengine.game.json";
const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
const magic = encoder.encode(METADATA_MAGIC);
const FOOTER = 32 + 8 + magic.length;
const MAX_DIRECTORY = 16 * 1024 * 1024;
const MAX_ENTRIES = 20000;
const fail = (message) => {
  throw new Error(message);
};
const object = (v) => v !== null && typeof v === "object" && !Array.isArray(v);
const text = (v, maximum, required = false) =>
  typeof v === "string" &&
  [...v].length <= maximum &&
  (!required || v.trim().length > 0);
const slug = (v) => text(v, 100, true) && /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(v);
const repository = (v) =>
  text(v, 200, true) && /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(v);
const commit = (v) =>
  typeof v === "string" && /^(?:[0-9a-f]{40}|[0-9a-f]{64})$/i.test(v);
function portablePath(value) {
  if (!text(value, 512, true) || /[\\:"\x00-\x1f\x7f*?<>|]/.test(value))
    return false;
  if (value === ".") return true;
  return value
    .split("/")
    .every(
      (part) =>
        part &&
        part !== "." &&
        part !== ".." &&
        !/[. ]$/.test(part) &&
        !/^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(part),
    );
}
function generic(value, depth = 0) {
  if (depth > 24) fail("Game metadata is nested too deeply.");
  if (value === null || typeof value === "boolean") return;
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value))
      fail("Game metadata contains an unsupported number.");
    return;
  }
  if (typeof value === "string") {
    if ([...value].length > 16000 || value.includes("\0"))
      fail("Game metadata contains invalid text.");
    for (let i = 0; i < value.length; i++) {
      const c = value.charCodeAt(i);
      if (c >= 0xd800 && c <= 0xdbff) {
        const next = value.charCodeAt(++i);
        if (!(next >= 0xdc00 && next <= 0xdfff))
          fail("Game metadata contains invalid Unicode.");
      } else if (c >= 0xdc00 && c <= 0xdfff)
        fail("Game metadata contains invalid Unicode.");
    }
    return;
  }
  if (Array.isArray(value)) {
    if (value.length > 256) fail("Game metadata has too many list entries.");
    value.forEach((entry) => generic(entry, depth + 1));
    return;
  }
  if (
    !object(value) ||
    ![Object.prototype, null].includes(Object.getPrototypeOf(value))
  )
    fail("Game metadata must contain ordinary JSON values.");
  const keys = Object.keys(value);
  if (keys.length > 256) fail("Game metadata has too many fields.");
  for (const key of keys) {
    if (!/^[A-Za-z0-9_.-]{1,128}$/.test(key))
      fail("Game metadata contains an invalid field name.");
    generic(value[key], depth + 1);
  }
}
export function validateGameMetadata(document) {
  generic(document);
  if (
    !object(document) ||
    document.format !== "blueengine.game" ||
    document.schema_version !== 1 ||
    !object(document.game)
  )
    fail("This file uses an unsupported game metadata format.");
  const game = document.game;
  if (
    !slug(game.slug) ||
    !text(game.title, 160, true) ||
    !text(game.version, 128) ||
    !text(game.description, 16000) ||
    !text(game.controls, 8000) ||
    ![game.authors, game.tags].every(
      (list) =>
        Array.isArray(list) &&
        list.length <= 32 &&
        list.every((v) => text(v, 160, true)),
    )
  )
    fail(
      "Game metadata is missing valid identity, description, controls or credits.",
    );
  for (const key of ["source", "engine"]) {
    const source = document[key];
    if (source === null) continue;
    if (
      !object(source) ||
      !repository(source.repository) ||
      !commit(source.commit) ||
      (key === "source" &&
        (!portablePath(source.game_path) ||
          ("dirty" in source && typeof source.dirty !== "boolean")))
    )
      fail("Game metadata has an invalid source pin.");
  }
  if (document.lineage !== undefined) {
    if (!object(document.lineage))
      fail("Game metadata has invalid fork history.");
    for (const key of [
      "root_game",
      "parent_game",
      "parent_revision",
      "revision_id",
    ])
      if (
        document.lineage[key] !== undefined &&
        document.lineage[key] !== null &&
        !text(document.lineage[key], 160)
      )
        fail("Game metadata has invalid fork history.");
  }
  if (document.wiki !== undefined && !object(document.wiki))
    fail("Game metadata has an invalid guide.");
  return document;
}
export function canonicalMetadata(value) {
  if (Array.isArray(value))
    return "[" + value.map(canonicalMetadata).join(",") + "]";
  if (object(value))
    return (
      "{" +
      Object.keys(value)
        .sort()
        .map((k) => JSON.stringify(k) + ":" + canonicalMetadata(value[k]))
        .join(",") +
      "}"
    );
  return JSON.stringify(value);
}
function decodePayload(bytes) {
  if (!bytes.length || bytes.length > MAX_METADATA_BYTES)
    fail("Game metadata exceeds the supported size.");
  let value, raw;
  try {
    raw = decoder.decode(bytes);
    value = JSON.parse(raw);
  } catch {
    fail("Game metadata is not valid UTF-8 JSON.");
  }
  validateGameMetadata(value);
  if (canonicalMetadata(value) !== raw)
    fail("Game metadata is not in its canonical format.");
  return value;
}
const view = (bytes) =>
  new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
function u64(bytes, offset) {
  const value = view(bytes).getBigUint64(offset, true);
  if (value > BigInt(Number.MAX_SAFE_INTEGER))
    fail("The game file has an unsupported size.");
  return Number(value);
}
const equal = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);
async function envelope(bytes) {
  if (bytes.length < FOOTER || !equal(bytes.subarray(-magic.length), magic))
    fail(
      "No portable game metadata was found. Export this game with the current BlueEngine tools, then drop the exported file here.",
    );
  const length = u64(bytes, bytes.length - magic.length - 8);
  if (
    !length ||
    length > MAX_METADATA_BYTES ||
    length + FOOTER !== bytes.length
  )
    fail("The embedded game metadata is truncated or oversized.");
  const payload = bytes.subarray(0, length);
  const checksum = new Uint8Array(
    await globalThis.crypto.subtle.digest("SHA-256", payload),
  );
  if (!equal(checksum, bytes.subarray(length, length + 32)))
    fail("The embedded game metadata checksum does not match.");
  return { document: decodePayload(payload), payload };
}
function reader(file) {
  if (
    !file ||
    !Number.isSafeInteger(file.size) ||
    file.size <= 0 ||
    typeof file.slice !== "function"
  )
    fail("Choose one regular game file.");
  return async (offset, length) => {
    if (
      !Number.isSafeInteger(offset) ||
      !Number.isSafeInteger(length) ||
      offset < 0 ||
      length < 0 ||
      length > MAX_DIRECTORY ||
      offset + length > file.size
    )
      fail("The game file contains invalid metadata offsets.");
    const result = new Uint8Array(
      await file.slice(offset, offset + length).arrayBuffer(),
    );
    if (result.length !== length)
      fail("The game file changed while it was being read.");
    return result;
  };
}
async function trailerAt(read, end, padding = 0) {
  const tail = await read(
    Math.max(0, end - MAX_METADATA_BYTES - FOOTER - padding),
    Math.min(end, MAX_METADATA_BYTES + FOOTER + padding),
  );
  const matches = [];
  for (let pad = 0; pad <= padding; pad++) {
    const stop = tail.length - pad;
    if (
      stop < FOOTER ||
      !tail.subarray(stop).every((b) => b === 0) ||
      !equal(tail.subarray(stop - magic.length, stop), magic)
    )
      continue;
    const length = u64(tail, stop - magic.length - 8);
    if (!length || length > MAX_METADATA_BYTES || length + FOOTER > stop)
      fail("The embedded game metadata is truncated or oversized.");
    matches.push({
      ...(await envelope(tail.subarray(stop - length - FOOTER, stop))),
      padding: pad,
    });
  }
  if (matches.length !== 1)
    fail(
      "No unambiguous portable game metadata was found. Re-export this game with the current BlueEngine tools.",
    );
  return matches[0];
}
async function executable(file, read) {
  const dos = await read(0, Math.min(file.size, 64));
  if (dos.length < 64 || dos[0] !== 77 || dos[1] !== 90)
    fail("The selected EXE is not a supported Windows executable.");
  const pe = view(dos).getUint32(60, true);
  if (pe < 64 || pe > 1024 * 1024 || pe + 24 > file.size)
    fail("The executable has an invalid PE header.");
  const header = await read(pe, 24),
    h = view(header);
  const count = h.getUint16(6, true),
    optionalSize = h.getUint16(20, true);
  if (
    h.getUint32(0, true) !== 0x4550 ||
    count < 1 ||
    count > 96 ||
    optionalSize < 96 ||
    optionalSize > 4096
  )
    fail("The executable has an invalid PE layout.");
  const optional = await read(pe + 24, optionalSize),
    o = view(optional),
    kind = o.getUint16(0, true);
  const directory = kind === 0x10b ? 96 : kind === 0x20b ? 112 : 0;
  if (
    !directory ||
    optionalSize < directory ||
    o.getUint32(directory - 4, true) < 5 ||
    optionalSize < directory + 40
  )
    fail("The executable has an invalid security directory.");
  const certificateOffset = o.getUint32(directory + 32, true),
    certificateSize = o.getUint32(directory + 36, true);
  const tableStart = pe + 24 + optionalSize,
    table = await read(tableStart, count * 40);
  let sectionEnd = tableStart + table.length;
  const spans = [];
  for (let i = 0; i < count; i++) {
    const size = view(table).getUint32(i * 40 + 16, true),
      start = view(table).getUint32(i * 40 + 20, true);
    if (!size) continue;
    if (
      start < tableStart + table.length ||
      start + size > file.size ||
      spans.some(([a, b]) => start < b && start + size > a)
    )
      fail("The executable has overlapping or invalid sections.");
    spans.push([start, start + size]);
    sectionEnd = Math.max(sectionEnd, start + size);
  }
  if (!certificateOffset && !certificateSize) {
    const result = await trailerAt(read, file.size);
    if (file.size - result.payload.length - FOOTER < sectionEnd)
      fail("Game metadata overlaps executable code.");
    return result;
  }
  if (
    !certificateOffset ||
    !certificateSize ||
    certificateOffset % 8 ||
    certificateOffset < sectionEnd ||
    certificateOffset + certificateSize !== file.size ||
    certificateSize > MAX_DIRECTORY
  )
    fail("The executable has an invalid Authenticode certificate table.");
  const certificates = await read(certificateOffset, certificateSize),
    c = view(certificates);
  let offset = 0;
  while (offset < certificates.length) {
    if (offset + 8 > certificates.length)
      fail("The executable certificate table is truncated.");
    const length = c.getUint32(offset, true),
      revision = c.getUint16(offset + 4, true);
    if (
      length < 8 ||
      ![0x100, 0x200].includes(revision) ||
      offset + length > certificates.length
    )
      fail("The executable certificate table is invalid.");
    offset += Math.ceil(length / 8) * 8;
  }
  if (offset !== certificates.length)
    fail("The executable certificate table is misaligned.");
  const result = await trailerAt(read, certificateOffset, 7);
  if (
    certificateOffset - result.payload.length - FOOTER - result.padding <
    sectionEnd
  )
    fail("Game metadata overlaps executable code.");
  return result;
}
async function batch(file, read) {
  const bytes = await read(0, Math.min(file.size, MAX_METADATA_BYTES * 2));
  // Only the initial comment block is ASCII. A legacy script body can use its
  // Windows code page and is neither decoded as UTF-8 nor interpreted here.
  const raw = new TextDecoder("windows-1252").decode(bytes);
  const begin = "@rem BLUEENGINE_GAME_METADATA_V1_BEGIN",
    end = "@rem BLUEENGINE_GAME_METADATA_V1_END";
  const lines = raw.split(/\r?\n/);
  if (lines.shift() !== begin)
    fail(
      "This batch file has no portable game metadata. Re-export it with the current BlueEngine tools.",
    );
  const stop = lines.indexOf(end);
  if (
    stop < 0 ||
    lines
      .slice(stop + 1)
      .some((line) => line.includes(begin) || line.includes(end))
  )
    fail("The batch metadata block is truncated or duplicated.");
  lines.splice(stop);
  if (
    !lines.length ||
    lines.some((line) => !/^@rem [A-Za-z0-9+/=]{1,76}$/.test(line))
  )
    fail("The batch metadata encoding is invalid.");
  const encoded = lines.map((line) => line.slice(5)).join("");
  if (
    !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(
      encoded,
    )
  )
    fail("The batch metadata encoding is invalid.");
  let payload;
  try {
    payload = Uint8Array.from(globalThis.atob(encoded), (c) => c.charCodeAt(0));
  } catch {
    fail("The batch metadata encoding is invalid.");
  }
  return { document: decodePayload(payload), payload };
}
function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let i = 0; i < 8; i++) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
  }
  return (crc ^ 0xffffffff) >>> 0;
}
async function archive(file, read) {
  const tail = await read(
    Math.max(0, file.size - 65557),
    Math.min(file.size, 65557),
  );
  let eocd = -1;
  for (let p = tail.length - 22; p >= 0; p--)
    if (
      view(tail).getUint32(p, true) === 0x06054b50 &&
      p + 22 + view(tail).getUint16(p + 20, true) === tail.length
    ) {
      eocd = p;
      break;
    }
  if (eocd < 0) fail("The ZIP directory is missing or truncated.");
  for (let at = eocd + 22; at + 4 <= tail.length; at++)
    if (view(tail).getUint32(at, true) === 0x06054b50)
      fail("The ZIP comment contains an ambiguous end-of-directory marker.");
  const e = view(tail),
    absolute = file.size - tail.length + eocd;
  if (e.getUint16(eocd + 4, true) || e.getUint16(eocd + 6, true))
    fail("Multi-part ZIP files are not supported.");
  let entries = e.getUint16(eocd + 10, true),
    directorySize = e.getUint32(eocd + 12, true),
    directoryOffset = e.getUint32(eocd + 16, true);
  if (entries !== e.getUint16(eocd + 8, true))
    fail("The ZIP directory has inconsistent entry counts.");
  if (
    entries === 65535 ||
    directorySize === 0xffffffff ||
    directoryOffset === 0xffffffff
  ) {
    if (absolute < 20) fail("The ZIP64 locator is missing.");
    const locator = await read(absolute - 20, 20),
      l = view(locator);
    if (
      l.getUint32(0, true) !== 0x07064b50 ||
      l.getUint32(4, true) ||
      l.getUint32(16, true) !== 1
    )
      fail("The ZIP64 locator is invalid.");
    const record = await read(u64(locator, 8), 56),
      z = view(record);
    if (
      z.getUint32(0, true) !== 0x06064b50 ||
      u64(record, 4) < 44 ||
      z.getUint32(16, true) ||
      z.getUint32(20, true) ||
      u64(record, 24) !== u64(record, 32)
    )
      fail("The ZIP64 directory is invalid.");
    entries = u64(record, 32);
    directorySize = u64(record, 40);
    directoryOffset = u64(record, 48);
  }
  if (
    !entries ||
    entries > MAX_ENTRIES ||
    directorySize > MAX_DIRECTORY ||
    directoryOffset + directorySize > absolute
  )
    fail("The ZIP directory exceeds safe inspection limits.");
  const directory = await read(directoryOffset, directorySize),
    d = view(directory),
    seen = new Set();
  let p = 0,
    manifest;
  for (let i = 0; i < entries; i++) {
    if (p + 46 > directory.length || d.getUint32(p, true) !== 0x02014b50)
      fail("The ZIP central directory is invalid.");
    const flags = d.getUint16(p + 8, true),
      method = d.getUint16(p + 10, true),
      nameSize = d.getUint16(p + 28, true),
      extraSize = d.getUint16(p + 30, true),
      commentSize = d.getUint16(p + 32, true);
    const next = p + 46 + nameSize + extraSize + commentSize;
    if (next > directory.length || flags & 1 || d.getUint16(p + 34, true))
      fail("Encrypted or malformed ZIP entries are not supported.");
    let name;
    try {
      name = decoder.decode(directory.subarray(p + 46, p + 46 + nameSize));
    } catch {
      fail("ZIP paths must use UTF-8.");
    }
    const normalized = name.replace(/\/$/, "");
    if (
      normalized === "." ||
      !portablePath(normalized) ||
      seen.has(normalized.toLowerCase())
    )
      fail("The ZIP contains unsafe or duplicate paths.");
    seen.add(normalized.toLowerCase());
    let packed = d.getUint32(p + 20, true),
      unpacked = d.getUint32(p + 24, true),
      local = d.getUint32(p + 42, true);
    if ([packed, unpacked, local].includes(0xffffffff)) {
      let cursor = p + 46 + nameSize,
        found = false;
      const limit = cursor + extraSize;
      while (cursor + 4 <= limit) {
        const key = d.getUint16(cursor, true),
          length = d.getUint16(cursor + 2, true);
        cursor += 4;
        if (cursor + length > limit) fail("The ZIP64 entry is truncated.");
        if (key === 1) {
          let at = cursor;
          for (const key of ["unpacked", "packed", "local"]) {
            const needed =
              key === "unpacked" ? unpacked : key === "packed" ? packed : local;
            if (needed !== 0xffffffff) continue;
            if (at + 8 > cursor + length)
              fail("The ZIP64 entry is missing a size.");
            const value = u64(directory, at);
            at += 8;
            if (key === "unpacked") unpacked = value;
            else if (key === "packed") packed = value;
            else local = value;
          }
          found = true;
        }
        cursor += length;
      }
      if (!found) fail("The ZIP64 entry is missing its size extension.");
    }
    if (local + 30 + packed > directoryOffset)
      fail("A ZIP entry has invalid bounds.");
    if (normalized.toLowerCase() === METADATA_MANIFEST) {
      const unixKind = (d.getUint32(p + 38, true) >>> 16) & 0o170000;
      if (
        name !== METADATA_MANIFEST ||
        method !== 0 ||
        packed !== unpacked ||
        unpacked > MAX_METADATA_BYTES ||
        ![0, 0o100000].includes(unixKind) ||
        d.getUint32(p + 38, true) & 0x10
      )
        fail(
          "The ZIP metadata manifest must be one small regular root-level stored JSON file.",
        );
      manifest = { local, packed, crc: d.getUint32(p + 16, true), flags };
    }
    p = next;
  }
  if (p !== directory.length || !manifest)
    fail(
      "No portable root metadata manifest was found. Re-export this ZIP with the current BlueEngine tools.",
    );
  const local = await read(manifest.local, 30),
    l = view(local);
  if (
    l.getUint32(0, true) !== 0x04034b50 ||
    l.getUint16(8, true) !== 0 ||
    l.getUint16(6, true) !== manifest.flags
  )
    fail("The ZIP metadata local header is invalid.");
  const nameLength = l.getUint16(26, true),
    extraLength = l.getUint16(28, true);
  const name = await read(manifest.local + 30, nameLength);
  if (!equal(name, encoder.encode(METADATA_MANIFEST)))
    fail("The ZIP metadata local name conflicts with its directory.");
  const dataOffset = manifest.local + 30 + nameLength + extraLength;
  if (dataOffset + manifest.packed > directoryOffset)
    fail("The ZIP metadata overlaps its directory.");
  const payload = await read(dataOffset, manifest.packed);
  if (crc32(payload) !== manifest.crc)
    fail("The ZIP metadata checksum does not match.");
  const result = { document: decodePayload(payload), payload };
  const comment = tail.subarray(eocd + 22);
  if (!comment.length)
    fail(
      "The ZIP is missing its portable metadata envelope. Re-export this package with the current BlueEngine tools.",
    );
  const embedded = await envelope(comment);
  if (!equal(embedded.payload, payload))
    fail("The ZIP manifest and embedded metadata disagree.");
  return result;
}
export async function readGameMetadata(file) {
  const read = reader(file),
    name = String(file.name || "");
  let result, kind;
  if (/\.exe$/i.test(name)) {
    result = await executable(file, read);
    kind = "Windows executable";
  } else if (/\.(?:bat|cmd)$/i.test(name)) {
    result = await batch(file, read);
    kind = "Windows launcher script";
  } else if (/\.zip$/i.test(name)) {
    result = await archive(file, read);
    kind = "ZIP package";
  } else
    fail(
      "Choose an exported EXE, BAT, CMD or ZIP game file. Other archive formats need a fresh BlueEngine export as ZIP.",
    );
  return { document: result.document, kind, filename: name };
}
