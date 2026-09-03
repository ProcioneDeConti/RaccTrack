// Signed-distance-field generation for MapLibre SDF icons, so `icon-color`,
// `icon-halo-color`, `icon-halo-width` and `icon-halo-blur` all work.
//
// This is the Felzenszwalb & Huttenlocher distance transform, the same approach
// @mapbox/tiny-sdf uses for glyphs — applied here to arbitrary canvas shapes.

const INF = 1e20;

function edt1d(
  f: Float64Array,
  d: Float64Array,
  v: Int16Array,
  z: Float64Array,
  n: number,
) {
  v[0] = 0;
  z[0] = -INF;
  z[1] = INF;
  for (let q = 1, k = 0; q < n; q++) {
    let s =
      (f[q] + q * q - (f[v[k]] + v[k] * v[k])) / (2 * q - 2 * v[k]);
    while (s <= z[k]) {
      k--;
      s = (f[q] + q * q - (f[v[k]] + v[k] * v[k])) / (2 * q - 2 * v[k]);
    }
    k++;
    v[k] = q;
    z[k] = s;
    z[k + 1] = INF;
  }
  for (let q = 0, k = 0; q < n; q++) {
    while (z[k + 1] < q) k++;
    d[q] = (q - v[k]) * (q - v[k]) + f[v[k]];
  }
}

function edt(
  grid: Float64Array,
  width: number,
  height: number,
  f: Float64Array,
  d: Float64Array,
  v: Int16Array,
  z: Float64Array,
) {
  for (let x = 0; x < width; x++) {
    for (let y = 0; y < height; y++) f[y] = grid[y * width + x];
    edt1d(f, d, v, z, height);
    for (let y = 0; y < height; y++) grid[y * width + x] = d[y];
  }
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) f[x] = grid[y * width + x];
    edt1d(f, d, v, z, width);
    for (let x = 0; x < width; x++) grid[y * width + x] = Math.sqrt(d[x]);
  }
}

/**
 * Turn a coverage bitmap (alpha 0..255 per pixel) into an RGBA SDF image
 * suitable for `map.addImage(id, img, { sdf: true })`.
 */
export function alphaToSdf(
  alpha: Uint8ClampedArray,
  width: number,
  height: number,
  radius = 8,
  cutoff = 0.25,
): ImageData {
  const size = width * height;
  const gridOuter = new Float64Array(size);
  const gridInner = new Float64Array(size);
  const dim = Math.max(width, height);
  const f = new Float64Array(dim);
  const d = new Float64Array(dim);
  const z = new Float64Array(dim + 1);
  const v = new Int16Array(dim);

  for (let i = 0; i < size; i++) {
    const a = alpha[i] / 255;
    gridOuter[i] =
      a === 1 ? 0 : a === 0 ? INF : Math.max(0, 0.5 - a) ** 2;
    gridInner[i] =
      a === 1 ? INF : a === 0 ? 0 : Math.max(0, a - 0.5) ** 2;
  }

  edt(gridOuter, width, height, f, d, v, z);
  edt(gridInner, width, height, f, d, v, z);

  const out = new Uint8ClampedArray(size * 4);
  for (let i = 0; i < size; i++) {
    const dist = gridOuter[i] - gridInner[i];
    const val = Math.round(255 - 255 * (dist / radius + cutoff));
    const c = val < 0 ? 0 : val > 255 ? 255 : val;
    out[i * 4 + 3] = c; // distance in the alpha channel (tiny-sdf convention)
  }
  return new ImageData(out, width, height);
}
