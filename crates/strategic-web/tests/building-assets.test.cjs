const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const zlib = require("node:zlib");

const services = [
  "map", "merchants", "weapons", "armor", "clothing", "herbalist", "inn", "religion",
];
const buildingRoot = path.join(
  __dirname,
  "..",
  "static",
  "styles",
  "timber-framed",
  "building",
);
const tiers = ["village", "town", "city"];
const variants = ["inland", "coastal", "river"];
const horizonRoot = path.join(buildingRoot, "..", "background");

function decodeRgbaPng(file) {
  const png = fs.readFileSync(file);
  assert.deepEqual([...png.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
  let cursor = 8;
  let width;
  let height;
  const compressed = [];
  while (cursor < png.length) {
    const length = png.readUInt32BE(cursor);
    const type = png.toString("ascii", cursor + 4, cursor + 8);
    const data = png.subarray(cursor + 8, cursor + 8 + length);
    cursor += length + 12;
    if (type === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      assert.equal(data[8], 8, "assets use eight-bit channels");
      assert.equal(data[9], 6, "assets are RGBA, not indexed or RGB-only");
      assert.equal(data[12], 0, "assets are non-interlaced for portable validation");
    } else if (type === "IDAT") {
      compressed.push(data);
    } else if (type === "IEND") {
      break;
    }
  }
  const filtered = zlib.inflateSync(Buffer.concat(compressed));
  const stride = width * 4;
  const rgba = Buffer.alloc(width * height * 4);
  let source = 0;
  const paeth = (left, above, upperLeft) => {
    const estimate = left + above - upperLeft;
    const dl = Math.abs(estimate - left);
    const da = Math.abs(estimate - above);
    const du = Math.abs(estimate - upperLeft);
    return dl <= da && dl <= du ? left : da <= du ? above : upperLeft;
  };
  for (let y = 0; y < height; y += 1) {
    const filter = filtered[source++];
    assert.ok(filter >= 0 && filter <= 4, `supported PNG filter ${filter}`);
    for (let x = 0; x < stride; x += 1) {
      const encoded = filtered[source++];
      const left = x >= 4 ? rgba[y * stride + x - 4] : 0;
      const above = y ? rgba[(y - 1) * stride + x] : 0;
      const upperLeft = y && x >= 4 ? rgba[(y - 1) * stride + x - 4] : 0;
      const predictor = filter === 1 ? left
        : filter === 2 ? above
          : filter === 3 ? Math.floor((left + above) / 2)
            : filter === 4 ? paeth(left, above, upperLeft)
              : 0;
      rgba[y * stride + x] = (encoded + predictor) & 255;
    }
  }
  return { width, height, rgba };
}

test("all settlement building backgrounds are normalized tintable RGBA assets", () => {
  const baselines = [];
  for (const tier of tiers) {
    const assetRoot = path.join(buildingRoot, tier);
    assert.deepEqual(
      fs.readdirSync(assetRoot).filter((file) => file.endsWith(".png")).sort(),
      services.map((service) => `${service}.png`).sort(),
    );
    for (const service of services) {
      const label = `${tier}/${service}`;
      const { width, height, rgba } = decodeRgbaPng(path.join(assetRoot, `${service}.png`));
      assert.equal(width, 512);
      assert.equal(height, 512);
      const cornerOffsets = [0, (width - 1) * 4, (height - 1) * width * 4, (width * height - 1) * 4];
      for (const offset of cornerOffsets) assert.equal(rgba[offset + 3], 0, `${label} corner alpha`);

      const tones = new Set();
      let visible = 0;
      let bottom = -1;
      for (let i = 0; i < rgba.length; i += 4) {
        const alpha = rgba[i + 3];
        if (!alpha) continue;
        const [red, green, blue] = [rgba[i], rgba[i + 1], rgba[i + 2]];
        assert.equal(red, green, `${label} has no colored fringe`);
        assert.equal(green, blue, `${label} is grayscale`);
        tones.add(red);
        visible += 1;
        bottom = Math.max(bottom, Math.floor(i / 4 / width));
      }
      assert.deepEqual([...tones].sort((a, b) => a - b), [24, 112, 220]);
      assert.ok(visible > width * height * 0.08, `${label} has useful visible coverage`);
      assert.ok(visible < width * height * 0.65, `${label} retains transparent padding`);
      if (tier !== "village") {
        let centralSupports = 0;
        for (let y = 322; y < 450; y += 1) {
          for (let x = 188; x < 324; x += 1) {
            const offset = (y * width + x) * 4;
            if (rgba[offset + 3] && rgba[offset] === 112) centralSupports += 1;
          }
        }
        assert.equal(centralSupports, 0, `${label} keeps the overlaid icon field free of structural beams`);

        for (let y = 260; y < bottom - 3; y += 1) {
          for (let x = 0; x < width; x += 1) {
            const offset = (y * width + x) * 4;
            if (!rgba[offset + 3] || rgba[offset] !== 112) continue;
            let touchesSilhouetteEdge = false;
            for (let yy = Math.max(0, y - 24); yy <= Math.min(height - 1, y + 24); yy += 1) {
              for (let xx = Math.max(0, x - 24); xx <= Math.min(width - 1, x + 24); xx += 1) {
                if (!rgba[(yy * width + xx) * 4 + 3]) touchesSilhouetteEdge = true;
              }
            }
            assert.ok(touchesSilhouetteEdge, `${label} has no interior secondary-tone beam at ${x},${y}`);
          }
        }
      }
      baselines.push(bottom);
    }
  }
  assert.ok(Math.max(...baselines) - Math.min(...baselines) <= 1, "shared bottom baseline");
});

test("all settlement horizons are standardized transparent panoramic assets", () => {
  for (const tier of tiers) {
    for (const variant of variants) {
      const label = `${tier}/${variant}`;
      const { width, height, rgba } = decodeRgbaPng(path.join(horizonRoot, tier, `${variant}.png`));
      assert.equal(width, 2880, `${label} width`);
      assert.equal(height, 240, `${label} height`);
      assert.equal(width / height, 12, `${label} aspect ratio`);

      let visible = 0;
      for (let y = 0; y < height; y += 1) {
        for (let x = 0; x < width; x += 1) {
          const alpha = rgba[(y * width + x) * 4 + 3];
          if (y === 0) assert.equal(alpha, 0, `${label} upper sky is transparent`);
          if (alpha) {
            assert.equal(rgba[(y * width + x) * 4], rgba[(y * width + x) * 4 + 1], `${label} no green fringe`);
            assert.equal(rgba[(y * width + x) * 4 + 1], rgba[(y * width + x) * 4 + 2], `${label} grayscale`);
          }
          if (alpha) visible += 1;
        }
      }
      assert.ok(visible > width * height * 0.05, `${label} useful scenery`);
      assert.ok(visible < width * height * 0.65, `${label} keeps sky transparent`);
      for (const x of [0, width - 1]) {
        assert.ok(rgba[((height - 1) * width + x) * 4 + 3] > 0, `${label} reaches bottom corner`);
      }
      for (const [side, startX, endX] of [
        ["left", 0, Math.floor(width / 6)],
        ["right", Math.floor(width * 5 / 6), width],
      ]) {
        let sideVisible = 0;
        const sideTones = new Set();
        const startY = 160;
        const endY = 216;
        for (let y = startY; y < endY; y += 1) {
          for (let x = startX; x < endX; x += 1) {
            const offset = (y * width + x) * 4;
            if (!rgba[offset + 3]) continue;
            sideVisible += 1;
            sideTones.add(rgba[offset]);
          }
        }
        const sideArea = (endX - startX) * (endY - startY);
        assert.ok(sideVisible > sideArea * 0.25, `${label} has meaningful ${side} edge scenery above filler`);
        assert.ok(sideTones.size >= 3, `${label} ${side} edge scenery has layered tonal detail`);
      }
    }
  }
});

test("town and city horizons carry an irregular built skyline through both crop edges", () => {
  for (const tier of ["town", "city"]) {
    for (const variant of variants) {
      const { width, rgba } = decodeRgbaPng(path.join(horizonRoot, tier, `${variant}.png`));
      for (const [side, startX, endX] of [
        ["left", 0, Math.floor(width / 6)],
        ["right", Math.floor(width * 5 / 6), width],
      ]) {
        const roofline = [];
        for (let x = startX; x < endX; x += 4) {
          let top = 240;
          for (let y = 48; y < 180; y += 1) {
            if (rgba[(y * width + x) * 4 + 3]) {
              top = y;
              break;
            }
          }
          roofline.push(top);
        }
        assert.ok(Math.min(...roofline) < 130, `${tier}/${variant} ${side} edge contains nearby roofs`);
        assert.ok(
          Math.max(...roofline) - Math.min(...roofline) > 35,
          `${tier}/${variant} ${side} edge is architecture rather than a flat filler band`,
        );
      }
    }
  }
});
