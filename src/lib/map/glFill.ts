// A custom MapLibre WebGL layer that fills arbitrary polygons directly in
// screen space, entirely bypassing the GeoJSON/vector-tile pipeline.
//
// Why: `geojson-vt` (which backs every normal `fill` layer) clips and
// simplifies each polygon independently *per tile*. That's fine for typical
// real-world polygons (airspace boundaries: few vertices, straight edges),
// but it breaks down for dense, near-regular rings — like a circle
// approximated by many evenly-spaced points — where adjacent tiles can
// simplify their shared edge differently and the seam between them comes out
// wrong (dropped or inverted fill). That's exactly what killed the original
// circular geofence feature. Triangulating the ring once with `earcut` and
// drawing it as one WebGL draw call, with no tiling step at all, makes that
// entire class of bug structurally impossible.
//
// See MapLibre's "Add a custom style layer" example for the base pattern
// (MercatorCoordinate + the projection matrix passed into `render`).

import earcut from "earcut";
import maplibregl from "maplibre-gl/dist/maplibre-gl-csp";
import type { CustomLayerInterface, Map as MlMap } from "maplibre-gl";

// MapLibre's `mat4` is its own indexable-array-like type (not a real
// Float32Array), but it's a plain 16-number buffer at runtime — safe to hand
// straight to `uniformMatrix4fv`, which only needs `ArrayLike<number>`.
type Mat4Like = ArrayLike<number>;

/** Kept in sync with `FillPattern` in api/types.ts — "solid" isn't in this
 *  map since it never reaches the shader (see `PATTERN_IDS` usage below). */
const PATTERN_IDS: Record<string, number> = { stripe: 1, hash: 2, dot: 3, check: 4 };

export interface FillPolygon {
  /** [lon, lat] ring — NOT closed (no need to repeat the first point). */
  ring: [number, number][];
  /** 0–1 RGBA. */
  color: [number, number, number, number];
  /** Defaults to solid fill. Anything not in `PATTERN_IDS` also renders solid. */
  pattern?: string;
}

interface CompiledPoly {
  buffer: WebGLBuffer;
  count: number;
  color: [number, number, number, number];
  pattern: number;
}

const VERT_SRC = `
  attribute vec2 a_pos;
  uniform mat4 u_matrix;
  void main() {
    gl_Position = u_matrix * vec4(a_pos, 0.0, 1.0);
  }
`;
// Patterns are procedural, computed straight from screen-space fragment
// coordinates — deliberately not a tiled texture/sprite (which is how
// MapLibre's own native `fill-pattern` works, e.g. the caution-stripe
// coverage boundary in coverage.ts): this custom layer exists specifically
// because tiled/sprite approaches broke on dense, near-regular rings (see
// the file header), so keeping the pattern itself in the shader avoids
// reintroducing any tiling/seam concerns for the *pattern* on top of the
// fill it already solved for the *shape*. Screen-space (not world-space)
// so the pattern's visual density stays constant regardless of zoom, like a
// UI texture rather than a scaled ground marking.
const FRAG_SRC = `
  precision mediump float;
  uniform vec4 u_color;
  uniform int u_pattern;
  const float TILE = 10.0;
  void main() {
    float mask = 1.0;
    vec2 p = gl_FragCoord.xy;
    if (u_pattern == 1) { // diagonal stripe
      mask = mod(p.x + p.y, TILE) < TILE * 0.5 ? 1.0 : 0.0;
    } else if (u_pattern == 2) { // crosshatch
      float a = mod(p.x + p.y, TILE);
      float b = mod(p.x - p.y, TILE);
      mask = (a < TILE * 0.35 || b < TILE * 0.35) ? 1.0 : 0.0;
    } else if (u_pattern == 3) { // dots
      vec2 g = mod(p, TILE) - TILE * 0.5;
      mask = length(g) < TILE * 0.28 ? 1.0 : 0.0;
    } else if (u_pattern == 4) { // checkerboard
      vec2 g = mod(p, TILE);
      bool xh = g.x < TILE * 0.5;
      bool yh = g.y < TILE * 0.5;
      mask = (xh == yh) ? 1.0 : 0.0;
    }
    float a = u_color.a * mask;
    gl_FragColor = vec4(u_color.rgb * a, a);
  }
`;

function compileShader(gl: WebGLRenderingContext, type: number, src: string): WebGLShader {
  const sh = gl.createShader(type)!;
  gl.shaderSource(sh, src);
  gl.compileShader(sh);
  if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
    const info = gl.getShaderInfoLog(sh);
    gl.deleteShader(sh);
    throw new Error(`glFill shader compile error: ${info}`);
  }
  return sh;
}

