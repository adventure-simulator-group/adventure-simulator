import assert from "node:assert/strict";
import test from "node:test";
import { effectiveGripRadius, MAX_ROUND_GRIP_RADIUS_M, MAX_SWORD_GRIP_THICKNESS_M, MAX_SWORD_GRIP_WIDTH_M } from "../src/anatomy.js";
import { prism, resolveDefinition, tubePath, validateWeapon } from "../src/mesh.js";
import { HAFT_MODULES, HEAD_ASSEMBLIES, PRESETS, composeWeapon, compositionControls, copyPreset, getPath, setControlValue } from "../src/presets.js";

function randomGenerator(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state * 1664525 + 1013904223) >>> 0;
    return state / 0x100000000;
  };
}

function sampleControl(control, random) {
  const steps = Math.max(0, Math.floor((control.max - control.min) / control.step + 1e-8));
  return Number((control.min + Math.floor(random() * (steps + 1)) * control.step).toFixed(10));
}

function assertValid(definition, controls, context) {
  const result = validateWeapon(definition, controls);
  assert.equal(result.valid, true, `${context}: ${result.errors.join(" | ")}`);
}

test("weapon grips stay within their cross-section-specific anatomical envelopes", () => {
  for (const preset of PRESETS) {
    if (preset.definition.shaft) assert.ok(effectiveGripRadius(preset.definition.shaft) <= MAX_ROUND_GRIP_RADIUS_M, `${preset.id} shaft`);
    for (const component of preset.definition.components.filter((candidate) => candidate.kind === "grip")) {
      assert.ok(effectiveGripRadius(component) <= MAX_ROUND_GRIP_RADIUS_M, `${preset.id} grip`);
    }
    for (const component of preset.definition.components.filter((candidate) => candidate.kind === "ovalGrip")) {
      const scale = Math.max(component.bottomScale ?? 1, component.topScale ?? 1);
      assert.ok(component.width * scale <= MAX_SWORD_GRIP_WIDTH_M, `${preset.id} grip width`);
      assert.ok(component.thickness * scale <= MAX_SWORD_GRIP_THICKNESS_M, `${preset.id} grip thickness`);
      assert.ok(component.width > component.thickness, `${preset.id} grip orientation`);
    }
    for (const control of preset.controls.filter((candidate) => candidate.label === "Grip radius")) {
      assert.equal(control.max, MAX_ROUND_GRIP_RADIUS_M, `${preset.id} control maximum`);
    }
    for (const control of preset.controls.filter((candidate) => candidate.label === "Shaft thickness")) {
      assert.ok(control.max * Math.max(preset.definition.shaft.bottomScale, preset.definition.shaft.topScale) <= MAX_ROUND_GRIP_RADIUS_M, `${preset.id} shaft control maximum`);
    }
  }

  const longsword = copyPreset(PRESETS.find((preset) => preset.id === "landsknecht-longsword"));
  const grip = longsword.definition.components.find((component) => component.kind === "ovalGrip");
  assert.equal(grip.width, 0.033);
  assert.equal(grip.thickness, 0.024);
  grip.width = MAX_SWORD_GRIP_WIDTH_M + 0.001;
  const result = validateWeapon(longsword.definition);
  assert.equal(result.valid, false);
  assert.ok(result.errors.some((error) => error.includes("anatomical maximum")));

  const halberd = copyPreset(PRESETS.find((preset) => preset.id === "halberd-1540"));
  halberd.definition.shaft.radius = 0.028;
  assert.ok(validateWeapon(halberd.definition).errors.some((error) => error.includes("anatomical maximum")));
});

test("polearm haft families retain appropriate round or octagonal sections", () => {
  for (const id of ["halberd-1540", "lucerne-hammer", "pollaxe", "hooked-bill"]) {
    const preset = PRESETS.find((candidate) => candidate.id === id);
    assert.equal(preset.definition.shaft.segments, 8, `${id} should use an octagonal haft`);
  }
  for (const id of ["kriegsspiess", "short-spear", "partisan", "glaive", "military-fork"]) {
    const preset = PRESETS.find((candidate) => candidate.id === id);
    assert.equal(preset.definition.shaft.segments, 16, `${id} should use a round haft`);
  }
});

