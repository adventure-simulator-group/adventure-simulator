const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");

const {
  extractStrategicNoticeMessage,
  safeStrategicErrorMessage,
} = require("../static/strategic-mutations.js");

const headers = (contentType) => ({
  get: (name) => name.toLowerCase() === "content-type" ? contentType : null,
});

test("extracts only the dedicated bounded strategic notice message", () => {
  const safe = {
    querySelector: (selector) => selector === "[data-strategic-safe-message]"
      ? { textContent: "  Rest is unavailable.\n Try later.  " }
      : null,
  };
  assert.equal(
    extractStrategicNoticeMessage(safe),
    "Rest is unavailable. Try later.",
  );
  assert.equal(
    extractStrategicNoticeMessage({
      querySelector: () => null,
      textContent: "<p>private reducer detail</p>",
    }),
    null,
  );
  assert.equal(
    extractStrategicNoticeMessage({
      querySelector: () => ({ textContent: "x".repeat(513) }),
    }),
    null,
  );
});

test("accepts marked HTML only from the current origin", async () => {
  let reads = 0;
  const response = {
    url: "https://game.example/settlements/willowmere/inn",
    headers: headers("text/html; charset=utf-8"),
    text: async () => { reads += 1; return "<safe notice>"; },
  };
  const parseHtml = () => ({
    querySelector: () => ({ textContent: "You cannot rest while travelling." }),
  });
  assert.equal(
    await safeStrategicErrorMessage(response, "https://game.example", parseHtml),
    "You cannot rest while travelling.",
  );
  assert.equal(reads, 1);

  assert.equal(
    await safeStrategicErrorMessage(
      { ...response, url: "https://other.example/error" },
      "https://game.example",
      parseHtml,
    ),
    null,
  );
  assert.equal(
    await safeStrategicErrorMessage(
      { ...response, headers: headers("text/plain") },
      "https://game.example",
      parseHtml,
    ),
    null,
  );
  assert.equal(reads, 1, "rejected responses are not read");
});

test("safe notice parsing fails closed but preserves cancellation", async () => {
  const base = {
    url: "https://game.example/error",
    headers: headers("text/html"),
  };
  assert.equal(
    await safeStrategicErrorMessage(
      { ...base, text: async () => { throw new Error("socket detail"); } },
      "https://game.example",
      () => { throw new Error("not reached"); },
    ),
    null,
  );
  assert.equal(
    await safeStrategicErrorMessage(
      { ...base, text: async () => "<html>" },
      "https://game.example",
      () => { throw new Error("malformed response detail"); },
    ),
    null,
  );

  const aborted = new Error("cancelled");
  aborted.name = "AbortError";
  await assert.rejects(
    safeStrategicErrorMessage(
      { ...base, text: async () => { throw aborted; } },
      "https://game.example",
      () => null,
    ),
    (error) => error === aborted,
  );
});

test("default mutation errors keep explicit overrides and the conflict fallback", () => {
  const source = fs.readFileSync(
    "crates/strategic-web/static/strategic-mutations.js",
    "utf8",
  );
  assert.match(source, /if \(errorMessageFromResponse\)/);
  assert.match(source, /safeMessage \|\| \(response\.status === 409/);
  assert.match(source, /The world changed before that action completed/);
  const errorBranch = source.split("if (!response.ok) {")[1].split("const text = await response.text()")[0];
  assert.match(errorBranch, /mine !== generation/);
  assert.match(
    errorBranch,
    /originPage !== document\.querySelector\("#strategic-page"\)/,
  );
  assert.ok(
    errorBranch.lastIndexOf("mine !== generation") > errorBranch.indexOf("safeStrategicErrorMessage"),
    "staleness is checked after asynchronous error parsing",
  );
});
