// Generates a 1024x1024 PNG app icon (no external deps) used as the source for
// `pnpm tauri icon`. Draws a rounded-square gradient with a simple mic glyph.
import { deflateSync } from "node:zlib";
import { writeFileSync } from "node:fs";

const SIZE = 1024;
const buf = Buffer.alloc(SIZE * SIZE * 4);

function set(x, y, r, g, b, a = 255) {
  if (x < 0 || y < 0 || x >= SIZE || y >= SIZE) return;
  const i = (y * SIZE + x) * 4;
  buf[i] = r;
  buf[i + 1] = g;
  buf[i + 2] = b;
  buf[i + 3] = a;
}

const radius = 180; // rounded corners
function insideRoundedRect(x, y) {
  const min = 0;
  const max = SIZE - 1;
  const cx = Math.min(Math.max(x, min + radius), max - radius);
  const cy = Math.min(Math.max(y, min + radius), max - radius);
  return Math.hypot(x - cx, y - cy) <= radius || (x >= radius && x <= max - radius) || (y >= radius && y <= max - radius);
}

for (let y = 0; y < SIZE; y++) {
  for (let x = 0; x < SIZE; x++) {
    if (!insideRoundedRect(x, y)) {
      set(x, y, 0, 0, 0, 0); // transparent outside the rounded square
      continue;
    }
    // Diagonal purple gradient background.
    const t = (x + y) / (2 * SIZE);
    const r = Math.round(80 + t * 40);
    const g = Math.round(70 + t * 20);
    const b = Math.round(200 + t * 40);
    set(x, y, r, g, b, 255);
  }
}

// Simple white microphone glyph (capsule + stand).
const cx = SIZE / 2;
const capTop = 300;
const capBottom = 600;
const capR = 95;
for (let y = 0; y < SIZE; y++) {
  for (let x = 0; x < SIZE; x++) {
    const inCapsuleBody = x >= cx - capR && x <= cx + capR && y >= capTop && y <= capBottom;
    const inTop = Math.hypot(x - cx, y - capTop) <= capR;
    const inBottom = Math.hypot(x - cx, y - capBottom) <= capR;
    if (inCapsuleBody || inTop || inBottom) set(x, y, 255, 255, 255, 255);

    // Stand: arc + post + base.
    const arcOuter = 175;
    const arcInner = 150;
    const d = Math.hypot(x - cx, y - capBottom);
    if (y > capBottom && d <= arcOuter && d >= arcInner) set(x, y, 255, 255, 255, 255);
    if (x >= cx - 12 && x <= cx + 12 && y >= capBottom + 130 && y <= capBottom + 240)
      set(x, y, 255, 255, 255, 255);
    if (x >= cx - 90 && x <= cx + 90 && y >= capBottom + 235 && y <= capBottom + 260)
      set(x, y, 255, 255, 255, 255);
  }
}

// Encode as PNG (single IDAT, filter type 0 per scanline).
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, "ascii");
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])) >>> 0, 0);
  return Buffer.concat([len, typeBuf, data, crc]);
}

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();
function crc32(b) {
  let c = 0xffffffff;
  for (let i = 0; i < b.length; i++) c = CRC_TABLE[(c ^ b[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // color type RGBA
ihdr[10] = 0;
ihdr[11] = 0;
ihdr[12] = 0;

const raw = Buffer.alloc(SIZE * (SIZE * 4 + 1));
for (let y = 0; y < SIZE; y++) {
  raw[y * (SIZE * 4 + 1)] = 0; // filter: none
  buf.copy(raw, y * (SIZE * 4 + 1) + 1, y * SIZE * 4, (y + 1) * SIZE * 4);
}

const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);

writeFileSync(new URL("../app-icon.png", import.meta.url), png);
console.log("Wrote app-icon.png", png.length, "bytes");