test("shared head families survive combined minimum, midpoint, and maximum shapes", () => {
  const families = [
    ["halberd-1540", 1, "axe"],
    ["short-spear", 1, "spear"],
    ["lucerne-hammer", 1, "hammer"],
    ["halberd-1540", 2, "beak"],
    ["partisan", 1, "partisan"],
    ["glaive", 1, "glaive"],
    ["hooked-bill", 1, "bill"],
    ["military-fork", 1, "fork"],
  ];
  for (const [id, componentIndex, family] of families)
    for (const endpoint of ["min", "mid", "max"]) {
      const preset = copyPreset(PRESETS.find((candidate) => candidate.id === id));
      const controls = preset.controls.filter((control) => (control.paths ?? [control.path]).some((path) => path.startsWith(`components.${componentIndex}.`)));
      for (const control of controls) {
        const stepCount = Math.round((control.max - control.min) / control.step);
        const value = endpoint === "min" ? control.min : endpoint === "max" ? control.max : Number((control.min + Math.round(stepCount / 2) * control.step).toFixed(10));
        setControlValue(preset.definition, control, value);
      }
      assertValid(preset.definition, preset.controls, `${family} ${endpoint}`);
    }
});

test("seeded slider fuzz keeps every preset structurally valid", () => {
  const random = randomGenerator(0x1544cafe);
  for (const source of PRESETS) {
    assertValid(source.definition, source.controls, `${source.id} default`);
    for (const control of source.controls)
      for (const value of [control.min, control.max, sampleControl(control, random)]) {
        const preset = copyPreset(source);
        setControlValue(preset.definition, control, value);
        assertValid(preset.definition, preset.controls, `${source.id} ${control.label}=${value}`);
      }
    for (let index = 0; index < source.controls.length - 1; index += 1) {
      const preset = copyPreset(source),
        pair = source.controls.slice(index, index + 2);
      for (const control of pair) setControlValue(preset.definition, control, sampleControl(control, random));
      assertValid(preset.definition, preset.controls, `${source.id} pair ${pair.map((control) => control.label).join("+")}`);
    }
    for (let pass = 0; pass < 2 && source.controls.length; pass += 1) {
      const preset = copyPreset(source);
      for (const control of preset.controls) setControlValue(preset.definition, control, sampleControl(control, random));
      assertValid(preset.definition, preset.controls, `${source.id} multi ${pass}`);
    }
  }
});

test("every preset control pair survives all four endpoint combinations", () => {
  let cases = 0;
  for (const source of PRESETS) {
    for (let first = 0; first < source.controls.length; first += 1)
      for (let second = first + 1; second < source.controls.length; second += 1) {
        for (const firstValue of [source.controls[first].min, source.controls[first].max])
          for (const secondValue of [source.controls[second].min, source.controls[second].max]) {
            const preset = copyPreset(source);
            setControlValue(preset.definition, preset.controls[first], firstValue);
            setControlValue(preset.definition, preset.controls[second], secondValue);
            assertValid(preset.definition, preset.controls, `pairwise case ${cases} preset=${source.id} ${preset.controls[first].label}=${firstValue} ${preset.controls[second].label}=${secondValue}`);
            cases += 1;
          }
      }
    for (const endpoint of ["min", "max"]) {
      const preset = copyPreset(source);
      for (const control of preset.controls) setControlValue(preset.definition, control, control[endpoint]);
      assertValid(preset.definition, preset.controls, `${source.id} all-${endpoint}`);
      cases += 1;
    }
    for (const seed of [0x1544a11, 0x1544b22, 0x1544c33]) {
      const random = randomGenerator(seed ^ source.id.length),
        preset = copyPreset(source);
      for (const control of preset.controls) setControlValue(preset.definition, control, sampleControl(control, random));
      assertValid(preset.definition, preset.controls, `seed=${seed} preset=${source.id}`);
      cases += 1;
    }
  }
  assert.ok(cases >= 7532, `expected reviewer-scale coverage, got ${cases}`);
});

