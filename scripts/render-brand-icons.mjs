import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import sharp from "sharp";

const directory = fileURLToPath(new URL("../src-tauri/icons/", import.meta.url));
const source = await readFile(`${directory}icon.svg`);
const sizes = [16, 24, 32, 48, 64, 128, 256, 1024];
const images = new Map();
for (const size of sizes) {
  const image = await sharp(source, { density: 384 }).resize(size, size).png().toBuffer();
  images.set(size, image);
  await writeFile(`${directory}icon-${size}.png`, image);
}

// ICO stores PNG frames at their native sizes, including 16/24/32px tray and taskbar sizes.
const frames = sizes.filter((size) => size <= 256);
const header = Buffer.alloc(6 + frames.length * 16);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(frames.length, 4);
let offset = header.length;
frames.forEach((size, index) => {
  const entry = 6 + index * 16;
  header[entry] = header[entry + 1] = size === 256 ? 0 : size;
  header.writeUInt16LE(1, entry + 4);
  header.writeUInt16LE(32, entry + 6);
  header.writeUInt32LE(images.get(size).length, entry + 8);
  header.writeUInt32LE(offset, entry + 12);
  offset += images.get(size).length;
});
await writeFile(`${directory}icon.ico`, Buffer.concat([header, ...frames.map((size) => images.get(size))]));
await writeFile(`${directory}tray-light.png`, images.get(32));
const rgba = await sharp(images.get(32)).ensureAlpha().raw().toBuffer();
await writeFile(`${directory}tray-light.rgba`, rgba);
console.log("Rendered 3D brand icons: SVG → PNG, multi-size ICO and tray RGBA.");
