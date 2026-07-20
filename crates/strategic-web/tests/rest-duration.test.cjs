const assert = require("node:assert/strict");
const test = require("node:test");

const {
  formatClock,
  formatDuration,
  minutesUntilWake,
  minutesUntilWakeWithMinimum,
  parseDuration,
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

test("field rest can target the next clock occurrence without a full-day minimum", () => {
  assert.equal(minutesUntilWakeWithMinimum(7 * 60, 8 * 60, 1), 60);
  assert.equal(minutesUntilWakeWithMinimum(8 * 60, 8 * 60, 1), 1440);
  assert.equal(minutesUntilWakeWithMinimum(22 * 60, 2 * 60, 1), 240);
});

test("typed HH:MM durations preserve exact minutes and update target modulo day", () => {
  assert.equal(parseDuration("49:31"), 49 * 60 + 31);
  assert.equal(formatDuration(45 * 60 + 30), "45:30");
  assert.equal(targetForDuration(7 * 60 + 59, 49 * 60 + 31), 9 * 60 + 30);
  assert.equal(targetForDuration(23 * 60 + 47, 24 * 60 + 1), 23 * 60 + 48);
  assert.equal(parseDuration("24:60"), null);
  assert.equal(parseDuration("24.5"), null);
});

test("markup reuses the accessible wake-time control for settlement and field rest", () => {
  const source = require("node:fs").readFileSync("crates/strategic-web/src/templates/settlement.rs", "utf8");
  const settlementControl = source.slice(source.indexOf("fn settlement_rest_duration_control"));
  assert.match(settlementControl, /type="range"/);
  assert.match(settlementControl, /step="60"/);
  assert.match(settlementControl, /pattern="\[0-9\]\+:\[0-5\]\[0-9\]"/);
  assert.match(settlementControl, /aria-label="Wake time"/);
  assert.match(settlementControl, /disabled\[!hours_active\]/);
  const partyControl = source.slice(source.indexOf("pub(crate) fn party_rest_menu"), source.indexOf("pub(crate) fn settlement_description"));
  assert.match(partyControl, /wake_time_rest_duration_control/);
  assert.match(source, /data-rest-minimum-minutes/);
  assert.match(source, /data-rest-default-minutes/);
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
    const slider = eventTarget({ value: "480", step: "60", disabled: false });
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
    return { buttons, control, duration, exact, radios, slider, submit };
  };
  const document = {
    querySelectorAll: () => liveControls.map(({ control }) => control),
    addEventListener(type, listener) { (documentListeners[type] ||= []).push(listener); },
  };
  const window = { strategicCharacterMinutes: 480 };
  class Event {
    constructor(type, options = {}) { this.type = type; Object.assign(this, options); }
    preventDefault() { this.defaultPrevented = true; }
  }
  const first = makeControl("hours", "24:00");
  liveControls = [first];
  vm.runInNewContext(source, { document, window, Event, Number, Math, String, WeakMap, WeakSet });

  first.radios[0].checked = false;
  first.radios[1].checked = true;
  first.radios[1].dispatchEvent(new Event("change"));
  assert.equal(first.duration.value, "1");
  assert.equal(first.exact.disabled, true);
  assert.equal(window.strategicRestDuration.isDirty(document), true);

  const replacement = makeControl("hours", "24:00");
  liveControls = [replacement];
  documentListeners["strategic-live-regions-refreshed"][0](new Event("strategic-live-regions-refreshed"));
  assert.equal(replacement.duration.value, "24:00");
  assert.equal(replacement.submit.disabled, false);
  assert.equal(replacement.control.dataset.wakeTimeMounted, undefined);

  replacement.duration.value = "25:30";
  replacement.duration.dispatchEvent(new Event("input"));
  assert.equal(replacement.exact.value, "1530");
  assert.equal(replacement.slider.value, 570);
  assert.equal(replacement.slider.step, "1");

  replacement.slider.dispatchEvent(new Event("keydown", { key: "ArrowRight" }));
  assert.equal(replacement.slider.value, 600);
  assert.equal(replacement.duration.value, "26:00");

  replacement.slider.dispatchEvent(new Event("pointerdown"));
  assert.equal(replacement.slider.step, "60");

  replacement.buttons[1].dispatchEvent(new Event("click"));
  assert.equal(replacement.duration.value, "27:00");
  assert.equal(replacement.exact.value, "1620");
});
