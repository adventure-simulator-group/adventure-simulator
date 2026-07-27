const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const source = fs.readFileSync(path.join(__dirname, "..", "static", "social-menu.js"), "utf8");
const { formatDuration, relationshipLabel } = require("../static/social-menu.js");

test("chat duration presentation supports quarter-hour activity controls", () => {
  assert.equal(formatDuration(15), "15 minutes");
  assert.equal(formatDuration(60), "1 hour");
  assert.equal(formatDuration(135), "2 hours 15 minutes");
  assert.match(source, /min = "15"/);
  assert.match(source, /max = "480"/);
  assert.match(source, /step = "15"/);
  assert.match(source, /requested_minutes/);
});

test("settlement NPC social menu uses safe qualitative projections", () => {
  assert.equal(relationshipLabel("well_known"), "well known");
  assert.doesNotMatch(source, /success_chance|personality_fit|morale_delta/);
  assert.match(source, /positive: "The conversation brings you closer/);
  assert.match(source, /negative: "The conversation leaves some friction/);
  assert.match(source, /data\.openNpcSocial|dataset\.openNpcSocial/);
});
