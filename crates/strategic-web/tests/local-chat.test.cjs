const assert = require("node:assert/strict");
const test = require("node:test");

const {
  applyChannelVisibility,
  chatTimestamp,
  decorateMessage,
  mergeChannelRows,
} = require("../static/local-chat.js");

const row = (channel, id, createdMicros) => ({
  dataset: {
    chatChannel: channel,
    chatMessageId: id,
    ...(createdMicros === undefined ? {} : { chatCreatedMicros: String(createdMicros) }),
  },
  hidden: false,
});

test("channel visibility applies to existing and dynamically inserted messages", () => {
  const local = row("local", "local-1", 100);
  const party = row("party", "party-1", 200);
  const info = row("info", "info-1", 300);

  applyChannelVisibility([local, party], new Set(["local"]));
  assert.equal(local.hidden, false);
  assert.equal(party.hidden, true);

  applyChannelVisibility([local, party, info], new Set(["local", "info"]));
  assert.equal(info.hidden, false);
  assert.equal(party.hidden, true);
});

test("Local reconciliation preserves other channels and chronological order", () => {
  const staleLocal = row("local", "local-old", 50);
  const party = row("party", "party-1", 300);
  const info = row("info", "info-1", 200);
  const replacementLocal = row("local", "local-1", 100);
  const pendingLocal = row("local", "local-pending");

  const merged = mergeChannelRows(
    [staleLocal, party, info],
    "local",
    [replacementLocal],
    [pendingLocal],
  );

  assert.deepEqual(merged, [replacementLocal, info, party, pendingLocal]);
  assert.equal(merged.includes(staleLocal), false);
});

test("message IDs provide deterministic ordering when timestamps match", () => {
  const laterId = row("party", "10", 100);
  const earlierId = row("local", "2", 100);

  const merged = mergeChannelRows([laterId], "local", [earlierId]);

  assert.deepEqual(merged, [earlierId, laterId]);
  assert.equal(chatTimestamp(row("info", "pending")), Number.POSITIVE_INFINITY);
});

test("message decoration adds a visible channel label beside the timestamp", () => {
  const inserted = [];
  const timestamp = { after: (element) => inserted.push(element) };
  const message = {
    dataset: { chatChannel: "guild" },
    prepend: (element) => inserted.unshift(element),
    querySelector: (selector) => selector === ".chat-timestamp" ? timestamp : null,
  };
  const ownerDocument = {
    createElement: () => ({ className: "", textContent: "" }),
  };

  decorateMessage(message, ownerDocument);

  assert.equal(inserted.length, 1);
  assert.equal(inserted[0].className, "chat-channel-badge");
  assert.equal(inserted[0].textContent, "[Guild] ");
});
