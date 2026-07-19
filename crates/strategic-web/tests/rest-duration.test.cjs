const assert = require("node:assert/strict");
const test = require("node:test");

const {
  formatClock,
  minutesUntilWake,
  targetForDuration,
} = require("../static/rest-duration.js");

test("08:00 wake preserves the full-day minimum at its boundaries", () => {
  assert.equal(minutesUntilWake(7 * 60 + 59, 8 * 60), 1441);
  assert.equal(minutesUntilWake(8 * 60, 8 * 60), 1440);
  assert.equal(minutesUntilWake(8 * 60 + 1, 8 * 60), 2879);
});

test("wake arithmetic preserves arbitrary current and target minutes", () => {
  assert.equal(minutesUntilWake(13 * 60 + 37, 6 * 60 + 12), 2435);
  assert.equal(formatClock(6 * 60 + 12), "06:12");
});

test("large typed hour values round to a minute and update target modulo day", () => {
  assert.equal(targetForDuration(7 * 60 + 59, 49.51), 9 * 60 + 30);
  assert.equal(targetForDuration(23 * 60 + 47, 24.016), 23 * 60 + 48);
});

test("markup keeps wake time settlement-only, accessible, and detached from days", () => {
  const source = require("node:fs").readFileSync("crates/strategic-web/src/templates/settlement.rs", "utf8");
  const settlementControl = source.slice(source.indexOf("fn settlement_rest_duration_control"));
  assert.match(settlementControl, /type="range"/);
  assert.match(settlementControl, /aria-label="Wake time"/);
  assert.match(settlementControl, /disabled\[!hours_active\]/);
  assert.doesNotMatch(source.slice(source.indexOf("pub fn camp_page"), source.indexOf("fn rest_duration_control")), /data-wake-time/);
});

test("controls remount after replacement and keep independent days state", () => {
  const fs = require("node:fs");
  const vm = require("node:vm");
  const source = fs.readFileSync("crates/strategic-web/static/rest-duration.js", "utf8");
  const documentListeners = {};
  let liveControls = [];

  const eventTarget = (properties = {}) => {
    const listeners = {};
    return {
      ...properties,
      addEventListener(type, listener) { (listeners[type] ||= []).push(listener); },
      dispatchEvent(event) { (listeners[event.type] || []).forEach((listener) => listener(event)); },
      setAttribute(name, value) { this[name] = String(value); },
    };
  };
  const makeControl = (unit, value) => {
    const submit = { disabled: false };
    const form = { querySelector: () => submit };
    const labels = [{ classList: { toggle() {} } }, { classList: { toggle() {} } }];
    const radios = ["hours", "days"].map((radioUnit, index) => eventTarget({
      checked: radioUnit === unit,
      value: radioUnit,
      closest: () => labels[index],
    }));
    const duration = eventTarget({ value: String(value), min: "", max: "", step: "" });
    const exact = { value: "", disabled: false };
    const slider = eventTarget({ value: "480", disabled: false });
    const output = { value: "", textContent: "" };
    const panel = { setAttribute(name, panelValue) { this[name] = String(panelValue); } };
    const buttons = [eventTarget({ dataset: { restStep: "-1" } }), eventTarget({ dataset: { restStep: "1" } })];
    const bySelector = new Map([
      ["[data-rest-duration-input]", duration], ["[data-rest-exact-minutes]", exact],
      ["[data-wake-time-slider]", slider], ["[data-wake-time-output]", output],
      ["[data-wake-time-panel]", panel], ["[data-rest-unit-label]", { textContent: "" }],
    ]);
    const control = {
      dataset: {},
      closest: () => form,
      querySelector: (selector) => bySelector.get(selector),
      querySelectorAll: (selector) => selector.includes("radio") ? radios : buttons,
    };
    return { control, duration, exact, radios, slider, submit };
  };
  const document = {
    querySelectorAll: () => liveControls.map(({ control }) => control),
    addEventListener(type, listener) { (documentListeners[type] ||= []).push(listener); },
  };
  const window = { strategicCharacterMinutes: 480 };
  class Event { constructor(type) { this.type = type; } }
  const first = makeControl("hours", 24);
  liveControls = [first];
  vm.runInNewContext(source, { document, window, Event, Number, Math, String, WeakMap, WeakSet });

  first.radios[0].checked = false;
  first.radios[1].checked = true;
  first.radios[1].dispatchEvent(new Event("change"));
  assert.equal(first.duration.value, "1");
  assert.equal(first.exact.disabled, true);
  assert.equal(window.strategicRestDuration.isDirty(document), true);

  const replacement = makeControl("hours", 24);
  liveControls = [replacement];
  documentListeners["strategic-live-regions-refreshed"][0](new Event("strategic-live-regions-refreshed"));
  assert.equal(replacement.duration.value, "24");
  assert.equal(replacement.submit.disabled, false);
  assert.equal(replacement.control.dataset.wakeTimeMounted, undefined);
});
