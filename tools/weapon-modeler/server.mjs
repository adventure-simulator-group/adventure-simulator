import { createServer } from "node:http";
import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { extname, join, normalize, sep } from "node:path";
import { fileURLToPath } from "node:url";

const withoutTrailingSeparator = (path) => normalize(path).replace(/[\\/]+$/, "");
const root = withoutTrailingSeparator(fileURLToPath(new URL(".", import.meta.url)));
const repository = withoutTrailingSeparator(fileURLToPath(new URL("../../", import.meta.url)));
const rigCandidates = [
  join(repository, "assets_src", "biped", "unarmed", "base.glb"),
  join(repository, "assets", "animations", "biped", "unarmed", "base.glb"),
];
const exportDirectory = join(repository, "assets_src", "weapons");
const port = Number.parseInt(process.env.WEAPON_MODELER_PORT ?? "4173", 10);
const types = {
  ".css": "text/css; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
};

createServer(async (request, response) => {
  try {
    const pathname = new URL(request.url ?? "/", "http://localhost").pathname;
    if (request.method === "GET" && pathname === "/api/rig") {
      let rig;
      for (const candidate of rigCandidates) {
        try { rig = { candidate, body: await readFile(candidate) }; break; } catch (error) { if (error?.code !== "ENOENT") throw error; }
      }
      if (!rig) {
        response.writeHead(409, { "Content-Type": "application/json" }).end(JSON.stringify({ error: "Character rig not found. Run just prepare-john-rig." }));
        return;
      }
      response.writeHead(200, { "Cache-Control": "no-store", "Content-Type": "model/gltf-binary", "X-Fabelgeist-Rig-Path": rig.candidate.slice(repository.length + 1).replaceAll("\\", "/") });
      response.end(rig.body);
      return;
    }
    if (request.method === "POST" && pathname === "/api/export") {
      const fileName = new URL(request.url ?? "/", "http://localhost").searchParams.get("name") ?? "";
      if (!/^[a-z0-9][a-z0-9_-]*\.glb$/.test(fileName)) {
        response.writeHead(400, { "Content-Type": "application/json" }).end(JSON.stringify({ error: "Export name must use lowercase letters, numbers, hyphens, or underscores." }));
        return;
      }
      const chunks = []; let size = 0;
      for await (const chunk of request) {
        size += chunk.length;
        if (size > 64 * 1024 * 1024) throw new Error("Export is larger than 64 MB");
        chunks.push(chunk);
      }
      const body = Buffer.concat(chunks);
      if (body.length < 12 || body.subarray(0, 4).toString("ascii") !== "glTF") throw new Error("Export body is not a GLB file");
      await mkdir(exportDirectory, { recursive: true });
      await writeFile(join(exportDirectory, fileName), body);
      response.writeHead(200, { "Content-Type": "application/json" }).end(JSON.stringify({ path: `assets_src/weapons/${fileName}` }));
      return;
    }
    const relative = pathname === "/" ? "index.html" : pathname.slice(1);
    const resolved = normalize(join(root, relative));
    if (resolved !== root && !resolved.startsWith(root + sep)) {
      response.writeHead(403).end("Forbidden");
      return;
    }
    const details = await stat(resolved);
    const file = details.isDirectory() ? join(resolved, "index.html") : resolved;
    const body = await readFile(file);
    response.writeHead(200, {
      "Cache-Control": "no-store",
      "Content-Type": types[extname(file)] ?? "application/octet-stream",
    });
    response.end(body);
  } catch (error) {
    response.writeHead(error?.code === "ENOENT" ? 404 : 500).end("Not found");
  }
}).listen(port, "127.0.0.1", () => {
  console.log(`Weapon modeler listening at http://127.0.0.1:${port}`);
});
