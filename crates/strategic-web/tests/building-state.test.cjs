const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");
const { parseHTML } = require("linkedom");

const source = fs.readFileSync(path.join(__dirname, "..", "static", "building-state.js"), "utf8");

function fixture(href) {
  const { window, document } = parseHTML(`<html><body>
    <main id="strategic-page">
      <nav data-settlement-id="lubeck">
        <a class="nav-tab" data-service-id="map" data-building-id="map"></a>
        <a class="nav-tab" data-service-id="inn" data-building-id="inn"></a>
        <a class="nav-tab active" data-service-id="organization"
          data-building-id="organization-merchants-lubeck"></a>
      </nav>
      <a id="party-link" href="/locations/settlement/lubeck/party/7">Party</a>
      <form id="party-form" action="/locations/settlement/lubeck/party/7/social"></form>
    </main>
  </body></html>`);
  const location = { href, origin: "http://game.test" };
  const replacements = [];
  const history = {
    state: null,
    replaceState(_state, _title, url) {
      replacements.push(url.toString());
    },
  };
  vm.runInNewContext(source, {
    document,
    location,
    history,
    URL,
    MutationObserver: window.MutationObserver,
    Node: window.Node,
  });
  return { window, document, location, replacements };
}

test("exact organization state survives links, forms, remounts, and live insertion", async () => {
  const view = fixture(
    "http://game.test/locations/settlement/lubeck/party/7?building=organization-merchants-lubeck",
  );
  const organization = view.document.querySelector('[data-building-id="organization-merchants-lubeck"]');
  assert.equal(organization.classList.contains("active"), true);
  assert.equal(view.document.querySelectorAll(".nav-tab.active").length, 1);
  assert.equal(view.document.querySelector("#party-link").getAttribute("href"),
    "/locations/settlement/lubeck/party/7?building=organization-merchants-lubeck");
  assert.equal(view.document.querySelector("#party-form").getAttribute("action"),
    "/locations/settlement/lubeck/party/7/social?building=organization-merchants-lubeck");

  view.document.dispatchEvent(new view.window.Event("strategic-page-mounted"));
  assert.equal(organization.classList.contains("active"), true);
  assert.equal(view.document.querySelectorAll(".nav-tab.active").length, 1);
  const live = view.document.createElement("a");
  live.href = "/locations/settlement/lubeck/party/9";
  view.document.querySelector("#strategic-page").append(live);
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(live.getAttribute("href"),
    "/locations/settlement/lubeck/party/9?building=organization-merchants-lubeck");
});

test("an invalid requested identity is removed and cannot become active", () => {
  const view = fixture(
    "http://game.test/locations/settlement/lubeck/party/7?building=organization-foreign-town",
  );
  assert.equal(view.replacements.length, 1);
  assert.equal(view.replacements[0], "http://game.test/locations/settlement/lubeck/party/7");
  assert.equal(view.document.querySelector('[data-building-id="organization-foreign-town"]'), null);
  assert.equal(view.document.querySelectorAll(".nav-tab.active").length, 1);
});

test("fireplace building state survives mount and remount without rewriting history", () => {
  const view = fixture(
    "http://game.test/locations/settlement/lubeck/fireplace?building=inn",
  );
  const inn = view.document.querySelector('[data-building-id="inn"]');
  assert.equal(inn.classList.contains("active"), true);
  assert.deepEqual(view.replacements, []);

  view.document.dispatchEvent(new view.window.Event("strategic-page-mounted"));
  assert.equal(inn.classList.contains("active"), true);
  assert.equal(view.document.querySelectorAll(".nav-tab.active").length, 1);
  assert.deepEqual(view.replacements, []);
});

test("an invalid fireplace building is removed", () => {
  const view = fixture(
    "http://game.test/locations/settlement/lubeck/fireplace?building=forge",
  );
  assert.equal(view.replacements.length, 1);
  assert.equal(
    view.replacements[0],
    "http://game.test/locations/settlement/lubeck/fireplace",
  );
  assert.equal(view.document.querySelectorAll(".nav-tab.active").length, 1);
});
