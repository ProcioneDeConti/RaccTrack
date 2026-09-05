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

export interface FillPolygon {
  /** [lon, lat] ring — NOT closed (no need to repeat the first point). */
  ring: [number, number][];
  /** 0–1 RGBA. */
  color: [number, number, number, number];
}

interface CompiledPoly {
  buffer: WebGLBuffer;
  count: number;
  color: [number, number, number, number];
}

const VERT_SRC = `
  attribute vec2 a_pos;
  uniform mat4 u_matrix;
  void main() {
    gl_Position = u_matrix * vec4(a_pos, 0.0, 1.0);
  }
`;
const FRAG_SRC = `
  precision mediump float;
  uniform vec4 u_color;
  void main() {
    gl_FragColor = vec4(u_color.rgb * u_color.a, u_color.a);
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
      this.compiled.push({ buffer, count: idx.length, color: poly.color });
    }
    this.dirty = false;
  }

  render(gl: WebGLRenderingContext, matrix: Mat4Like) {
    if (this.dirty) this.rebuild();
    if (!this.program || this.compiled.length === 0) return;
    gl.useProgram(this.program);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    gl.uniformMatrix4fv(this.uMatrix, false, Float32Array.from(matrix));
    gl.enableVertexAttribArray(this.aPos);
    for (const c of this.compiled) {
      gl.bindBuffer(gl.ARRAY_BUFFER, c.buffer);
      gl.vertexAttribPointer(this.aPos, 2, gl.FLOAT, false, 0, 0);
      gl.uniform4f(this.uColor, c.color[0], c.color[1], c.color[2], c.color[3]);
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
