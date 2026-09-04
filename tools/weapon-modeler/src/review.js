import { reviewCases } from "./review-cases.js";
import { buildWeapon, validateWeapon, measureMassProperties } from "./mesh.js";
import { automaticGripPoint } from "./glb-export.js";
import { WeaponRenderer } from "./renderer.js";

const query = new URLSearchParams(location.search);
const seed = Number(query.get("seed") ?? 1544), batch = Number(query.get("batch") ?? 0);
const pose = query.get("pose") ?? "oblique", focus = query.get("focus") ?? "whole", lod = query.get("lod") ?? "medium";
const ids = query.get("ids")?.split(",");
const specimens = (window.reviewFixtures ?? reviewCases(seed, ids)).slice(batch * 6, batch * 6 + 6);
document.getElementById("settings").textContent = `Seed ${seed} · batch ${batch} · ${pose} · ${focus} · ${lod}`;
const results = [];
for (const specimen of specimens) {
  const article = document.createElement("article"), title = document.createElement("h2"), canvas = document.createElement("canvas"), metrics = document.createElement("p"), changes = document.createElement("p");
  title.textContent = `${specimen.name} / ${specimen.variant}`;
  article.append(title, canvas, metrics, changes); document.querySelector("main").append(article);
  const result = validateWeapon(specimen.definition, [], { lod });
  if (!result.valid) { metrics.textContent = result.errors.join(" · "); metrics.className = "error"; results.push({ ...specimen, errors: result.errors }); continue; }
  const renderer = new WeaponRenderer(canvas); renderer.framingMesh = buildWeapon(specimen.definition, { lod: "high" }); renderer.setMesh(result.mesh); renderer.setView(pose, focus);
  const physical = measureMassProperties(result.mesh, automaticGripPoint(result.resolved));
  metrics.textContent = `${result.mesh.stats.dimensions.map((value) => (value * 100).toFixed(1)).join(" × ")} cm · ${physical.massKg.toFixed(2)} kg · ${result.mesh.stats.triangles} triangles`;
  changes.textContent = specimen.changes.length ? specimen.changes.map((change) => `${change.label}: ${change.to}`).join(" · ") : specimen.variant === "Default" ? "Authored preset" : "Explicit review fixture";
  results.push({ ...specimen, stats: result.mesh.stats, physical, errors: [] });
}
window.reviewManifest = { seed, batch, pose, focus, lod, results };
document.body.dataset.ready = "true";
