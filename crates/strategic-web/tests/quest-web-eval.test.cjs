const assert = require("node:assert/strict");
const http = require("node:http");
const { mkdtemp, readFile, rm } = require("node:fs/promises");
const { tmpdir } = require("node:os");
const test = require("node:test");
const { pathToFileURL } = require("node:url");
const { resolve } = require("node:path");

const modulePromise = import(
  pathToFileURL(resolve("scripts/quest_web_eval.mjs")).href
);

test("browser evaluator requires explicit network consent and a named key", async () => {
  const { parseArgs, validateOptions } = await modulePromise;
  const parsed = parseArgs([
    "--base-url",
    "http://127.0.0.1:24301",
    "--start-path",
    "/characters",
    "--output-dir",
    "quest-browser-log",
    "--api-key-env",
    "OPENAI_API_KEY",
  ]);
  assert.throws(
    () => validateOptions(parsed, { OPENAI_API_KEY: "secret" }),
    /explicit --allow-network/,
  );
  parsed.allowNetwork = true;
  assert.throws(() => validateOptions(parsed, {}), /OPENAI_API_KEY is not set/);
});

test("browser evaluator is restricted to a loopback game server", async () => {
  const { validateOptions } = await modulePromise;
  assert.throws(
    () =>
      validateOptions(
        {
          baseUrl: "https://example.com",
          startPath: "/",
          outputDir: "quest-browser-log",
          apiKeyEnv: "OPENAI_API_KEY",
          endpoint: "https://api.openai.com/v1/chat/completions",
          model: "test",
          maxSteps: 5,
          allowNetwork: true,
        },
        { OPENAI_API_KEY: "secret" },
      ),
    /local development server/,
  );
});

test("model can only select a visible opaque control", async () => {
  const { parseDecision } = await modulePromise;
  const controls = [{ ref: "e1", tag: "button", name: "Enter tavern" }];
  assert.deepEqual(
    parseDecision(
      JSON.stringify({ action: "click", ref: "e1", reason: "Ask for local news." }),
      controls,
    ),
    { action: "click", ref: "e1", reason: "Ask for local news." },
  );
  assert.throws(
    () =>
      parseDecision(
        JSON.stringify({ action: "click", ref: "case:secret", reason: "Use hidden id." }),
        controls,
      ),
    /not currently visible/,
  );
  assert.throws(
    () =>
      parseDecision(
        JSON.stringify({ action: "navigate", url: "/quests", reason: "Skip the UI." }),
        controls,
      ),
    /unsupported model action/,
  );
  assert.deepEqual(
    parseDecision(
      JSON.stringify({ action: "scroll", deltaY: 600, reason: "Look below the fold." }),
      controls,
    ),
    { action: "scroll", deltaY: 600, reason: "Look below the fold." },
  );
});

test("screenshot log renders chronological images without hidden quest truth", async () => {
  const { renderScreenshotLog } = await modulePromise;
  const html = renderScreenshotLog({
    runId: "run-1",
    model: "cheap-model",
    status: "completed",
    steps: [
      {
        sequence: 0,
        caption: "Initial page",
        url: "http://127.0.0.1:24301/characters",
        screenshot: "step-000.png",
      },
      {
        sequence: 1,
        caption: "click e1: Enter the tavern",
        url: "http://127.0.0.1:24301/settlements/riverdale/inn",
        screenshot: "step-001.png",
      },
    ],
  });
  assert.match(html, /step-000\.png/);
  assert.match(html, /step-001\.png/);
  assert.match(html, /Enter the tavern/);
  assert.doesNotMatch(html, /canonicalCause|probability|trueSite/);
});

test(
  "browser evaluator drives visible controls and records each screenshot",
  { skip: process.env.ADVENTURESIM_BROWSER_EVAL_INTEGRATION !== "1" },
  async () => {
    const { run } = await modulePromise;
    let modelCalls = 0;
    const server = http.createServer((request, response) => {
      if (request.url === "/v1/chat/completions") {
        request.resume();
        request.on("end", () => {
          modelCalls += 1;
          const decision =
            modelCalls === 1
              ? { action: "click", ref: "e1", reason: "Open the visible quest." }
              : { action: "finish", status: "completed", reason: "The page says Quest complete." };
          response.writeHead(200, { "content-type": "application/json" });
          response.end(
            JSON.stringify({ choices: [{ message: { content: JSON.stringify(decision) } }] }),
          );
        });
        return;
      }
      response.writeHead(200, { "content-type": "text/html" });
      response.end(`<!doctype html><button onclick="document.body.innerHTML='<h1>Quest complete</h1>'">Begin quest</button>`);
    });
    await new Promise((accept) => server.listen(0, "127.0.0.1", accept));
    const address = server.address();
    const root = await mkdtemp(resolve(tmpdir(), "quest-web-eval-"));
    const outputDir = resolve(root, "screenshots");
    try {
      const manifest = await run(
        {
          baseUrl: `http://127.0.0.1:${address.port}`,
          startPath: "/",
          outputDir,
          apiKeyEnv: "TEST_API_KEY",
          endpoint: `http://127.0.0.1:${address.port}/v1/chat/completions`,
          model: "fixture-llm",
          maxSteps: 4,
          allowNetwork: true,
          headless: true,
        },
        { TEST_API_KEY: "fixture-secret" },
      );
      assert.equal(manifest.status, "completed");
      assert.equal(manifest.steps.length, 3);
      const html = await readFile(resolve(outputDir, "index.html"), "utf8");
      assert.match(html, /step-000\.png/);
      assert.match(html, /step-001\.png/);
      assert.match(html, /step-002\.png/);
    } finally {
      await new Promise((accept) => server.close(accept));
      await rm(root, { recursive: true, force: true });
    }
  },
);
