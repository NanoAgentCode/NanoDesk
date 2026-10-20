import { readFile } from "node:fs/promises";
import sharp from "sharp";
import { describe, expect, it } from "vitest";

const asset = (name: string) => readFile(new URL(`../../src-tauri/icons/${name}`, import.meta.url));
describe("Windows brand icon assets", () => {
  it("contains decodable native-size ICO frames for desktop and small Windows surfaces", async () => {
    const ico = await asset("icon.ico");
    expect(ico.readUInt16LE(0)).toBe(0);
    expect(ico.readUInt16LE(2)).toBe(1);
    expect(ico.readUInt16LE(4)).toBe(7);
    const sizes = [];
    for (let i = 0; i < 7; i++) {
      const entry = 6 + i * 16;
      const size = ico[entry] || 256;
      const length = ico.readUInt32LE(entry + 8), offset = ico.readUInt32LE(entry + 12);
      const metadata = await sharp(ico.subarray(offset, offset + length)).metadata();
      expect([metadata.width, metadata.height]).toEqual([size, size]);
      sizes.push(size);
    }
    expect(sizes).toEqual([16, 24, 32, 48, 64, 128, 256]);
  });
  it("uses the same colored brand image in the tray with a valid 32×32 RGBA buffer", async () => {
    const rgba = await asset("tray-light.rgba");
    const decoded = await sharp(await asset("tray-light.png")).ensureAlpha().raw().toBuffer();
    expect(rgba.length).toBe(32 * 32 * 4);
    expect(rgba.equals(decoded)).toBe(true);
    expect((await asset("tray-light.png")).equals(await asset("icon-32.png"))).toBe(true);
    expect(rgba[3]).toBe(0);
    expect(rgba[(16 * 32 + 16) * 4 + 3]).toBe(255);
  });
});