test("Messer grip length drives guard, blade, and Nagel attachment frames", () => {
  const preset = copyPreset(PRESETS.find((candidate) => candidate.id === "grosse-messer")),
    control = preset.controls.find((candidate) => candidate.label === "Grip length");
  const before = resolveDefinition(preset.definition),
    delta = 0.05;
  setControlValue(preset.definition, control, getPath(preset.definition, control.path) + delta);
  assertValid(preset.definition, preset.controls, "Messer extended grip");
  const after = resolveDefinition(preset.definition);
  for (const frame of ["guard.center", "blade.base", "nagel-stem.base", "nagel-button.base"]) assert.ok(Math.abs(after._frames[frame][1] - before._frames[frame][1] - delta) < 1e-8, frame);
  assert.ok(Math.abs(after._frames["guard.center"][1] - after._frames["grip.top"][1]) < 1e-8);
  assert.ok(Math.abs(after._frames["blade.base"][1] - after._frames["guard.center"][1]) < 1e-8);
});

test("every independent haft and head module composes through shared frames", () => {
  for (const haft of HAFT_MODULES)
    for (const head of HEAD_ASSEMBLIES) {
      const definition = composeWeapon(haft.id, head.id),
        controls = compositionControls(definition),
        result = validateWeapon(definition, controls);
      assert.equal(result.valid, true, `${haft.id}+${head.id}: ${result.errors.join(" | ")}`);
      assert.ok(result.mesh.stats.partCount >= 3);
    }
  const poleMaceDefinition = composeWeapon("wooden-polearm", "flanged-mace"),
    poleMace = validateWeapon(poleMaceDefinition, compositionControls(poleMaceDefinition));
  assert.equal(poleMace.valid, true, poleMace.errors.join(" | "));
  assert.ok(poleMace.mesh.stats.dimensions[1] > 1.9);
});

test("composed modules survive deterministic key-dimension fuzzing", () => {
  const random = randomGenerator(0x1544beef);
  for (const haft of HAFT_MODULES)
    for (const head of HEAD_ASSEMBLIES)
      for (let pass = 0; pass < 4; pass += 1) {
        const definition = composeWeapon(haft.id, head.id),
          controls = compositionControls(definition);
        for (const control of controls) setControlValue(definition, control, sampleControl(control, random));
        assertValid(definition, controls, `${haft.id}+${head.id} fuzz ${pass}`);
      }
});

test("all composer controls survive combined and pairwise endpoints", () => {
  for (const haft of HAFT_MODULES)
    for (const head of HEAD_ASSEMBLIES) {
      const source = composeWeapon(haft.id, head.id),
        controls = compositionControls(source);
      for (const endpoint of ["min", "max"]) {
        const definition = structuredClone(source);
        for (const control of controls) setControlValue(definition, control, control[endpoint]);
        assertValid(definition, controls, `${haft.id}+${head.id} all-${endpoint}`);
      }
      for (let first = 0; first < controls.length; first += 1)
        for (let second = first + 1; second < controls.length; second += 1)
          for (const a of [controls[first].min, controls[first].max])
            for (const b of [controls[second].min, controls[second].max]) {
              const definition = structuredClone(source);
              setControlValue(definition, controls[first], a);
              setControlValue(definition, controls[second], b);
              assertValid(definition, controls, `${haft.id}+${head.id} ${controls[first].label}=${a} ${controls[second].label}=${b}`);
            }
    }
});

