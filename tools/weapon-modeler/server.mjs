import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL(".", import.meta.url));
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
    const relative = pathname === "/" ? "index.html" : pathname.slice(1);
    const resolved = normalize(join(root, relative));
    if (!resolved.startsWith(root)) {
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
