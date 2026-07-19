const assert = require("node:assert/strict");
const test = require("node:test");

const {
  applyChannelVisibility,
  appendInfo,
  chatTimestamp,
  createChannelRow,
  mergeChannelRows,
  pendingLocalRows,
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

test("Local reconciliation preserves private one-shot dialogue absent from durable history", () => {
  const request = row("local", undefined);
  request.dataset.privateDialogue = "true";
  const failure = row("local", undefined);
  failure.dataset.privateDialogue = "true";
  const diagnosis = row("local", undefined);
  diagnosis.dataset.privateDialogue = "true";

  const pending = pendingLocalRows([request, failure, diagnosis], []);
  const merged = mergeChannelRows([request, failure, diagnosis], "local", [], pending);

  assert.deepEqual(merged, [request, failure, diagnosis]);
});

const fakeDocument = () => {
  const ownerDocument = {
    createElement: (tagName) => ({
      tagName,
      className: "",
      dataset: {},
      children: [],
      ownerDocument,
      append(...children) { this.children.push(...children); },
    }),
    createTextNode: (textContent) => ({ textContent }),
  };
  return ownerDocument;
};

test("channel row creation includes content and timestamp without a channel badge", () => {
  const row = createChannelRow("guild", "Guild news", {}, fakeDocument());

  assert.equal(row.dataset.chatChannel, "guild");
  assert.equal(row.children[0].className, "chat-timestamp");
  assert.equal(row.children[1].textContent, "Guild news");
  assert.equal(row.children.some((child) => child.className === "chat-channel-badge"), false);
});

test("Info notices append without changing filter state", () => {
  const ownerDocument = fakeDocument();
  const messages = ownerDocument.createElement("div");
  messages.className = "settlement-chat-messages";
  messages.matches = (selector) => selector === ".settlement-chat-messages";
  messages.scrollHeight = 120;
  const filters = [{ checked: false }, { checked: true }];

  const inserted = appendInfo(messages, "25 gold has been added.");

  assert.equal(messages.children[0], inserted);
  assert.equal(inserted.dataset.chatChannel, "info");
  assert.deepEqual(filters.map((filter) => filter.checked), [false, true]);
  assert.equal(messages.scrollTop, 120);
  assert.equal(inserted.children.some((child) => child.className === "chat-channel-badge"), false);
});