class FillLayer implements CustomLayerInterface {
  id: string;
  type = "custom" as const;
  renderingMode = "2d" as const;

  private gl: WebGLRenderingContext | null = null;
  private program: WebGLProgram | null = null;
  private aPos = -1;
  private uMatrix: WebGLUniformLocation | null = null;
  private uColor: WebGLUniformLocation | null = null;
  private uPattern: WebGLUniformLocation | null = null;
  private polygons: FillPolygon[] = [];
  private compiled: CompiledPoly[] = [];
  private dirty = true;

  constructor(id: string) {
    this.id = id;
  }

  setPolygons(polys: FillPolygon[]) {
    this.polygons = polys;
    this.dirty = true;
  }

  onAdd(_map: MlMap, gl: WebGLRenderingContext) {
    this.gl = gl;
    const vs = compileShader(gl, gl.VERTEX_SHADER, VERT_SRC);
    const fs = compileShader(gl, gl.FRAGMENT_SHADER, FRAG_SRC);
    const program = gl.createProgram()!;
    gl.attachShader(program, vs);
    gl.attachShader(program, fs);
    gl.linkProgram(program);
    this.program = program;
    this.aPos = gl.getAttribLocation(program, "a_pos");
    this.uMatrix = gl.getUniformLocation(program, "u_matrix");
    this.uColor = gl.getUniformLocation(program, "u_color");
    this.uPattern = gl.getUniformLocation(program, "u_pattern");
    this.dirty = true;
  }

  onRemove() {
    const gl = this.gl;
    if (gl) {
      for (const c of this.compiled) gl.deleteBuffer(c.buffer);
      if (this.program) gl.deleteProgram(this.program);
    }
    this.compiled = [];
    this.program = null;
    this.gl = null;
    this.dirty = true;
  }

  private rebuild() {
    const gl = this.gl!;
    for (const c of this.compiled) gl.deleteBuffer(c.buffer);
    this.compiled = [];
    for (const poly of this.polygons) {
      if (poly.ring.length < 3) continue;
      const flat: number[] = [];
      for (const [lng, lat] of poly.ring) {
        const m = maplibregl.MercatorCoordinate.fromLngLat({ lng, lat });
        flat.push(m.x, m.y);
      }
      const idx = earcut(flat);
      if (idx.length === 0) continue;
      const verts = new Float32Array(idx.length * 2);
      for (let i = 0; i < idx.length; i++) {
        verts[i * 2] = flat[idx[i] * 2];
        verts[i * 2 + 1] = flat[idx[i] * 2 + 1];
      }
      const buffer = gl.createBuffer()!;
      gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
      gl.bufferData(gl.ARRAY_BUFFER, verts, gl.STATIC_DRAW);
      this.compiled.push({
        buffer,
        count: idx.length,
        color: poly.color,
        pattern: (poly.pattern && PATTERN_IDS[poly.pattern]) || 0,
      });
    }
    this.dirty = false;
  }

  render(gl: WebGLRenderingContext, matrix: Mat4Like) {
    if (this.dirty) this.rebuild();
    if (!this.program || this.compiled.length === 0) return;
    gl.useProgram(this.program);
    // MapLibre composites its own 2D layers purely by paint order and never
    // needed this layer to manage depth-test state itself — but the shared
    // WebGL context is no longer MapLibre's alone now that deck.gl renders
    // into it too (interleaved mode), and deck.gl's renderer, being built
    // for 3D content, can leave depth testing enabled. Without an explicit
    // reset here, this fill inherits whatever state the previous draw call
    // left behind, which can mean failing a depth test against something
    // drawn after it in paint order (e.g. a basemap water/lake fill) even
    // though paint order says this should be on top — the exact "fill stuck
    // under lakes, outline fine" symptom, since the outline is a plain
    // MapLibre `line` layer MapLibre manages state for directly.
    gl.disable(gl.DEPTH_TEST);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    gl.uniformMatrix4fv(this.uMatrix, false, Float32Array.from(matrix));
    gl.enableVertexAttribArray(this.aPos);
    for (const c of this.compiled) {
      gl.bindBuffer(gl.ARRAY_BUFFER, c.buffer);
      gl.vertexAttribPointer(this.aPos, 2, gl.FLOAT, false, 0, 0);
      gl.uniform4f(this.uColor, c.color[0], c.color[1], c.color[2], c.color[3]);
      gl.uniform1i(this.uPattern, c.pattern);
      gl.drawArrays(gl.TRIANGLES, 0, c.count);
    }
    gl.disableVertexAttribArray(this.aPos);
  }
}

export function createFillLayer(id: string): CustomLayerInterface & {
  setPolygons(polys: FillPolygon[]): void;
} {
  return new FillLayer(id);
}
