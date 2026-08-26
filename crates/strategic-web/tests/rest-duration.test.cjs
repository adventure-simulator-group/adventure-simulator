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

test("rest summaries receive focus, contain Tab, and dismiss with Escape", () => {
  const source = require("node:fs").readFileSync(
    "crates/strategic-web/static/rest-duration.js",
    "utf8",
  );
  assert.match(source, /mountRestSummary/);
  assert.match(source, /summary\.focus\(\)/);
  assert.match(source, /event\.key === "Escape"/);
  assert.match(source, /event\.key !== "Tab"/);
  assert.match(source, /rest-summary-close/);
});

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

test("settlement rest uses whole days while field rest retains the wake-time control", () => {
  const source = require("node:fs").readFileSync("crates/strategic-web/src/templates/settlement/rest.rs", "utf8");
  const settlementControl = source.slice(
    source.indexOf("fn settlement_rest_duration_control"),
    source.indexOf("fn wake_time_rest_duration_control"),
  );
  assert.match(settlementControl, /name="unit" value="days"/);
  assert.match(settlementControl, /type="number"[\s\S]+min="1" max="365" step="1"/);
  assert.doesNotMatch(settlementControl, /wake_time_rest_duration_control|type="range"/);
  const partyControl = source.slice(source.indexOf("pub(crate) fn party_rest_menu"), source.indexOf("pub fn rest_service_menu"));
  assert.match(partyControl, /wake_time_rest_duration_control/);
  assert.match(source, /data-rest-minimum-minutes/);
  assert.match(source, /data-rest-default-minutes/);
  assert.match(source, /data-rest-scheduled-wake-minute/);
});

