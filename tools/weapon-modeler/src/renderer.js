import { lookAt, mat4Multiply, perspective } from "./math.js";

export function fitDistance(bounds, aspect, verticalFov = 35 * Math.PI / 180, margin = 1.25) {
  const halfWidth = (bounds.max[0] - bounds.min[0]) / 2;
  const halfHeight = (bounds.max[1] - bounds.min[1]) / 2;
  const halfDepth = (bounds.max[2] - bounds.min[2]) / 2;
  const tanV = Math.tan(verticalFov / 2), tanH = tanV * aspect;
  return Math.max(halfHeight / tanV, halfWidth / tanH) * margin + halfDepth;
}

export function projectedFit(positions, bounds, aspect, yaw = 0, pitch = 0, verticalFov = 35 * Math.PI / 180, margin = 1.25) {
  const center = bounds.min.map((value, axis) => (value + bounds.max[axis]) / 2), tanV = Math.tan(verticalFov / 2), tanH = tanV * aspect;
  const eyeDirection = [Math.sin(yaw) * Math.cos(pitch), Math.sin(pitch), Math.cos(yaw) * Math.cos(pitch)];
  const right = [Math.cos(yaw), 0, -Math.sin(yaw)], up = [-Math.sin(yaw) * Math.sin(pitch), Math.cos(pitch), -Math.cos(yaw) * Math.sin(pitch)];
  const projected = [];
  for (let index = 0; index < positions.length; index += 3) {
    const relative = [positions[index] - center[0], positions[index + 1] - center[1], positions[index + 2] - center[2]];
    projected.push([relative[0] * right[0] + relative[1] * right[1] + relative[2] * right[2], relative[0] * up[0] + relative[1] * up[1] + relative[2] * up[2], relative[0] * eyeDirection[0] + relative[1] * eyeDirection[1] + relative[2] * eyeDirection[2]]);
  }
  const distance = Math.max(...projected.map(([x, y, depth]) => Math.max(depth + margin * Math.abs(x) / tanH, depth + margin * Math.abs(y) / tanV)), 0.01);
  const maxProjected = Math.max(...projected.map(([x, y, depth]) => Math.max(Math.abs(x) / Math.max(1e-6, distance - depth) / tanH, Math.abs(y) / Math.max(1e-6, distance - depth) / tanV)));
  return { distance, maxProjected, contained: Number.isFinite(distance) && maxProjected <= 1 / margin + 1e-7 };
}

const vertexSource = `#version 300 es
in vec3 position;
in vec3 normal;
in vec3 color;
uniform mat4 mvp;
out vec3 worldNormal;
out vec3 baseColor;
void main() {
  worldNormal = normal;
  baseColor = color;
  gl_Position = mvp * vec4(position, 1.0);
}`;

const fragmentSource = `#version 300 es
precision highp float;
in vec3 worldNormal;
in vec3 baseColor;
out vec4 outColor;
void main() {
  vec3 n = normalize(worldNormal);
  vec3 key = normalize(vec3(-0.4, 0.8, 0.7));
  vec3 fill = normalize(vec3(0.8, 0.2, -0.5));
  float diffuse = max(dot(n, key), 0.0) * 0.66 + max(dot(n, fill), 0.0) * 0.16;
  float rim = pow(1.0 - abs(n.z), 3.0) * 0.12;
  vec3 shaded = baseColor * (0.28 + diffuse) + vec3(rim);
  outColor = vec4(pow(shaded, vec3(1.0 / 2.2)), 1.0);
}`;