test("schema, radial fit, and actual contact reject malformed or detached definitions", () => {
  for (const malformed of [
    null,
    {},
    { components: [{ kind: "nonsense" }] },
    { components: [{ kind: "box", size: [1, 1, 1], mount: "teleport" }] },
    { components: [{ kind: "box", size: [1, 1, 1], material: "mithril" }] },
    { components: [{ kind: "box", size: [1, 1, 1], widht: 2 }] },
    { shaft: { length: 1, radius: 0.02, material: null }, components: [] },
    { shaft: { length: 1, radius: 0.02, topScale: null }, components: [] },
    {
      shaft: { length: 1, radius: 0.02 },
      components: [
        {
          kind: "spear",
          length: 0.3,
          width: 0.05,
          thickness: 0.02,
          face: 999,
          mount: "shaft-top",
        },
      ],
    },
  ]) {
    const result = validateWeapon(malformed);
    assert.equal(result.valid, false);
    assert.ok(result.errors.length);
  }
  const badFit = composeWeapon("wooden-polearm", "spear"),
    socket = badFit.components.find((part) => part.kind === "socket");
  socket.fitShaft = false;
  socket.profile = socket.profile.map(([y]) => [y, 0.02]);
  assert.ok(validateWeapon(badFit, compositionControls(badFit)).errors.some((error) => error.includes("cannot fit shaft")));
  const mace = composeWeapon("steel-one-hand", "flanged-mace");
  mace.components.find((part) => part.id === "head").offset = [4, 4, 4];
  assert.ok(validateWeapon(mace, compositionControls(mace)).errors.some((error) => error.includes("shaft footprint") || error.includes("concentrically")));
  const messer = copyPreset(PRESETS.find((preset) => preset.id === "grosse-messer"));
  messer.definition.components.find((part) => part.id === "nagel-stem").offset = [2, 0, 0];
  assert.ok(validateWeapon(messer.definition, messer.controls).errors.some((error) => error.includes("placement only through attach.offset")));
  for (const lateral of [0.02, 0.1, 0.2, 0.249, 0.251]) {
    const spear = composeWeapon("wooden-polearm", "spear");
    spear.components.find((part) => part.kind === "spear").offset = [lateral, 0, 0];
    assert.ok(
      validateWeapon(spear, compositionControls(spear)).errors.some((error) => error.includes("concentrically")),
      `lateral=${lateral}`,
    );
  }
  const bypass = composeWeapon("wooden-polearm", "spear");
  bypass.components.find((part) => part.kind === "spear").freeOffset = "yes";
  assert.ok(validateWeapon(bypass, compositionControls(bypass)).errors.some((error) => error.includes("does not allow field freeOffset")));
  const orphan = {
    shaft: { length: 1.5, radius: 0.025, segments: 12, material: "wood" },
    components: [
      {
        kind: "spear",
        label: "orphan",
        offset: [0.2, 1.5, 0],
        length: 0.3,
        width: 0.06,
        thickness: 0.02,
      },
    ],
  };
  assert.ok(validateWeapon(orphan).errors.some((error) => error.includes("must declare a mount")));
  const stretchedSpear = composeWeapon("wooden-polearm", "spear"),
    stretchedHead = stretchedSpear.components.find((part) => part.kind === "spear");
  delete stretchedHead.mount;
  stretchedHead.stretchBetween = ["shaft.bottom", "shaft.top"];
  stretchedHead.offset = [0.2, 0, 0];
  assert.ok(validateWeapon(stretchedSpear, compositionControls(stretchedSpear)).errors.some((error) => error.includes("only supported by knuckleBow")));
  for (const malformedStretch of [["grip.base"], [null, "grip.top"], ["grip.base", "grip.base"], ["teleport", "grip.top"]]) {
    const definition = copyPreset(PRESETS.find((preset) => preset.id === "dussack"));
    definition.definition.components.find((part) => part.kind === "knuckleBow").stretchBetween = malformedStretch;
    assert.ok(validateWeapon(definition.definition, definition.controls).errors.some((error) => error.includes("two distinct attachment frame strings")));
  }
  const displacedBow = copyPreset(PRESETS.find((preset) => preset.id === "dussack"));
  displacedBow.definition.components.find((part) => part.kind === "knuckleBow").offset = [0.2, 0, 0];
  assert.ok(validateWeapon(displacedBow.definition, displacedBow.controls).errors.some((error) => error.includes("stretched start") || error.includes("stretched end")));
  for (const malformedAttach of [
    [0, 0, null],
    ["0", 0, 0],
  ]) {
    const definition = copyPreset(PRESETS.find((preset) => preset.id === "grosse-messer"));
    definition.definition.components.find((part) => part.id === "nagel-stem").attach.offset = malformedAttach;
    assert.ok(validateWeapon(definition.definition, definition.controls).errors.some((error) => error.includes("attach.offset must be three finite numbers")));
  }
  const stringOverlap = copyPreset(PRESETS.find((preset) => preset.id === "grosse-messer"));
  stringOverlap.definition.components.find((part) => part.id === "nagel-stem").attach.overlap = "0.01";
  assert.ok(validateWeapon(stringOverlap.definition, stringOverlap.controls).errors.some((error) => error.includes("attach.overlap must be a non-negative finite number")));
  const rotatedGuard = copyPreset(PRESETS.find((preset) => preset.id === "grosse-messer"));
  rotatedGuard.definition.components.find((part) => part.id === "guard").rotation = [0, 90, 0];
  rotatedGuard.definition.components.find((part) => part.id === "nagel-stem").attach.offset = [0.1, 0, 0];
  assert.ok(validateWeapon(rotatedGuard.definition, rotatedGuard.controls).errors.some((error) => error.includes("parent footprint guard.center")));
  const conflictingSpear = composeWeapon("wooden-polearm", "spear"),
    conflictingHead = conflictingSpear.components.find((part) => part.kind === "spear");
  conflictingHead.mount = "component-end";
  conflictingHead.anchor = "head-socket";
  conflictingHead.attach = { to: "weapon.root", at: "base" };
  assert.ok(validateWeapon(conflictingSpear, compositionControls(conflictingSpear)).errors.some((error) => error.includes("mutually exclusive placement declarations")));
  for (const declarations of [
    { mount: "shaft-top", stretchBetween: ["grip.base", "grip.top"] },
    {
      attach: { to: "grip.base", at: "base" },
      stretchBetween: ["grip.base", "grip.top"],
    },
  ]) {
    const definition = copyPreset(PRESETS.find((preset) => preset.id === "dussack")),
      bow = definition.definition.components.find((part) => part.kind === "knuckleBow");
    Object.assign(bow, declarations);
    assert.ok(validateWeapon(definition.definition, definition.controls).errors.some((error) => error.includes("mutually exclusive placement declarations")));
  }
  const displacedOrigin = composeWeapon("steel-one-hand", "spear"),
    collar = displacedOrigin.components.find((part) => part.id === "composer-collar");
  collar.attach = {
    to: "composer-grip.origin",
    at: "center",
    offset: [0, -0.09, 0],
  };
  assert.ok(validateWeapon(displacedOrigin, compositionControls(displacedOrigin)).errors.some((error) => error.includes("outside parent axial geometry composer-grip.origin")));
  for (const frame of ["shaft.top", "shaft.bottom"]) {
    const shaftAttachment = composeWeapon("wooden-polearm", "spear"),
      head = shaftAttachment.components.find((part) => part.kind === "spear");
    delete head.mount;
    delete head.offset;
    head.attach = { to: frame, at: "base" };
    assertValid(shaftAttachment, compositionControls(shaftAttachment), `explicit ${frame} attachment`);
  }
  const explosive = new Proxy(
      {},
      {
        get() {
          throw new Error("hostile property access");
        },
      },
    ),
    total = validateWeapon(explosive);
  assert.equal(total.valid, false);
  assert.ok(total.errors.some((error) => error.includes("validation failed unexpectedly: hostile property access")));
});

