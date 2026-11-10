// Merge two icon containers: layers up to 48 px come from the "small" drawing, larger ones from the full one.
//   node scripts/icons/merge.cjs ico  <full.ico>  <small.ico>  <out.ico>
//   node scripts/icons/merge.cjs icns <full.icns> <small.icns> <out.icns>
//   node scripts/icons/merge.cjs list <file.ico|file.icns>
const fs = require("fs");
const [kind, a, b, out] = process.argv.slice(2);

function readIco(p) {
  const buf = fs.readFileSync(p);
  if (buf.readUInt16LE(0) !== 0 || buf.readUInt16LE(2) !== 1) throw new Error("not an .ico: " + p);
  const n = buf.readUInt16LE(4);
  const entries = [];
  for (let i = 0; i < n; i++) {
    const o = 6 + i * 16;
    const size = buf.readUInt32LE(o + 8);
    const offset = buf.readUInt32LE(o + 12);
    entries.push({ w: buf[o] || 256, h: buf[o + 1] || 256, head: buf.subarray(o, o + 8), data: buf.subarray(offset, offset + size) });
  }
  return entries;
}

function writeIco(p, entries) {
  const head = Buffer.alloc(6 + entries.length * 16);
  head.writeUInt16LE(1, 2);
  head.writeUInt16LE(entries.length, 4);
  let offset = head.length;
  entries.forEach((e, i) => {
    const o = 6 + i * 16;
    e.head.copy(head, o);
    head.writeUInt32LE(e.data.length, o + 8);
    head.writeUInt32LE(offset, o + 12);
    offset += e.data.length;
  });
  fs.writeFileSync(p, Buffer.concat([head, ...entries.map((e) => e.data)]));
}

function readIcns(p) {
  const buf = fs.readFileSync(p);
  if (buf.toString("latin1", 0, 4) !== "icns" || buf.readUInt32BE(4) !== buf.length) throw new Error("not an .icns: " + p);
  const chunks = [];
  for (let o = 8; o < buf.length; ) {
    const len = buf.readUInt32BE(o + 4);
    chunks.push({ type: buf.toString("latin1", o, o + 4), data: buf.subarray(o, o + len) });
    o += len;
  }
  return chunks;
}

// Chunk types that hold 16 px and 32 px pictures (and their masks); everything else is 64 px or larger.
const ICNS_SMALL = new Set(["is32", "s8mk", "il32", "l8mk", "icp4", "icp5", "ic04", "ic05", "ic11"]);

if (kind === "list") {
  if (a.endsWith(".ico")) console.log(readIco(a).map((e) => `${e.w}x${e.h} ${e.data.length}B ${e.data.readUInt32BE(0) === 0x89504e47 ? "png" : "bmp"}`).join("\n"));
  else console.log(readIcns(a).map((c) => `${c.type} ${c.data.length}B`).join("\n"));
} else if (kind === "ico") {
  const full = readIco(a);
  const small = readIco(b);
  const merged = full.map((e) => (e.w <= 48 ? small.find((s) => s.w === e.w) ?? e : e));
  writeIco(out, merged);
  console.log("ico:", merged.map((e) => `${e.w}${e.w <= 48 ? "s" : "f"}`).join(" "));
} else if (kind === "icns") {
  const full = readIcns(a);
  const small = readIcns(b);
  // The chunk order is not stable between runs of the generator, so match by type.
  const types = (list) => list.map((c) => c.type).sort().join();
  if (types(full) !== types(small)) throw new Error("the two .icns files do not hold the same chunk types");
  const merged = full.map((c) => (ICNS_SMALL.has(c.type) ? small.find((x) => x.type === c.type) : c));
  const body = Buffer.concat(merged.map((c) => c.data));
  const head = Buffer.alloc(8);
  head.write("icns", 0, "latin1");
  head.writeUInt32BE(body.length + 8, 4);
  fs.writeFileSync(out, Buffer.concat([head, body]));
  console.log("icns:", merged.map((c) => `${c.type}${ICNS_SMALL.has(c.type) ? "(s)" : ""}`).join(" "));
} else {
  throw new Error("ico | icns | list");
}