function shader(gl, type, source) {
  const result = gl.createShader(type);
  gl.shaderSource(result, source); gl.compileShader(result);
  if (!gl.getShaderParameter(result, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(result));
  return result;
}

function program(gl) {
  const result = gl.createProgram();
  gl.attachShader(result, shader(gl, gl.VERTEX_SHADER, vertexSource));
  gl.attachShader(result, shader(gl, gl.FRAGMENT_SHADER, fragmentSource));
  gl.linkProgram(result);
  if (!gl.getProgramParameter(result, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(result));
  return result;
}

export class WeaponRenderer {
  constructor(canvas) {
    this.canvas = canvas;
    this.gl = canvas.getContext("webgl2", { antialias: true, preserveDrawingBuffer: true });
    if (!this.gl) throw new Error("WebGL 2 is required");
    this.program = program(this.gl);
    this.locations = {
      position: this.gl.getAttribLocation(this.program, "position"),
      normal: this.gl.getAttribLocation(this.program, "normal"),
      color: this.gl.getAttribLocation(this.program, "color"),
      mvp: this.gl.getUniformLocation(this.program, "mvp"),
    };
    this.buffers = [this.gl.createBuffer(), this.gl.createBuffer(), this.gl.createBuffer()];
    this.yaw = 0; this.pitch = 0; this.zoom = 1; this.focus = "whole"; this.drag = null;
    this.bindInteraction();
    new ResizeObserver(() => this.draw()).observe(canvas);
  }

  bindInteraction() {
    this.canvas.addEventListener("pointerdown", (event) => { this.drag = [event.clientX, event.clientY]; this.canvas.setPointerCapture(event.pointerId); });
    this.canvas.addEventListener("pointermove", (event) => {
      if (!this.drag) return;
      this.yaw += (event.clientX - this.drag[0]) * 0.009;
      this.pitch = Math.max(-1.2, Math.min(1.2, this.pitch + (event.clientY - this.drag[1]) * 0.007));
      this.drag = [event.clientX, event.clientY]; this.draw();
    });
    this.canvas.addEventListener("pointerup", () => { this.drag = null; });
    this.canvas.addEventListener("wheel", (event) => { event.preventDefault(); this.zoom = Math.max(0.55, Math.min(2.4, this.zoom * Math.exp(event.deltaY * 0.001))); this.draw(); }, { passive: false });
    this.canvas.addEventListener("dblclick", () => this.setView("front", "whole"));
  }

  setView(pose = "front", focus = this.focus) {
    const poses = { front: [0, 0], back: [Math.PI, 0], left: [-Math.PI / 2, 0], right: [Math.PI / 2, 0], oblique: [0.68, 0.18] };
    [this.yaw, this.pitch] = poses[pose] ?? poses.front;
    this.focus = focus; this.zoom = 1; this.draw();
  }

  setMesh(mesh) {
    this.mesh = mesh;
    const gl = this.gl;
    [mesh.positions, mesh.normals, mesh.colors].forEach((data, index) => {
      gl.bindBuffer(gl.ARRAY_BUFFER, this.buffers[index]);
      gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(data), gl.STATIC_DRAW);
    });
    this.setView("front", "whole");
  }

  focusBounds() {
    if (this.focus === "whole") return this.mesh.stats.bounds;
    const whole = this.mesh.stats.bounds, height = whole.max[1] - whole.min[1];
    const hasShaft = this.mesh.parts.some((part) => part.label === "shaft");
    const limit = hasShaft ? whole.max[1] - Math.min(0.72, height * 0.34) : whole.min[1] + Math.min(0.48, height * 0.4);
    const selected = [];
    for (let index = 0; index < this.mesh.positions.length; index += 3) {
      const point = this.mesh.positions.slice(index, index + 3);
      if ((hasShaft && point[1] >= limit) || (!hasShaft && point[1] <= limit)) selected.push(point);
    }
    if (!selected.length) return whole;
    return { min: [0, 1, 2].map((axis) => Math.min(...selected.map((point) => point[axis]))), max: [0, 1, 2].map((axis) => Math.max(...selected.map((point) => point[axis]))) };
  }

  draw() {
    if (!this.mesh) return;
    const gl = this.gl, ratio = window.devicePixelRatio || 1;
    const width = Math.max(1, Math.floor(this.canvas.clientWidth * ratio));
    const height = Math.max(1, Math.floor(this.canvas.clientHeight * ratio));
    if (this.canvas.width !== width || this.canvas.height !== height) { this.canvas.width = width; this.canvas.height = height; }
    gl.viewport(0, 0, width, height);
    gl.clearColor(0.075, 0.068, 0.058, 1); gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    gl.enable(gl.DEPTH_TEST); gl.enable(gl.CULL_FACE); gl.cullFace(gl.BACK);
    gl.useProgram(this.program);
    const focusBounds = this.focusBounds();
    const center = focusBounds.min.map((value, axis) => (value + focusBounds.max[axis]) / 2);
    const radius = Math.max(0.3, this.mesh.stats.radius);
    const focusPositions = [];
    for (let index = 0; index < this.mesh.positions.length; index += 3) if ([0, 1, 2].every((axis) => this.mesh.positions[index + axis] >= focusBounds.min[axis] - 1e-8 && this.mesh.positions[index + axis] <= focusBounds.max[axis] + 1e-8)) focusPositions.push(...this.mesh.positions.slice(index, index + 3));
    const distance = projectedFit(focusPositions, focusBounds, width / height, this.yaw, this.pitch).distance * this.zoom;
    const eye = [center[0] + Math.sin(this.yaw) * Math.cos(this.pitch) * distance, center[1] + Math.sin(this.pitch) * distance, center[2] + Math.cos(this.yaw) * Math.cos(this.pitch) * distance];
    const projection = perspective(35 * Math.PI / 180, width / height, radius * 0.02, radius * 12);
    const mvp = mat4Multiply(projection, lookAt(eye, center, [0, 1, 0]));
    gl.uniformMatrix4fv(this.locations.mvp, false, mvp);
    [this.locations.position, this.locations.normal, this.locations.color].forEach((location, index) => {
      gl.bindBuffer(gl.ARRAY_BUFFER, this.buffers[index]); gl.enableVertexAttribArray(location); gl.vertexAttribPointer(location, 3, gl.FLOAT, false, 0, 0);
    });
    gl.drawArrays(gl.TRIANGLES, 0, this.mesh.positions.length / 3);
  }
}