test("wake slider uses the travel rail's solid time-period colors", () => {
  const css = require("node:fs").readFileSync("crates/strategic-web/static/css/strategic.css", "utf8");
  const tracks = [...css.matchAll(/\.rest-wake-time input\[type="range"\]::(?:-webkit-slider-runnable-track|-moz-range-track) \{([\s\S]*?)\}/g)];
  assert.equal(tracks.length, 2);
  for (const [, rule] of tracks) {
    assert.match(rule, /#14223a 0 25%/);
    assert.match(rule, /#c98255 25% 33\.333%/);
    assert.match(rule, /#d9b95f 33\.333% 75%/);
    assert.match(rule, /#b96850 75% 83\.333%/);
    assert.doesNotMatch(rule, /#4ba6e6|#59b9f4|#4a9fdd/);
  }
});

test("controls remount when raw DOM patches repeatedly replace only their descendants", () => {
  const fs = require("node:fs");
  const vm = require("node:vm");
  const source = fs.readFileSync("crates/strategic-web/static/rest-duration.js", "utf8");
  const documentListeners = {};
  let mutationCallback;
  let liveControls = [];

  const eventTarget = (properties = {}) => {
    const listeners = {};
    return {
      ...properties,
      addEventListener(type, listener, options = {}) {
        (listeners[type] ||= []).push({ listener, signal: options.signal });
      },
      dispatchEvent(event) {
        (listeners[event.type] || []).forEach(({ listener, signal }) => {
          if (!signal?.aborted) listener(event);
        });
      },
      setAttribute(name, value) { this[name] = String(value); },
    };
  };
  const makeControl = (unit, value, dataset = {}) => {
    let submit;
    let control;
    const form = {
      querySelector: () => submit,
      querySelectorAll: (selector) => selector === "[data-wake-time]" ? [control] : [],
    };
    let bySelector;
    let radios;
    let buttons;
    control = {
      dataset,
      closest: (selector) => selector === "form" ? form : null,
      matches: (selector) => selector === "[data-wake-time]",
      querySelector: (selector) => bySelector.get(selector),
      querySelectorAll: (selector) => {
        if (selector.includes("radio")) return radios;
        if (selector === "[data-rest-step]") return buttons;
        return [];
      },
    };
    const fixture = { control };
    fixture.replaceSubmit = () => {
      submit = { disabled: false, closest: (selector) => selector === "form" ? form : null };
      fixture.submit = submit;
    };
    fixture.replaceChildren = (nextUnit, nextValue) => {
      const labels = [{ classList: { toggle() {} } }, { classList: { toggle() {} } }];
      const owner = (selector) => selector === "[data-wake-time]" ? control : null;
      radios = ["hours", "days"].map((radioUnit, index) => eventTarget({
        checked: radioUnit === nextUnit,
        value: radioUnit,
        closest: (selector) => selector === "label" ? labels[index] : owner(selector),
      }));
      const duration = eventTarget({
        value: String(nextValue), min: "", max: "", step: "", closest: owner,
      });
      const exact = { value: "", disabled: false };
      const slider = eventTarget({
        value: "480", step: "60", disabled: false, closest: owner,
      });
      const output = { value: "", textContent: "" };
      const panel = { setAttribute(name, panelValue) { this[name] = String(panelValue); } };
      buttons = ["-1", "1"].map((restStep) => eventTarget({
        dataset: { restStep }, closest: owner,
      }));
      bySelector = new Map([
        ["[data-rest-duration-input]", duration], ["[data-rest-exact-minutes]", exact],
        ["[data-wake-time-slider]", slider], ["[data-wake-time-output]", output],
        ["[data-wake-time-panel]", panel], ["[data-rest-unit-label]", { textContent: "" }],
      ]);
      Object.assign(fixture, { buttons, duration, exact, radios, slider });
    };
    fixture.replaceSubmit();
    fixture.replaceChildren(unit, value);
    return fixture;
  };
  const document = {
    documentElement: {},
    querySelectorAll: () => liveControls.map(({ control }) => control),
    addEventListener(type, listener) { (documentListeners[type] ||= []).push(listener); },
  };
  class MutationObserver {
    constructor(callback) { mutationCallback = callback; }
    observe() {}
  }
  class AbortController {
    constructor() { this.signal = { aborted: false }; }
    abort() { this.signal.aborted = true; }
  }
  const window = { strategicCharacterMinutes: 480 };
  class Event {
    constructor(type, options = {}) { this.type = type; Object.assign(this, options); }
    preventDefault() { this.defaultPrevented = true; }
  }
  const first = makeControl("hours", "24:00");
  liveControls = [first];
  vm.runInNewContext(source, {
    AbortController, document, window, Event, MutationObserver, Number, Math, String, WeakMap, WeakSet,
  });

  first.radios[0].checked = false;
  first.radios[1].checked = true;
  first.radios[1].dispatchEvent(new Event("change"));
  assert.equal(first.duration.value, "1");
  assert.equal(first.exact.disabled, true);
  assert.equal(window.strategicRestDuration.isDirty(document), true);

  const exerciseReplacement = () => {
    first.replaceChildren("hours", "24:00");
    const record = [{ addedNodes: [first.duration] }];
    mutationCallback(record);
    mutationCallback(record);
    assert.equal(first.duration.value, "24:00");
    assert.equal(first.submit.disabled, false);

    first.duration.value = "25:30";
    first.duration.dispatchEvent(new Event("input"));
    assert.equal(first.exact.value, "1530");
    assert.equal(first.slider.value, 570);

    first.buttons[1].dispatchEvent(new Event("click"));
    assert.equal(first.duration.value, "26:30", "one click has one handler");
    first.buttons[0].dispatchEvent(new Event("click"));
    assert.equal(first.duration.value, "25:30");

    first.radios[0].checked = false;
    first.radios[1].checked = true;
    first.radios[1].dispatchEvent(new Event("change"));
    first.duration.value = "5";
    first.duration.dispatchEvent(new Event("input"));
    first.buttons[1].dispatchEvent(new Event("click"));
    assert.equal(first.duration.value, "6", "days increment once");
    first.buttons[0].dispatchEvent(new Event("click"));
    assert.equal(first.duration.value, "5");

    first.radios[0].checked = true;
    first.radios[1].checked = false;
    first.radios[0].dispatchEvent(new Event("change"));
    assert.equal(first.duration.value, "25:30");
    first.slider.dispatchEvent(new Event("keydown", { key: "ArrowRight" }));
    assert.equal(first.slider.value, 600);
    assert.equal(first.duration.value, "26:00");
  };

  exerciseReplacement();
  exerciseReplacement();

  first.replaceSubmit();
  mutationCallback([{ type: "childList", addedNodes: [first.submit] }]);
  first.duration.value = "invalid";
  first.duration.dispatchEvent(new Event("input"));
  assert.equal(first.submit.disabled, true, "replacement submit receives invalid state");
  first.duration.value = "26:00";
  first.duration.dispatchEvent(new Event("input"));
  assert.equal(first.submit.disabled, false, "replacement submit receives valid state");

  first.control.dataset.restMinimumMinutes = "2880";
  mutationCallback([{ type: "attributes", target: first.control }]);
  first.duration.value = "25:00";
  first.duration.dispatchEvent(new Event("input"));
  assert.equal(first.submit.disabled, true, "updated minimum applies after attribute-only morph");
  first.duration.value = "48:00";
  first.duration.dispatchEvent(new Event("input"));
  assert.equal(first.submit.disabled, false);

  const scheduled = makeControl("hours", "16:00", {
    restDefaultMinutes: "960",
    restMinimumMinutes: "1",
    restScheduledWakeMinute: "480",
  });
  liveControls = [scheduled];
  documentListeners["strategic-live-regions-refreshed"][0](new Event("strategic-live-regions-refreshed"));
  documentListeners["strategic-time-ready"][0](new Event("strategic-time-ready", { detail: { characterMinutes: 960 } }));
  assert.equal(scheduled.slider.value, 480);
  assert.equal(scheduled.duration.value, "16:00");
  documentListeners["strategic-time-ready"][0](new Event("strategic-time-ready", { detail: { characterMinutes: 1020 } }));
  assert.equal(scheduled.slider.value, 480);
  assert.equal(scheduled.duration.value, "15:00");
  documentListeners["strategic-time-ready"][0](new Event("strategic-time-ready", { detail: { characterMinutes: 1930 } }));
  assert.equal(scheduled.slider.value, 480);
  assert.equal(scheduled.duration.value, "23:50");
});
