#!/usr/bin/env node

import { mkdir, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { basename, isAbsolute, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const DEFAULT_MODEL = "gpt-4.1-mini";
const DEFAULT_ENDPOINT = "https://api.openai.com/v1/chat/completions";
const DEFAULT_MAX_STEPS = 64;
const MAX_VISIBLE_TEXT = 24_000;
const MAX_CONTROLS = 200;
const MAX_RESPONSE_BYTES = 32_000;

export function parseArgs(argv) {
  const options = {
    model: DEFAULT_MODEL,
    endpoint: DEFAULT_ENDPOINT,
    maxSteps: DEFAULT_MAX_STEPS,
    headless: true,
    allowNetwork: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    const value = () => {
      index += 1;
      if (index >= argv.length || argv[index].startsWith("--")) {
        throw new Error(`${flag} requires a value`);
      }
      return argv[index];
    };
    switch (flag) {
      case "--base-url":
        options.baseUrl = value();
        break;
      case "--start-path":
        options.startPath = value();
        break;
      case "--output-dir":
        options.outputDir = value();
        break;
      case "--api-key-env":
        options.apiKeyEnv = value();
        break;
      case "--model":
        options.model = value();
        break;
      case "--endpoint":
        options.endpoint = value();
        break;
      case "--max-steps":
        options.maxSteps = Number.parseInt(value(), 10);
        break;
      case "--allow-network":
        options.allowNetwork = true;
        break;
      case "--headed":
        options.headless = false;
        break;
      default:
        throw new Error(`unknown argument: ${flag}`);
    }
  }
  return options;
}

export function validateOptions(options, env = process.env) {
  for (const key of ["baseUrl", "startPath", "outputDir", "apiKeyEnv"]) {
    if (!options[key]) {
      throw new Error(`--${key.replace(/[A-Z]/g, (part) => `-${part.toLowerCase()}`)} is required`);
    }
  }
  if (!options.allowNetwork) {
    throw new Error("LLM browser evaluation requires explicit --allow-network");
  }
  if (!/^[A-Z_][A-Z0-9_]*$/.test(options.apiKeyEnv)) {
    throw new Error("--api-key-env must name an uppercase environment variable");
  }
  if (!env[options.apiKeyEnv]) {
    throw new Error(`environment variable ${options.apiKeyEnv} is not set`);
  }
  if (!Number.isInteger(options.maxSteps) || options.maxSteps < 1 || options.maxSteps > 256) {
    throw new Error("--max-steps must be between 1 and 256");
  }

  const base = new URL(options.baseUrl);
  if (base.protocol !== "http:" && base.protocol !== "https:") {
    throw new Error("--base-url must use HTTP or HTTPS");
  }
  if (!["127.0.0.1", "localhost", "::1"].includes(base.hostname)) {
    throw new Error("--base-url must target the local development server");
  }
  if (base.username || base.password || base.search || base.hash) {
    throw new Error("--base-url must not contain credentials, a query, or a fragment");
  }
  if (!options.startPath.startsWith("/") || options.startPath.startsWith("//")) {
    throw new Error("--start-path must be an absolute path on the local server");
  }
  const endpoint = new URL(options.endpoint);
  const endpointIsLoopback = ["127.0.0.1", "localhost", "::1"].includes(endpoint.hostname);
  if (endpoint.protocol !== "https:" && !(endpoint.protocol === "http:" && endpointIsLoopback)) {
    throw new Error("--endpoint must use HTTPS unless it is a loopback fixture");
  }
  if (endpoint.username || endpoint.password || endpoint.search || endpoint.hash) {
    throw new Error("--endpoint must not contain credentials, a query, or a fragment");
  }
  if (!isAbsolute(resolve(options.outputDir))) {
    throw new Error("--output-dir could not be resolved");
  }
  return {
    ...options,
    baseUrl: base.toString(),
    endpoint: endpoint.toString(),
    outputDir: resolve(options.outputDir),
    apiKey: env[options.apiKeyEnv],
  };
}

export function parseDecision(raw, controls) {
  if (Buffer.byteLength(raw, "utf8") > MAX_RESPONSE_BYTES) {
    throw new Error("model response exceeded the byte limit");
  }
  let value;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new Error("model response was not valid JSON");
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("model decision must be an object");
  }
  const allowed = new Set(["click", "fill", "select", "scroll", "wait", "finish"]);
  if (!allowed.has(value.action)) {
    throw new Error(`unsupported model action: ${String(value.action)}`);
  }
  if (typeof value.reason !== "string" || !value.reason.trim()) {
    throw new Error("model decision requires a non-empty reason");
  }
  if (value.action === "finish") {
    if (!["completed", "failed"].includes(value.status)) {
      throw new Error("finish requires status completed or failed");
    }
    return { action: value.action, status: value.status, reason: value.reason.trim() };
  }
  if (value.action === "wait") {
    const milliseconds = Number(value.milliseconds ?? 1_000);
    if (!Number.isFinite(milliseconds) || milliseconds < 100 || milliseconds > 5_000) {
      throw new Error("wait milliseconds must be between 100 and 5000");
    }
    return { action: value.action, milliseconds, reason: value.reason.trim() };
  }
  if (value.action === "scroll") {
    const deltaY = Number(value.deltaY);
    if (!Number.isFinite(deltaY) || deltaY === 0 || Math.abs(deltaY) > 900) {
      throw new Error("scroll deltaY must be non-zero and no greater than 900 pixels");
    }
    return { action: value.action, deltaY, reason: value.reason.trim() };
  }
  if (typeof value.ref !== "string" || !controls.some((control) => control.ref === value.ref)) {
    throw new Error("model selected a control that is not currently visible");
  }
  if (value.action === "fill" || value.action === "select") {
    if (typeof value.value !== "string" || value.value.length > 500) {
      throw new Error(`${value.action} requires a string value of at most 500 characters`);
    }
  }
  return {
    action: value.action,
    ref: value.ref,
    ...(value.value === undefined ? {} : { value: value.value }),
    reason: value.reason.trim(),
  };
}

export function renderScreenshotLog(manifest) {
  const cards = manifest.steps
    .map(
      (step) => `<article>
  <h2>${escapeHtml(step.sequence.toString().padStart(3, "0"))}: ${escapeHtml(step.caption)}</h2>
  <p><code>${escapeHtml(step.url)}</code></p>
  <img src="${escapeHtml(step.screenshot)}" alt="Browser state ${step.sequence}: ${escapeHtml(step.caption)}">
</article>`,
    )
    .join("\n");
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Quest browser evaluation ${escapeHtml(manifest.runId)}</title>
  <style>
    body { margin: 2rem auto; max-width: 96rem; padding: 0 1rem; color: #eee; background: #161616; font: 16px/1.5 system-ui, sans-serif; }
    header, article { margin-bottom: 2rem; padding: 1rem; border: 1px solid #555; border-radius: .5rem; background: #222; }
    h1, h2 { margin-top: 0; }
    img { display: block; width: 100%; height: auto; border: 1px solid #777; }
    code { overflow-wrap: anywhere; }
  </style>
</head>
<body>
<header>
  <h1>Quest browser evaluation</h1>
  <p>Run ${escapeHtml(manifest.runId)} · model ${escapeHtml(manifest.model)} · ${escapeHtml(manifest.status)}</p>
</header>
${cards}
</body>
</html>
`;
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

async function observe(page) {
  const interactive = page.locator(
    'a[href], button, input:not([type="hidden"]), textarea, select, [role="button"], [role="link"], [role="tab"]',
  );
  const count = Math.min(await interactive.count(), MAX_CONTROLS);
  const viewport = page.viewportSize();
  const controls = [];
  const locators = new Map();
  for (let index = 0; index < count; index += 1) {
    const locator = interactive.nth(index);
    if (!(await locator.isVisible()) || !(await locator.isEnabled())) continue;
    const box = await locator.boundingBox();
    if (
      !box ||
      !viewport ||
      box.x + box.width <= 0 ||
      box.y + box.height <= 0 ||
      box.x >= viewport.width ||
      box.y >= viewport.height
    ) {
      continue;
    }
    const info = await locator.evaluate((element) => ({
      tag: element.tagName.toLowerCase(),
      role: element.getAttribute("role") || "",
      name:
        element.getAttribute("aria-label") ||
        element.getAttribute("title") ||
        element.innerText ||
        element.getAttribute("placeholder") ||
        element.getAttribute("name") ||
        "",
      type: element.getAttribute("type") || "",
      value: "value" in element ? String(element.value) : "",
      options:
        element instanceof HTMLSelectElement
          ? [...element.options].map((option) => option.text).slice(0, 50)
          : [],
    }));
    const ref = `e${controls.length + 1}`;
    controls.push({
      ref,
      tag: info.tag,
      role: info.role,
      name: info.name.replace(/\s+/g, " ").trim().slice(0, 300),
      type: info.type,
      value: info.value.slice(0, 300),
      ...(info.options.length ? { options: info.options } : {}),
    });
    locators.set(ref, locator);
  }
  const visibleText = (
    await page.locator("body").evaluate((body) => {
      const walker = document.createTreeWalker(body, NodeFilter.SHOW_TEXT);
      const lines = [];
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        const text = node.textContent?.replace(/\s+/g, " ").trim();
        if (!text || !(node.parentElement instanceof HTMLElement)) continue;
        const style = getComputedStyle(node.parentElement);
        if (style.display === "none" || style.visibility === "hidden" || style.opacity === "0") {
          continue;
        }
        const range = document.createRange();
        range.selectNodeContents(node);
        const onScreen = [...range.getClientRects()].some(
          (rect) =>
            rect.right > 0 &&
            rect.bottom > 0 &&
            rect.left < window.innerWidth &&
            rect.top < window.innerHeight,
        );
        if (onScreen) lines.push(text);
      }
      return lines.join("\n");
    })
  )
    .replace(/\u0000/g, "")
    .slice(0, MAX_VISIBLE_TEXT);
  return { controls, locators, visibleText, title: await page.title(), url: page.url() };
}

async function requestDecision(options, observation, screenshotBytes, history) {
  const abort = new AbortController();
  const timeout = setTimeout(() => abort.abort(), 45_000);
  try {
    const response = await fetch(options.endpoint, {
      method: "POST",
      redirect: "error",
      signal: abort.signal,
      headers: {
        authorization: `Bearer ${options.apiKey}`,
        "content-type": "application/json",
      },
      body: JSON.stringify({
        model: options.model,
        temperature: 0,
        max_tokens: 500,
        response_format: { type: "json_object" },
        messages: [
          {
            role: "system",
            content:
              "You are playing Adventure Simulator entirely through its visible web UI. Start and finish one quest. Infer clues from dialogue and the journal; do not assume hidden state. Choose exactly one currently visible control or scroll the viewport. Return JSON only. Actions: click {ref}, fill {ref,value}, select {ref,value}, scroll {deltaY}, wait {milliseconds}, or finish {status:'completed'|'failed'}. Every action needs a short reason. Only finish completed when the visible UI establishes that the quest was resolved.",
          },
          {
            role: "user",
            content: [
              {
                type: "text",
                text: JSON.stringify({
                  page: {
                    title: observation.title,
                    url: observation.url,
                    visibleText: observation.visibleText,
                    controls: observation.controls,
                  },
                  recentActions: history.slice(-8),
                }),
              },
              {
                type: "image_url",
                image_url: { url: `data:image/png;base64,${screenshotBytes.toString("base64")}` },
              },
            ],
          },
        ],
      }),
    });
    const body = await response.text();
    if (!response.ok) {
      throw new Error(`model request failed with HTTP ${response.status}: ${body.slice(0, 500)}`);
    }
    if (Buffer.byteLength(body, "utf8") > 1_000_000) {
      throw new Error("provider response exceeded the byte limit");
    }
    const envelope = JSON.parse(body);
    const content = envelope?.choices?.[0]?.message?.content;
    if (typeof content !== "string") {
      throw new Error("provider response did not contain message content");
    }
    return parseDecision(content, observation.controls);
  } finally {
    clearTimeout(timeout);
  }
}

async function perform(decision, observation, page) {
  if (decision.action === "wait") {
    await page.waitForTimeout(decision.milliseconds);
    return;
  }
  if (decision.action === "scroll") {
    await page.mouse.wheel(0, decision.deltaY);
    await page.waitForTimeout(150);
    return;
  }
  const locator = observation.locators.get(decision.ref);
  if (!locator) throw new Error("selected control disappeared");
  if (decision.action === "click") await locator.click();
  if (decision.action === "fill") await locator.fill(decision.value);
  if (decision.action === "select") await locator.selectOption({ label: decision.value });
  await page.waitForLoadState("domcontentloaded").catch(() => {});
  await page.waitForTimeout(350);
}

function decisionCaption(decision, observation) {
  if (decision.ref) {
    const control = observation.controls.find((candidate) => candidate.ref === decision.ref);
    const label = control?.name || `${control?.role || control?.tag || "control"} ${decision.ref}`;
    return `${decision.action} “${label}”: ${decision.reason}`;
  }
  if (decision.action === "scroll") {
    return `scroll ${decision.deltaY > 0 ? "down" : "up"}: ${decision.reason}`;
  }
  return `${decision.action}: ${decision.reason}`;
}

async function saveState(page, outputDir, manifest, caption) {
  const sequence = manifest.steps.length;
  const screenshot = `step-${sequence.toString().padStart(3, "0")}.png`;
  const bytes = await page.screenshot({ path: join(outputDir, screenshot) });
  manifest.steps.push({ sequence, caption, url: page.url(), screenshot });
  await writeFile(join(outputDir, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  await writeFile(join(outputDir, "index.html"), renderScreenshotLog(manifest));
  return bytes;
}

export async function run(rawOptions, env = process.env) {
  const options = validateOptions(rawOptions, env);
  await mkdir(options.outputDir, { recursive: false });
  const { chromium } = createRequire(import.meta.url)("playwright");
  const browser = await chromium.launch({ headless: options.headless });
  const manifest = {
    schemaVersion: 1,
    runId: basename(options.outputDir),
    model: options.model,
    startedAt: new Date().toISOString(),
    status: "running",
    steps: [],
  };
  const history = [];
  try {
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
    const page = await context.newPage();
    context.on("page", (unexpectedPage) => {
      if (unexpectedPage !== page) void unexpectedPage.close();
    });
    page.on("dialog", (dialog) => dialog.dismiss());
    const startUrl = new URL(options.startPath, options.baseUrl);
    await page.goto(startUrl.toString(), { waitUntil: "domcontentloaded", timeout: 30_000 });
    let screenshot = await saveState(page, options.outputDir, manifest, "Initial page");
    for (let step = 1; step <= options.maxSteps; step += 1) {
      const observation = await observe(page);
      const decision = await requestDecision(options, observation, screenshot, history);
      history.push(decision);
      if (decision.action === "finish") {
        manifest.status = decision.status;
        manifest.finishedAt = new Date().toISOString();
        manifest.finishReason = decision.reason;
        await saveState(page, options.outputDir, manifest, `Model finished: ${decision.reason}`);
        break;
      }
      await perform(decision, observation, page);
      if (new URL(page.url()).origin !== new URL(options.baseUrl).origin) {
        throw new Error("a visible control navigated outside the local game server");
      }
      screenshot = await saveState(
        page,
        options.outputDir,
        manifest,
        decisionCaption(decision, observation),
      );
    }
    if (manifest.status === "running") {
      manifest.status = "step_limit";
      manifest.finishedAt = new Date().toISOString();
      manifest.finishReason = `Reached ${options.maxSteps} steps`;
      await writeFile(join(options.outputDir, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
      await writeFile(join(options.outputDir, "index.html"), renderScreenshotLog(manifest));
    }
    return manifest;
  } catch (error) {
    manifest.status = "error";
    manifest.finishedAt = new Date().toISOString();
    manifest.finishReason = error instanceof Error ? error.message : String(error);
    await writeFile(join(options.outputDir, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
    await writeFile(join(options.outputDir, "index.html"), renderScreenshotLog(manifest));
    throw error;
  } finally {
    await browser.close();
  }
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const result = await run(options);
  process.stdout.write(`${result.status}: ${join(resolve(options.outputDir), "index.html")}\n`);
  if (result.status !== "completed") process.exitCode = 1;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
}
