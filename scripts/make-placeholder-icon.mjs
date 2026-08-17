/**
 * 生成 1024x1024 占位应用图标（浮冰上的匣子，与 BrandMark 同源几何）。
 * 零依赖：手写 PNG 编码（zlib + CRC），2x 超采样抗锯齿。
 * 拿到正式图标后：pnpm tauri icon <正式图标.png> 一条命令替换全套。
 */
import { deflateSync } from "node:zlib";
import { writeFileSync } from "node:fs";

const SIZE = 1024;
const SS = 2; // 超采样倍数
const N = SIZE * SS;

/* ---------- SDF ---------- */
const sdRoundRect = (px, py, cx, cy, hw, hh, r) => {
  const dx = Math.abs(px - cx) - hw + r;
  const dy = Math.abs(py - cy) - hh + r;
  const ox = Math.max(dx, 0);
  const oy = Math.max(dy, 0);
  return Math.hypot(ox, oy) + Math.min(Math.max(dx, dy), 0) - r;
};

/** 图标设计（1024 坐标系，逻辑同 BrandMark） */
function paint(x, y) {
  // 返回 [r, g, b, a]
  // 背景：深冰蓝圆角方形
  const bg = sdRoundRect(x, y, 512, 512, 448, 448, 216);
  if (bg > 0) return [0, 0, 0, 0];
  const deep = [37, 102, 133, 255];

  // 匣体：冰白描边圆角方（stroke 66）
  const box = sdRoundRect(x, y, 512, 590, 344, 230, 118);
  if (Math.abs(box) <= 33) return [235, 247, 251, 255];

  // 匣口：半透明短横（只在匣体内部）
  if (box < -33) {
    const mouth = sdRoundRect(x, y, 512, 571, 152, 26, 26);
    if (mouth < 0) return [235, 247, 251, 140];
  }

  // 悬浮冰板：右上小圆角块
  const floe = sdRoundRect(x, y, 710, 227, 158, 75, 66);
  if (floe < 0) return [235, 247, 251, 255];

  return deep;
}

/* ---------- 渲染（2x 超采样） ---------- */
const rgba = Buffer.alloc(SIZE * SIZE * 4);
for (let py = 0; py < SIZE; py++) {
  for (let px = 0; px < SIZE; px++) {
    let r = 0, g = 0, b = 0, a = 0;
    for (let sy = 0; sy < SS; sy++) {
      for (let sx = 0; sx < SS; sx++) {
        const [pr, pg, pb, pa] = paint((px * SS + sx + 0.5) / SS, (py * SS + sy + 0.5) / SS);
        r += pr * pa;
        g += pg * pa;
        b += pb * pa;
        a += pa;
      }
    }
    const k = a === 0 ? 0 : 1;
    const i = (py * SIZE + px) * 4;
    rgba[i] = a ? Math.round(r / a) : 0;
    rgba[i + 1] = a ? Math.round(g / a) : 0;
    rgba[i + 2] = a ? Math.round(b / a) : 0;
    rgba[i + 3] = Math.round(a / (SS * SS));
    void k;
  }
}

/* ---------- PNG 编码 ---------- */
const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
const chunk = (type, data) => {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
};

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // RGBA
// 每行前加 filter 字节 0
const raw = Buffer.alloc(SIZE * (SIZE * 4 + 1));
for (let y = 0; y < SIZE; y++) {
  rgba.copy(raw, y * (SIZE * 4 + 1) + 1, y * SIZE * 4, (y + 1) * SIZE * 4);
}
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);
writeFileSync("app-icon.png", png);
console.log("saved app-icon.png", png.length, "bytes");
