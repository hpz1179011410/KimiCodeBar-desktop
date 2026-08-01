// 生成 KimiCodeBar 全套 PNG 图标：蓝底圆角矩形 + 白色 K（仿截图设计）
// 无需第三方依赖：手写 PNG 编码 + zlib；几何图形用距离场做抗锯齿
// 用法: node scripts/generate-icons.mjs
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri", "icons");
mkdirSync(root, { recursive: true });

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const out = Buffer.alloc(12 + data.length);
  out.writeUInt32BE(data.length, 0);
  out.write(type, 4, "ascii");
  data.copy(out, 8);
  out.writeUInt32BE(crc32(Buffer.concat([Buffer.from(type, "ascii"), data])), 8 + data.length);
  return out;
}

function makePng(size, pixelAt) {
  const sig = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // color type RGBA
  const raw = Buffer.alloc(size * (size * 4 + 1));
  for (let y = 0; y < size; y++) {
    const rowStart = y * (size * 4 + 1);
    raw[rowStart] = 0; // filter: none
    for (let x = 0; x < size; x++) {
      const [r, g, b, a] = pixelAt(x, y, size);
      const p = rowStart + 1 + x * 4;
      raw[p] = r;
      raw[p + 1] = g;
      raw[p + 2] = b;
      raw[p + 3] = a;
    }
  }
  return Buffer.concat([
    sig,
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

// ---- 几何工具（归一化坐标 0..1） ----

// 点到线段的距离（胶囊形笔画，天然圆头）
function segDist(px, py, x1, y1, x2, y2) {
  const dx = x2 - x1;
  const dy = y2 - y1;
  const len2 = dx * dx + dy * dy;
  let t = len2 === 0 ? 0 : ((px - x1) * dx + (py - y1) * dy) / len2;
  t = Math.max(0, Math.min(1, t));
  return Math.hypot(px - (x1 + t * dx), py - (y1 + t * dy));
}

// 圆角矩形符号距离（<0 在内部）
function roundedRectDist(nx, ny, half, radius) {
  const qx = Math.abs(nx - 0.5) - (half - radius);
  const qy = Math.abs(ny - 0.5) - (half - radius);
  const ax = Math.max(qx, 0);
  const ay = Math.max(qy, 0);
  return Math.hypot(ax, ay) + Math.min(Math.max(qx, qy), 0) - radius;
}

// "K" 的三笔（归一化坐标）
const K_SEGMENTS = [
  [0.33, 0.24, 0.33, 0.76], // 竖
  [0.67, 0.24, 0.35, 0.51], // 上斜
  [0.37, 0.49, 0.68, 0.76], // 下斜
];
const K_STROKE = 0.058; // 笔画半径（归一化）

function kDist(nx, ny) {
  let d = Infinity;
  for (const [x1, y1, x2, y2] of K_SEGMENTS) {
    d = Math.min(d, segDist(nx, ny, x1, y1, x2, y2));
  }
  return d;
}

// 生成某配色方案的像素函数
function iconPixel(topColor, bottomColor) {
  const RADIUS = 0.22; // 圆角（归一化）
  return (x, y, size) => {
    // 归一化到 0..1，并留出 1px 边缘用于抗锯齿
    const nx = (x + 0.5) / size;
    const ny = (y + 0.5) / size;
    const aa = 0.75 / size; // 抗锯齿过渡带（归一化）

    // 圆角矩形背景
    const dRect = roundedRectDist(nx, ny, 0.5, RADIUS);
    if (dRect > aa) return [0, 0, 0, 0];
    const bgAlpha = Math.min(1, Math.max(0, (aa - dRect) / (2 * aa)));

    // 垂直渐变
    const t = ny;
    const bg = [
      Math.round(topColor[0] + (bottomColor[0] - topColor[0]) * t),
      Math.round(topColor[1] + (bottomColor[1] - topColor[1]) * t),
      Math.round(topColor[2] + (bottomColor[2] - topColor[2]) * t),
    ];

    // K 覆盖度
    const dK = kDist(nx, ny) - K_STROKE;
    const kAlpha = Math.min(1, Math.max(0, (aa - dK) / (2 * aa)));

    // K 白色叠加在背景上
    const r = Math.round(bg[0] + (255 - bg[0]) * kAlpha);
    const g = Math.round(bg[1] + (255 - bg[1]) * kAlpha);
    const b = Math.round(bg[2] + (255 - bg[2]) * kAlpha);
    return [r, g, b, Math.round(bgAlpha * 255)];
  };
}

const NORMAL_TOP = [111, 162, 255]; // 截图蓝：上浅
const NORMAL_BOTTOM = [62, 114, 246]; // 下深
const WARN_TOP = [255, 122, 110];
const WARN_BOTTOM = [224, 58, 48];

const normal = iconPixel(NORMAL_TOP, NORMAL_BOTTOM);
const warn = iconPixel(WARN_TOP, WARN_BOTTOM);

writeFileSync(join(root, "tray-normal.png"), makePng(32, normal));
writeFileSync(join(root, "tray-warn.png"), makePng(32, warn));
writeFileSync(join(root, "32x32.png"), makePng(32, normal));
writeFileSync(join(root, "128x128.png"), makePng(128, normal));
writeFileSync(join(root, "128x128@2x.png"), makePng(256, normal));
writeFileSync(join(root, "icon.png"), makePng(512, normal));

// icon.ico：PNG 直接嵌入（Vista+ 支持）
const png = readFileSync(join(root, "icon.png"));
const w = png.readUInt32BE(16);
const h = png.readUInt32BE(20);
const header = Buffer.alloc(6);
header.writeUInt16LE(0, 0);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(1, 4);
const entry = Buffer.alloc(16);
entry.writeUInt8(w >= 256 ? 0 : w, 0);
entry.writeUInt8(h >= 256 ? 0 : h, 1);
entry.writeUInt16LE(1, 4);
entry.writeUInt16LE(32, 6);
entry.writeUInt32LE(png.length, 8);
entry.writeUInt32LE(22, 12);
writeFileSync(join(root, "icon.ico"), Buffer.concat([header, entry, png]));

// icon.icns：macOS 打包用。现代 icns 容器直接内嵌 PNG：
// icns 头（magic + 总长度，大端）+ 若干条目（类型 + 条目长度 + PNG 数据）。
// ic09 = 512x512@1x，ic10 = 512x512@2x（此处都内嵌同一张 512 PNG，macOS 可正常解析）。
function icnsEntry(type, pngBuf) {
  const head = Buffer.alloc(8);
  head.write(type, 0, "ascii");
  head.writeUInt32BE(8 + pngBuf.length, 4);
  return Buffer.concat([head, pngBuf]);
}
const icnsBody = Buffer.concat([icnsEntry("ic09", png), icnsEntry("ic10", png)]);
const icnsHeader = Buffer.alloc(8);
icnsHeader.write("icns", 0, "ascii");
icnsHeader.writeUInt32BE(8 + icnsBody.length, 4);
writeFileSync(join(root, "icon.icns"), Buffer.concat([icnsHeader, icnsBody]));

console.log("icons written to", root);