test("named frames use rotated local anchors rather than unrotated axial ranges", () => {
  const definition = {
    components: [
      {
        kind: "grip",
        id: "parent",
        attach: { to: "weapon.root", at: "base" },
        length: 0.2,
        radius: 0.02,
        rotation: [0, 0, 90],
        material: "leather",
      },
      {
        kind: "collar",
        id: "child",
        attach: { to: "parent.top", at: "center" },
        width: 0.01,
        radius: 0.024,
        material: "steel",
      },
    ],
  };
  const resolved = resolveDefinition(definition),
    top = resolved._frames["parent.top"],
    contact = resolved.components.find((part) => part.id === "child")._resolvedAttachment.contact;
  assert.ok(Math.abs(top[0] + 0.2) < 1e-9 && Math.abs(top[1]) < 1e-9 && Math.abs(top[2]) < 1e-9, `rotated top=${top}`);
  assert.deepEqual(
    contact.map((value) => Number(value.toFixed(9))),
    top.map((value) => Number(value.toFixed(9))),
  );
});

test("tessellation and nonempty-part invariants reject structural hostile values", () => {
  for (const mutate of [
    (definition) => {
      definition.shaft.segments = 0;
    },
    (definition) => {
      definition.components.find((part) => part.kind === "socket").segments = 0;
    },
    (definition) => {
      definition.components.find((part) => part.kind === "spear").thickness = 1e-14;
    },
  ]) {
    const definition = composeWeapon("wooden-polearm", "spear");
    mutate(definition);
    const result = validateWeapon(definition, compositionControls(definition));
    assert.equal(result.valid, false);
    assert.ok(
      result.errors.some((error) => /segments|near-zero|volume/.test(error)),
      result.errors.join(" | "),
    );
  }
});

