import { access, mkdir, readFile, writeFile } from "node:fs/promises";
import { basename, dirname, extname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { automaticGripPoint, buildSkinnedWeaponGlb, parseGlb } from "./src/glb-export.js";
import { validateWeapon } from "./src/mesh.js";
import { PRESETS, copyPreset } from "./src/presets.js";

const repository = fileURLToPath(new URL("../../", import.meta.url));
const defaultRigs = [
  join(repository, "assets_src", "biped", "unarmed", "base.glb"),
  join(repository, "assets", "animations", "biped", "unarmed", "base.glb"),
];

function usage() {
  return "usage: node cli.mjs --preset <id> --skinned <output.glb> [--joint r_weapon|l_weapon] [--rig <base.glb>] [--name <mesh-name>]";
}

export function parseArguments(values) {
  const options = {};
  for (let index = 0; index < values.length; index += 1) {
    const flag = values[index];
    if (!["--preset", "--skinned", "--joint", "--rig", "--name"].includes(flag)) throw new Error(`unknown argument ${flag}\n${usage()}`);
    const value = values[++index];
    if (!value || value.startsWith("--")) throw new Error(`${flag} requires a value\n${usage()}`);
    const key = flag.slice(2); options[key] = value;
  }
  if (!options.preset || !options.skinned) throw new Error(usage());
  if (options.joint !== undefined && !new Set(["r_weapon", "l_weapon"]).has(options.joint)) throw new Error("--joint must be r_weapon or l_weapon");
  if (extname(options.skinned).toLowerCase() !== ".glb") throw new Error("--skinned output must end in .glb");
  return options;
}

async function firstExisting(paths) {
  for (const path of paths) {
    try { await access(path); return path; } catch (error) { if (error?.code !== "ENOENT") throw error; }
  }
  throw new Error("character rig not found; run `just prepare-john-rig` or pass --rig <base.glb>");
}

export async function exportSkinnedPreset(options) {
  const preset = PRESETS.find((candidate) => candidate.id === options.preset);
  if (!preset) throw new Error(`unknown weapon preset ${options.preset}`);
  const active = copyPreset(preset);
  const validation = validateWeapon(active.definition, active.controls);
  if (!validation.valid) throw new Error(`preset ${options.preset} is invalid: ${validation.errors.join(" · ")}`);

  const rigPath = options.rig ? resolve(options.rig) : await firstExisting(defaultRigs);
  const outputPath = resolve(options.skinned);
  const meshName = options.name ?? basename(outputPath, extname(outputPath));
  const shield = validation.resolved.components.find((component) => ["roundShield", "shapedShield"].includes(component.kind));
  const attachment = options.joint ?? (shield?.mirrored ? "l_weapon" : "r_weapon");
  const glb = buildSkinnedWeaponGlb(await readFile(rigPath), validation.mesh, {
    name: meshName,
    attachment,
    gripPoint: automaticGripPoint(validation.resolved),
  });
  const parsed = parseGlb(glb);
  const meshNode = parsed.document.nodes.find((node) => node.name === meshName && node.mesh !== undefined);
  if (!meshNode || meshNode.skin === undefined) throw new Error("generated GLB does not contain a skinned weapon node");
  await mkdir(dirname(outputPath), { recursive: true });
  await writeFile(outputPath, glb);
  return {
    outputPath,
    rigPath,
    bytes: glb.byteLength,
    triangles: validation.mesh.stats.triangles,
    joints: parsed.document.skins[meshNode.skin].joints.length,
    attachment,
  };
}

async function main() {
  const result = await exportSkinnedPreset(parseArguments(process.argv.slice(2)));
  console.log(`exported ${result.triangles} triangles as a ${result.joints}-joint skinned GLB to ${result.outputPath}`);
  console.log(`attachment: ${result.attachment}`);
  console.log(`rig: ${result.rigPath}`);
  console.log(`bytes: ${result.bytes}`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