test("self-intersecting outlines and tube centerlines are rejected", () => {
  assert.throws(
    () =>
      prism(
        [
          [0, 0],
          [1, 1],
          [0, 1],
          [1, 0],
        ],
        0.1,
      ),
    /intersect/,
  );
  assert.throws(
    () =>
      tubePath(
        [
          [0, 0],
          [1, 1],
          [0, 1],
          [1, 0],
        ],
        0.02,
      ),
    /intersect/,
  );
});

test("every composed control changes geometry and retains shaft fit", () => {
  for (const haft of HAFT_MODULES)
    for (const head of HEAD_ASSEMBLIES) {
      const base = composeWeapon(haft.id, head.id),
        controls = compositionControls(base),
        baseline = validateWeapon(base, controls);
      assert.equal(baseline.valid, true, `${haft.id}+${head.id}: ${baseline.errors.join(" | ")}`);
      for (const control of controls) {
        const changed = structuredClone(base),
          current = control.target === "shaft" ? changed.shaft[control.key] : changed.components.find((part) => part.id === control.componentId)?.[control.key],
          value = control.min === current ? control.max : control.min;
        setControlValue(changed, control, value);
        const result = validateWeapon(changed, controls);
        assert.equal(result.valid, true, `${haft.id}+${head.id} ${control.label}: ${result.errors.join(" | ")}`);
        assert.notDeepEqual(result.mesh.positions, baseline.mesh.positions, `${haft.id}+${head.id} ${control.label} must change geometry`);
      }
    }
});

test("hammer neck ratio has no clamped dead zone", () => {
  const preset = copyPreset(PRESETS.find((candidate) => candidate.id === "lucerne-hammer")),
    control = preset.controls.find((candidate) => candidate.label === "Poll neck length ratio");
  const values = [];
  for (let value = control.min; value <= control.max + 1e-9; value = Number((value + control.step).toFixed(10))) {
    const definition = structuredClone(preset.definition);
    setControlValue(definition, control, value);
    const result = validateWeapon(definition, preset.controls);
    assert.equal(result.valid, true, `neckRatio=${value}: ${result.errors.join(" | ")}`);
    values.push(result.mesh.positions.join(","));
  }
  assert.equal(new Set(values).size, values.length, "every exposed neck-ratio step must generate distinct geometry");
});

test("validator returns actionable generator and control errors", () => {
  const preset = copyPreset(PRESETS.find((candidate) => candidate.id === "flanged-mace"));
  preset.definition.components[4].flanges = 5.5;
  const result = validateWeapon(preset.definition, preset.controls);
  assert.equal(result.valid, false);
  assert.ok(result.errors.some((error) => error.includes("flanges") && error.includes("integer")));
});
