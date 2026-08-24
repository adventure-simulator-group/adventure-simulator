import { maximumAuthoredGripRadius, MAX_ROUND_GRIP_RADIUS_M, MAX_SWORD_GRIP_THICKNESS_M, MAX_SWORD_GRIP_WIDTH_M } from "./anatomy.js";

const deepCopy = (value) => JSON.parse(JSON.stringify(value));
const OCTAGONAL_SHAFTS = new Set(["halberd-1540", "lucerne-hammer", "pollaxe", "hooked-bill"]);
const shaft = (length, radius = 0.021, segments = 16) => ({
  length,
  radius,
  topScale: 0.92,
  bottomScale: 0.9,
  segments,
  material: "wood",
});
const mounted = (kind, label, offset, values = {}) => ({
  kind,
  label,
  mount: "shaft-top",
  offset,
  ...values,
});
const socket = (length = 0.2, radius = 0.034) =>
  mounted("socket", "head socket", [0, -length * 0.62, 0], {
    profile: [
      [0, radius],
      [length, radius * 0.9],
    ],
    fitShaft: true,
    wall: 0.003,
    material: "darkSteel",
  });
const langets = (length = 0.42, spacing = 0.023) => [
  mounted("box", "left langet", [-spacing, -length * 0.72, 0], {
    size: [0.013, length, 0.008],
    fitShaftSide: true,
    material: "darkSteel",
  }),
  mounted("box", "right langet", [spacing, -length * 0.72, 0], {
    size: [0.013, length, 0.008],
    fitShaftSide: true,
    material: "darkSteel",
  }),
];
const poleControls = (extra = [], range = {}) => [
  {
    label: "Shaft length",
    path: "shaft.length",
    min: range.min ?? 0.45,
    max: range.max ?? 5.5,
    step: 0.01,
    unit: "m",
  },
  {
    label: "Shaft thickness",
    path: "shaft.radius",
    min: 0.016,
    max: maximumAuthoredGripRadius({
      bottomScale: 0.9,
      topScale: 0.92,
      radius: 0,
    }),
    step: 0.001,
    unit: "m",
  },
  ...extra,
];
const polearm = (id, name, family, description, shaftValue, components, controls = []) => ({
  id,
  name,
  family,
  description,
  definition: {
    shaft: shaft(shaftValue[0], shaftValue[1], OCTAGONAL_SHAFTS.has(id) ? 8 : 16),
    components: [
      ...components,
      {
        kind: "pommel",
        label: "butt cap",
        attach: { to: "weapon.root", at: "top", overlap: 0.005 },
        profile: [
          [0, shaftValue[1] ?? 0.021],
          [0.04, (shaftValue[1] ?? 0.021) * 0.92],
        ],
        material: "darkSteel",
      },
    ],
  },
  controls: poleControls(controls, shaftValue[2]),
});
const pommel = (profile, material = "steel") => ({
  kind: "pommel",
  label: "pommel",
  offset: [0, 0, 0],
  profile,
  segments: 12,
  material,
});
const ovalGrip = (y, length, width = 0.032, thickness = 0.023) => ({
  kind: "ovalGrip",
  label: "grip",
  offset: [0, y, 0],
  length,
  width,
  thickness,
  bottomScale: 0.85,
  topScale: 1,
  segments: 16,
  material: "leather",
});
const cross = (y, width, sweep = 0, height = 0.035) => ({
  kind: "guard",
  label: "crossguard",
  offset: [0, y, 0],
  width,
  height,
  thickness: 0.024,
  sweep,
});
const blade = (y, length, width, thickness, values = {}) => ({
  kind: "blade",
  label: "blade",
  offset: [0, y, 0],
  length,
  width,
  thickness,
  curvature: 0,
  taper: 0.85,
  singleEdge: 0,
  belly: 0,
  ...values,
});
const sectionBlade = (y, length, width, thickness, section = "diamond", values = {}) => ({
  kind: "sectionBlade",
  label: `${section}-section blade`,
  offset: [0, y, 0],
  length,
  width,
  thickness,
  section,
  taper: 0.82,
  ...values,
});
const sword = ({ id, name, family, description, pommel: p, grip: g, guards, blade: b, controls = true, guardControl = {}, extraControls = [] }) => {
  p.id = "pommel";
  p.attach = { to: "weapon.root", at: "base" };
  const pommelTop = (p.offset?.[1] ?? 0) + (p.profile ? Math.max(...p.profile.map((point) => point[0])) : (p.height ?? 0));
  g.id = "grip";
  g.attach = {
    to: "pommel.top",
    at: "base",
    overlap: Math.max(0, pommelTop - (g.offset?.[1] ?? 0)),
  };
  const gripTop = (g.offset?.[1] ?? 0) + g.length,
    primary = guards[0];
  primary.id = "guard";
  const primaryLocalCenter = primary.profile ? (Math.min(...primary.profile.map((point) => point[0])) + Math.max(...primary.profile.map((point) => point[0]))) / 2 : 0,
    primaryOriginalY = primary.offset?.[1] ?? 0;
  const primaryCenter = primaryOriginalY + primaryLocalCenter;
  if (primary.kind === "knuckleBow") {
    primary.stretchBetween = ["grip.base", "grip.top"];
    primary.offset = [primary.offset?.[0] ?? 0, 0, primary.offset?.[2] ?? 0];
  } else
    primary.attach = {
      to: "grip.top",
      at: "center",
      offset: [0, primaryCenter - gripTop, primary.offset?.[2] ?? 0],
    };
  guards.slice(1).forEach((guard, index) => {
    guard.id = `guard-${index + 2}`;
    if (guard.kind === "knuckleBow") {
      guard.stretchBetween = ["grip.base", "guard.center"];
      guard.offset = [guard.offset?.[0] ?? 0, 0, guard.offset?.[2] ?? 0];
    } else {
      const rangeCenter = guard.profile ? (Math.min(...guard.profile.map((point) => point[0])) + Math.max(...guard.profile.map((point) => point[0]))) / 2 : 0;
      const target = guard.kind === "guard" ? "blade.base" : "guard.center";
      guard.attach = {
        to: target,
        at: guard.kind === "pommel" || guard.kind === "tube" ? "base" : "center",
        offset: [guard.offset?.[0] ?? 0, (guard.offset?.[1] ?? 0) + rangeCenter - primaryCenter, guard.offset?.[2] ?? 0],
      };
    }
  });
  const bladeFrame = primary.kind === "knuckleBow" ? "guard.top" : "guard.center",
    bladeFrameWorld = primary.kind === "knuckleBow" ? primaryOriginalY + primary.length : primaryCenter;
  b.id = "blade";
  b.attach = {
    to: bladeFrame,
    at: "base",
    offset: [b.offset?.[0] ?? 0, (b.offset?.[1] ?? 0) - bladeFrameWorld, b.offset?.[2] ?? 0],
  };
  for (const component of [p, g, ...guards, b]) if (component.attach) delete component.offset;
  const components = [p, g, primary, b, ...guards.slice(1)],
    bi = 3;
  const guardPaths = guards.map((guard, index) => (guard.width === undefined || guard.controlWidth === false ? null : `components.${index === 0 ? 2 : index + 3}.width`)).filter(Boolean);
  return {
    id,
    name,
    family,
    description,
    definition: { gripClearance: 0.05, components },
    controls: [
      {
        label: "Grip clearance",
        path: "gripClearance",
        min: 0.02,
        max: 0.08,
        step: 0.005,
        unit: "m",
      },
      ...(controls
        ? [
          {
            label: "Blade length",
            path: `components.${bi}.length`,
            min: 0.28,
            max: 1.55,
            step: 0.01,
            unit: "m",
          },
          {
            label: "Blade width",
            path: `components.${bi}.width`,
            min: 0.018,
            max: 0.105,
            step: 0.001,
            unit: "m",
          },
          ...(b.kind === "blade"
            ? [
                {
                  label: "Blade curvature",
                  path: `components.${bi}.curvature`,
                  min: -0.32,
                  max: 0.32,
                  step: 0.005,
                  unit: "m",
                },
                {
                  label: "Blade belly",
                  path: `components.${bi}.belly`,
                  min: -0.2,
                  max: 0.65,
                  step: 0.01,
                  unit: "",
                },
              ]
            : [
                {
                  label: "Section depth",
                  path: `components.${bi}.thickness`,
                  min: 0.006,
                  max: 0.022,
                  step: 0.001,
                  unit: "m",
                },
              ]),
          {
            label: "Blade taper",
            path: `components.${bi}.taper`,
            min: 0.3,
            max: 2.4,
            step: 0.01,
            unit: "",
          },
          {
            label: "Grip length",
            path: "components.1.length",
            min: 0.09,
            max: 0.48,
            step: 0.005,
            unit: "m",
          },
          ...(g.kind === "ovalGrip"
            ? [
                {
                  label: "Grip width",
                  path: "components.1.width",
                  min: 0.029,
                  max: MAX_SWORD_GRIP_WIDTH_M,
                  step: 0.001,
                  unit: "m",
                },
                {
                  label: "Grip thickness",
                  path: "components.1.thickness",
                  min: 0.018,
                  max: MAX_SWORD_GRIP_THICKNESS_M,
                  step: 0.001,
                  unit: "m",
                },
              ]
            : []),
          ...(guardPaths.length
            ? [
                {
                  label: guardControl.label ?? "Cross span",
                  path: guardPaths[0],
                  paths: guardPaths,
                  min: guardControl.min ?? 0.12,
                  max: guardControl.max ?? 0.5,
                  step: 0.005,
                  unit: "m",
                },
              ]
            : []),
            ...extraControls,
          ]
        : []),
    ],
  };
};

const flangedMacePreset = (id, name, family, description, values) => ({
  id,
  name,
  family,
  description,
  definition: {
    shaft: {
      length: values.haftLength,
      radius: values.haftRadius,
      topScale: 0.94,
      bottomScale: 1,
      segments: 16,
      material: "darkSteel",
    },
    components: [
      {
        kind: "grip",
        label: "dark cylindrical grip",
        attach: { to: "weapon.root", at: "base" },
        length: values.gripLength,
        radius: values.gripRadius,
        topScale: 0.98,
        wraps: 0,
        material: values.gripMaterial ?? "leather",
      },
      {
        kind: "collar",
        label: "lower grip collar",
        attach: { to: "weapon.root", at: "center", offset: [0, 0.012, 0] },
        width: values.collarWidth,
        radius: values.collarRadius,
        material: "steel",
      },
      {
        kind: "collar",
        label: "upper grip collar",
        mount: "component-end",
        anchor: "dark cylindrical grip",
        offset: [0, 0, 0],
        width: values.collarWidth,
        radius: values.collarRadius,
        material: "steel",
      },
      {
        kind: "sleeve",
        label: "narrow head sleeve",
        mount: "shaft-top-sleeve",
        offset: [0, 0, 0],
        insertion: 0.012,
        length: values.sleeveLength,
        radius: Math.round(values.rootRadius * 1.18 * 2000) / 2000,
        topRadius: values.rootRadius,
        fitShaft: true,
        wall: 0.003,
        material: "darkSteel",
      },
      {
        kind: "mace",
        label: "longitudinal flanged head",
        mount: "shaft-top-centered",
        offset: [0, 0, 0],
        insertion: 0.012,
        length: values.headLength,
        rootRadius: values.rootRadius,
        shoulderRadius: values.shoulderRadius,
        cuspRadius: values.cuspRadius,
        cuspHeight: values.cuspHeight,
        concavity: values.concavity,
        crownLength: values.crownLength,
        flanges: values.flanges,
        flangeThickness: values.flangeThickness,
        material: "steel",
      },
    ],
  },
  controls: [
    {
      label: "Flange count",
      path: "components.4.flanges",
      min: 4,
      max: 10,
      step: 1,
      unit: "",
    },
    {
      label: "Head length",
      path: "components.4.length",
      min: 0.11,
      max: 0.28,
      step: 0.005,
      unit: "m",
    },
    {
      label: "Core / root radius",
      path: "components.4.rootRadius",
      min: 0.006,
      max: 0.016,
      step: 0.001,
      unit: "m",
    },
    {
      label: "Flange thickness",
      path: "components.4.flangeThickness",
      min: 0.0015,
      max: 0.006,
      step: 0.0005,
      unit: "m",
    },
    {
      label: "Maximum cusp radius",
      path: "components.4.cuspRadius",
      min: 0.035,
      max: 0.075,
      step: 0.001,
      unit: "m",
    },
    {
      label: "Cusp height",
      path: "components.4.cuspHeight",
      min: 0.35,
      max: 0.82,
      step: 0.01,
      unit: "",
    },
    {
      label: "Shoulder radius",
      path: "components.4.shoulderRadius",
      min: 0.005,
      max: 0.016,
      step: 0.0005,
      unit: "m",
    },
    {
      label: "Side concavity",
      path: "components.4.concavity",
      min: 0,
      max: 1,
      step: 0.01,
      unit: "",
    },
    {
      label: "Crown tip length",
      path: "components.4.crownLength",
      min: 0,
      max: 0.055,
      step: 0.001,
      unit: "m",
    },
    {
      label: "Head sleeve length",
      path: "components.3.length",
      min: 0.07,
      max: 0.17,
      step: 0.005,
      unit: "m",
    },
    {
      label: "Sleeve wall thickness",
      path: "components.3.wall",
      min: 0.002,
      max: 0.006,
      step: 0.0005,
      unit: "m",
    },
    {
      label: "Metal haft length",
      path: "shaft.length",
      min: 0.48,
      max: 0.9,
      step: 0.01,
      unit: "m",
    },
    {
      label: "Metal haft radius",
      path: "shaft.radius",
      min: 0.009,
      max: 0.017,
      step: 0.001,
      unit: "m",
    },
    {
      label: "Grip length",
      path: "components.0.length",
      min: 0.12,
      max: 0.28,
      step: 0.01,
      unit: "m",
    },
    {
      label: "Grip radius",
      path: "components.0.radius",
      min: 0.014,
      max: MAX_ROUND_GRIP_RADIUS_M,
      step: 0.001,
      unit: "m",
    },
    {
      label: "Collar width",
      path: "components.1.width",
      paths: ["components.1.width", "components.2.width"],
      min: 0.008,
      max: 0.028,
      step: 0.002,
      unit: "m",
    },
    {
      label: "Collar radius",
      path: "components.1.radius",
      paths: ["components.1.radius", "components.2.radius"],
      min: 0.017,
      max: 0.029,
      step: 0.001,
      unit: "m",
    },
  ],
});

export const PRESETS = [
  polearm(
    "halberd-1540",
    "German halberd",
    "Polearm · c. 1530–1550",
    "Compact forged head scaled to the Met's c.1525–50 German example (about 24.1 cm across), with narrow axe, downturned beak and central spike.",
    [1.82, 0.022],
    [
      socket(0.24, 0.033),
      mounted("axe", "narrow axe blade", [0, 0.015, 0], {
        width: 0.155,
        height: 0.27,
        thickness: 0.022,
        beard: 0.42,
        curvature: 0.08,
        side: 1,
      }),
      mounted("beak", "compact downturned beak", [-0.015, 0.075, 0], {
        length: 0.1,
        radius: 0.022,
        thickness: 0.018,
        direction: -1,
        curvature: -0.04,
      }),
      mounted("spear", "top spike", [0, -0.015, 0], {
        length: 0.32,
        width: 0.06,
        thickness: 0.022,
        shoulder: 0.14,
      }),
      ...langets(0.38),
    ],
    [
      {
        label: "Axe reach",
        path: "components.1.width",
        min: 0.13,
        max: 0.18,
        step: 0.005,
        unit: "m",
      },
      {
        label: "Axe beard",
        path: "components.1.beard",
        min: 0.2,
        max: 0.62,
        step: 0.01,
        unit: "",
      },
      {
        label: "Edge curvature",
        path: "components.1.curvature",
        min: 0,
        max: 0.16,
        step: 0.01,
        unit: "",
      },
      {
        label: "Beak droop",
        path: "components.2.curvature",
        min: -0.07,
        max: -0.01,
        step: 0.005,
        unit: "m",
      },
      {
        label: "Top spike length",
        path: "components.3.length",
        min: 0.24,
        max: 0.4,
        step: 0.01,
        unit: "m",
      },
    ],
  ),
  polearm(
    "lucerne-hammer",
    "Lucerne hammer",
    "Polearm · early 16th century",
    "Compact poll and narrow armour beak flow from a reinforced central socket into the top spike.",
    [1.74, 0.022],
    [
      socket(0.24, 0.033),
      mounted("hammer", "compact hammer poll", [0, 0.045, 0], {
        length: 0.09,
        face: 0.075,
        neck: 0.046,
        thickness: 0.07,
        direction: 1,
        crown: 0.06,
      }),
      mounted("beak", "narrow rear beak", [-0.008, 0.045, 0], {
        length: 0.14,
        radius: 0.024,
        thickness: 0.018,
        direction: -1,
        curvature: 0.025,
      }),
      mounted("spear", "top spike", [0, -0.01, 0], {
        length: 0.31,
        width: 0.052,
        thickness: 0.022,
        shoulder: 0.12,
      }),
      ...langets(0.42),
    ],
    [
      {
        label: "Rear beak length",
        path: "components.2.length",
        min: 0.11,
        max: 0.18,
        step: 0.005,
        unit: "m",
      },
      {
        label: "Rear beak curve",
        path: "components.2.curvature",
        min: -0.02,
        max: 0.06,
        step: 0.005,
        unit: "m",
      },
      {
        label: "Hammer face",
        path: "components.1.face",
        min: 0.055,
        max: 0.095,
        step: 0.005,
        unit: "m",
      },
    ],
  ),
  polearm(
    "pollaxe",
    "Knightly pollaxe",
    "Polearm · c. 1500–1540",
    "Compact armoured-fighting head with a waisted axe and shaped poll joined through a reinforced eye.",
    [1.48, 0.022],
    [
      socket(0.25, 0.032),
      mounted("axe", "waisted narrow axe", [0, 0.01, 0], {
        width: 0.115,
        height: 0.18,
        thickness: 0.023,
        beard: 0.08,
        curvature: 0.025,
        side: 1,
      }),
      mounted("hammer", "compact hammer poll", [0, 0.04, 0], {
        length: 0.09,
        face: 0.065,
        neck: 0.04,
        thickness: 0.068,
        direction: -1,
        crown: 0.08,
      }),
      mounted("spear", "top spike", [0, -0.01, 0], {
        length: 0.25,
        width: 0.048,
        thickness: 0.022,
        shoulder: 0.11,
      }),
      ...langets(0.5),
    ],
    [
      {
        label: "Axe reach",
        path: "components.1.width",
        min: 0.095,
        max: 0.145,
        step: 0.005,
        unit: "m",
      },
      {
        label: "Hammer face",
        path: "components.2.face",
        min: 0.05,
        max: 0.08,
        step: 0.005,
        unit: "m",
      },
    ],
  ),
  polearm(
    "kriegsspiess",
    "Kriegsspieß / pike",
    "Infantry spear · c. 1520–1550",
    "Long infantry pike with a compact diamond-section head and small reinforcing socket.",
    [3.35, 0.019, { min: 3, max: 5.5 }],
    [
      socket(0.18, 0.027),
      mounted("spear", "pike head", [0, 0, 0], {
        length: 0.25,
        width: 0.043,
        thickness: 0.022,
        shoulder: 0.15,
      }),
    ],
    [
      {
        label: "Head length",
        path: "components.1.length",
        min: 0.14,
        max: 0.42,
        step: 0.01,
        unit: "m",
      },
    ],
  ),
  polearm(
    "short-spear",
    "Short spear",
    "Spear · 16th century",
    "General-purpose stout spear with a leaf-shaped blade and iron shoe.",
    [1.72, 0.02],
    [
      socket(0.2, 0.031),
      mounted("spear", "leaf head", [0, 0, 0], {
        length: 0.31,
        width: 0.085,
        thickness: 0.024,
        shoulder: 0.32,
      }),
    ],
  ),
  polearm(
    "partisan",
    "Partisan",
    "Guard polearm · c. 1530–1550",
    "Broad spear blade forged continuously into short symmetrical basal lugs.",
    [1.78, 0.021],
    [
      socket(0.22),
      mounted("partisan", "blade and short root lugs", [0, 0, 0], {
        length: 0.42,
        width: 0.135,
        lugWidth: 0.145,
        lugDrop: 0.075,
        thickness: 0.022,
      }),
      ...langets(0.32),
    ],
    [
      {
        label: "Blade width",
        path: "components.1.width",
        min: 0.09,
        max: 0.18,
        step: 0.005,
        unit: "m",
      },
      {
        label: "Total lug span",
        path: "components.1.lugWidth",
        min: 0.1,
        max: 0.18,
        step: 0.005,
        unit: "m",
      },
    ],
  ),
  polearm(
    "glaive",
    "German Kuse / glaive",
    "Polearm · early 16th century",
    "Curved single-edged blade with an integral narrow root seated inside the socket.",
    [1.72, 0.021],
    [
      socket(0.25),
      mounted("glaive", "integral glaive blade", [0, 0, 0], {
        length: 0.54,
        width: 0.105,
        thickness: 0.018,
        curvature: 0.13,
        root: 0.032,
      }),
      ...langets(0.4),
    ],
    [
      {
        label: "Blade curvature",
        path: "components.1.curvature",
        min: 0.02,
        max: 0.22,
        step: 0.005,
        unit: "m",
      },
      {
        label: "Blade width",
        path: "components.1.width",
        min: 0.07,
        max: 0.15,
        step: 0.005,
        unit: "m",
      },
    ],
  ),
  polearm(
    "hooked-bill",
    "Hooked bill",
    "Polearm · 16th century",
    "Single forged bill outline with an exposed 8 cm recurved hook, top point and integral socket root.",
    [1.83, 0.022],
    [
      socket(0.23, 0.033),
      mounted("bill", "continuous bill and hook", [0, 0, 0], {
        length: 0.38,
        width: 0.09,
        hook: 0.08,
        thickness: 0.02,
        root: 0.03,
      }),
      ...langets(0.38),
    ],
    [
      {
        label: "Hook projection",
        path: "components.1.hook",
        min: 0.06,
        max: 0.09,
        step: 0.005,
        unit: "m",
      },
      {
        label: "Bill body width",
        path: "components.1.width",
        min: 0.07,
        max: 0.11,
        step: 0.005,
        unit: "m",
      },
    ],
  ),
  polearm(
    "military-fork",
    "Military fork",
    "Polearm · 16th century",
    "A single forged head flowing from socket root through crotch into tapered tines.",
    [1.86, 0.021],
    [
      socket(0.24),
      mounted("fork", "forged fork head", [0, 0, 0], {
        length: 0.39,
        width: 0.13,
        baseWidth: 0.055,
        tineWidth: 0.026,
        crotch: 0.34,
        thickness: 0.022,
      }),
      ...langets(0.34),
    ],
    [
      {
        label: "Fork span",
        path: "components.1.width",
        min: 0.09,
        max: 0.19,
        step: 0.005,
        unit: "m",
      },
      {
        label: "Tine length",
        path: "components.1.length",
        min: 0.28,
        max: 0.52,
        step: 0.01,
        unit: "m",
      },
    ],
  ),

  sword({
    id: "landsknecht-longsword",
    name: "Longsword",
    family: "Sword · c. 1520–1550",
    description: "Long fullered blade, straight cross, two-handed grip and scent-stopper pommel.",
    pommel: pommel([
      [0, 0.012],
      [0.01, 0.017],
      [0.038, 0.02],
      [0.055, 0.01],
    ]),
    grip: ovalGrip(0.12, 0.3, 0.033, 0.024),
    guards: [cross(0.425, 0.31, 0.018)],
    blade: sectionBlade(0.425, 1.02, 0.065, 0.012, "fullered"),
  }),
  sword({
    id: "zweihander",
    name: "Early Zweihänder",
    family: "Two-handed sword · c. 1530–1550",
    description: "Large fullered infantry sword with long grip, broad guard and parrying lugs.",
    pommel: pommel([
      [0, 0.014],
      [0.012, 0.021],
      [0.045, 0.024],
      [0.065, 0.012],
    ]),
    grip: ovalGrip(0.135, 0.415, MAX_SWORD_GRIP_WIDTH_M, MAX_SWORD_GRIP_THICKNESS_M),
    guards: [cross(0.555, 0.48, 0.035, 0.045), { ...cross(0.77, 0.18, 0, 0.025), controlWidth: false }],
    blade: sectionBlade(0.555, 1.28, 0.072, 0.013, "fullered", { taper: 0.68 }),
  }),
  sword({
    id: "katzbalger",
    name: "Katzbalger",
    family: "Landsknecht sidearm · c. 1500–1550",
    description: "Short broad fullered blade with a compact c.14 cm figure-eight guard and joined fan cap.",
    pommel: {
      kind: "fanPommel",
      label: "compact fan cap",
      offset: [0, 0, 0],
      width: 0.055,
      height: 0.045,
      thickness: 0.019,
    },
    grip: ovalGrip(0.04, 0.165, 0.034, 0.025),
    guards: [
      {
        kind: "figureEight",
        label: "compact figure-eight guard",
        offset: [0, 0.205, 0],
        width: 0.14,
        height: 0.038,
        bar: 0.0065,
      },
    ],
    blade: sectionBlade(0.205, 0.66, 0.07, 0.011, "fullered", { taper: 0.5 }),
    guardControl: { label: "Figure-eight span", min: 0.12, max: 0.17 },
  }),
  sword({
    id: "grosse-messer",
    name: "Großes Messer",
    family: "Single-edged sword · c. 1500–1540",
    description: "Broad curved blade, slab-and-tang grip, forged cross and visibly projecting Nagel.",
    pommel: pommel(
      [
        [0, 0.016],
        [0.018, 0.022],
        [0.038, 0.014],
      ],
      "brass",
    ),
    grip: {
      kind: "slabGrip",
      label: "slab grip",
      offset: [0, 0.05, 0],
      length: 0.215,
      width: 0.038,
      thickness: 0.01,
      scaleThickness: 0.009,
      material: "wood",
    },
    guards: [
      cross(0.265, 0.23, 0.01),
      {
        kind: "hammer",
        label: "projecting Nagel",
        offset: [0, 0.265, 0],
        length: 0.07,
        face: 0.028,
        neck: 0.014,
        thickness: 0.026,
        direction: 1,
        rotation: [0, 90, 0],
      },
      {
        kind: "pommel",
        label: "Nagel terminal",
        offset: [0, 0.265, -0.072],
        profile: [
          [0, 0.018],
          [0.009, 0.018],
        ],
        material: "steel",
        rotation: [90, 0, 0],
      },
    ],
    blade: blade(0.265, 0.84, 0.064, 0.011, {
      curvature: 0.085,
      taper: 0.78,
      singleEdge: 0.72,
      belly: 0.16,
    }),
  }),
  sword({
    id: "dussack",
    name: "Dussack form study",
    family: "Form study · reference pending",
    description: "Non-curated parametric curved-blade and hand-bow study; retain for generator testing, not historical baseline scoring.",
    pommel: pommel(
      [
        [0, 0.013],
        [0.015, 0.018],
        [0.032, 0.011],
      ],
      "brass",
    ),
    grip: ovalGrip(0.045, 0.155, 0.032, 0.023),
    guards: [
      {
        kind: "knuckleBow",
        label: "enclosed hand bow",
        offset: [0, 0.045, 0],
        width: 0.1,
        length: 0.155,
        bar: 0.01,
        thickness: 0.01,
        side: 1,
      },
    ],
    blade: blade(0.2, 0.69, 0.068, 0.012, {
      curvature: 0.13,
      taper: 0.82,
      singleEdge: 0.8,
      belly: 0.32,
    }),
    guardControl: { label: "Hand-bow reach", min: 0.075, max: 0.125 },
  }),
  sword({
    id: "estoc",
    name: "Panzerstecher / estoc",
    family: "Armour-piercing sword · c. 1500–1550",
    description: "Long narrow stiff thrusting blade with a raised diamond section.",
    pommel: pommel([
      [0, 0.012],
      [0.01, 0.017],
      [0.036, 0.019],
      [0.052, 0.009],
    ]),
    grip: ovalGrip(0.11, 0.28, 0.031, 0.023),
    guards: [cross(0.39, 0.3, 0.005)],
    blade: sectionBlade(0.39, 1.05, 0.034, 0.018, "diamond", { taper: 0.82 }),
  }),
  sword({
    id: "rondel-dagger",
    name: "Rondel dagger",
    family: "Dagger · late 15th–early 16th century",
    description: "Stiff diamond-section thrusting dagger with paired correctly wound rondels.",
    pommel: pommel([
      [0, 0.022],
      [0.008, 0.032],
      [0.016, 0.022],
    ]),
    grip: ovalGrip(0.026, 0.125, 0.03, 0.022),
    guards: [
      {
        kind: "pommel",
        label: "upper rondel",
        offset: [0, 0.148, 0],
        profile: [
          [0, 0.032],
          [0.008, 0.032],
        ],
        material: "steel",
      },
    ],
    blade: sectionBlade(0.155, 0.38, 0.032, 0.016, "diamond", { taper: 0.92 }),
    controls: false,
  }),
  sword({
    id: "reitschwert-1540",
    name: "German Reitschwert",
    family: "Riding sword · c. 1540s",
    description: "Connected mid-century riding-sword hilt: forged cross block, attached side-ring arc, paired finger loops and knuckle bow in distinct planes.",
    pommel: pommel([
      [0, 0.012],
      [0.01, 0.017],
      [0.032, 0.019],
      [0.045, 0.009],
    ]),
    grip: ovalGrip(0.085, 0.17, 0.031, 0.022),
    guards: [
      { ...cross(0.255, 0.23, 0.01), controlWidth: false },
      {
        kind: "ringGuard",
        label: "attached side-ring arc",
        offset: [0, 0.255, 0.012],
        radius: 0.055,
        bar: 0.006,
        arcStart: 0,
        arcEnd: 3.14159,
      },
      {
        kind: "ringGuard",
        label: "fore finger loop",
        offset: [0.05, 0.25, 0],
        radius: 0.03,
        bar: 0.0055,
        rotation: [0, 90, 0],
        arcStart: -1.5708,
        arcEnd: 1.5708,
      },
      {
        kind: "ringGuard",
        label: "rear finger loop",
        offset: [-0.05, 0.25, 0],
        radius: 0.03,
        bar: 0.0055,
        rotation: [0, 90, 0],
        arcStart: -1.5708,
        arcEnd: 1.5708,
      },
      {
        kind: "knuckleBow",
        label: "knuckle bow",
        offset: [0, 0.085, 0],
        width: 0.075,
        length: 0.17,
        bar: 0.0065,
        side: 1,
      },
      {
        kind: "box",
        label: "right branch junction",
        offset: [0.052, 0.255, 0],
        size: [0.018, 0.022, 0.026],
        material: "darkSteel",
      },
      {
        kind: "box",
        label: "left branch junction",
        offset: [-0.052, 0.255, 0],
        size: [0.018, 0.022, 0.026],
        material: "darkSteel",
      },
    ],
    blade: sectionBlade(0.255, 0.88, 0.046, 0.01, "fullered"),
    guardControl: { label: "Knuckle-bow reach", min: 0.06, max: 0.095 },
    extraControls: [
      {
        label: "Side-ring radius",
        path: "components.3.radius",
        min: 0.045,
        max: 0.065,
        step: 0.002,
        unit: "m",
      },
      {
        label: "Finger-loop radius",
        path: "components.4.radius",
        paths: ["components.4.radius", "components.5.radius"],
        min: 0.025,
        max: 0.036,
        step: 0.001,
        unit: "m",
      },
    ],
  }),

  polearm(
    "reiter-war-hammer",
    "Reiter war hammer",
    "Horseman's weapon · c. 1520–1550",
    "Reference-scale 14 cm head with compact crowned poll, near-square faceted beak and long steel haft sleeve.",
    [0.58, 0.018],
    [
      mounted("socket", "steel haft sheathing", [0, -0.32, 0], {
        profile: [
          [0, 0.022],
          [0.35, 0.02],
        ],
        material: "darkSteel",
      }),
      mounted("hammer", "compact crowned poll", [0, 0.015, 0], {
        length: 0.064,
        face: 0.05,
        neck: 0.03,
        thickness: 0.05,
        direction: 1,
        crown: 0.14,
      }),
      mounted("facetedBeak", "faceted square beak", [0, 0.015, 0], {
        length: 0.075,
        root: 0.038,
        tip: 0.008,
        thickness: 0.014,
        direction: -1,
        set: 0.005,
      }),
    ],
    [
      {
        label: "Beak length",
        path: "components.2.length",
        min: 0.065,
        max: 0.09,
        step: 0.005,
        unit: "m",
      },
      {
        label: "Beak set",
        path: "components.2.set",
        min: -0.005,
        max: 0.015,
        step: 0.005,
        unit: "m",
      },
    ],
  ),
  polearm(
    "hand-axe",
    "Primitive bearded-axe study",
    "Primitive study · not curated",
    "Generator stress-test for the shared axe primitive; excluded from the curated 1544 baseline.",
    [0.67, 0.021],
    [
      socket(0.1, 0.027),
      mounted("axe", "bearded axe head", [0, 0, 0], {
        width: 0.18,
        height: 0.18,
        thickness: 0.028,
        beard: 0.5,
        curvature: 0.1,
        side: 1,
      }),
    ],
    [
      {
        label: "Edge curvature",
        path: "components.1.curvature",
        min: -0.05,
        max: 0.2,
        step: 0.01,
        unit: "",
      },
    ],
  ),
  flangedMacePreset("flanged-mace", "Compact flanged mace", "Cavalry sidearm · early 16th century", "Compact endpoint of the shared sampled-flange system: gently curved sides, central cusps, steel haft and dark grip.", {
    haftLength: 0.58,
    haftRadius: 0.013,
    gripLength: 0.17,
    gripRadius: 0.02,
    collarWidth: 0.014,
    collarRadius: 0.023,
    headLength: 0.14,
    sleeveLength: 0.09,
    rootRadius: 0.008,
    shoulderRadius: 0.007,
    cuspRadius: 0.06,
    cuspHeight: 0.5,
    concavity: 0.15,
    crownLength: 0.008,
    flanges: 6,
    flangeThickness: 0.0025,
  }),
  flangedMacePreset("gothic-flanged-mace", "Elongated Gothic flanged mace", "Gothic mace endpoint · late 15th–early 16th century", "Reference-like endpoint of the same sampled generator: long inward-bowed flanges, high cusped shoulders, long sleeve, short pointed crown, steel haft, collars and near-black grip.", {
    haftLength: 0.78,
    haftRadius: 0.011,
    gripLength: 0.2,
    gripRadius: 0.019,
    gripMaterial: "darkLeather",
    collarWidth: 0.018,
    collarRadius: 0.022,
    headLength: 0.25,
    sleeveLength: 0.15,
    rootRadius: 0.009,
    shoulderRadius: 0.0065,
    cuspRadius: 0.06,
    cuspHeight: 0.75,
    concavity: 0.92,
    crownLength: 0.015,
    flanges: 6,
    flangeThickness: 0.002,
  }),
];

// Head-family controls are hydrated here so every generator parameter has an
// explicit declarative default and can participate in the same fuzz contract.
const presetById = (id) => PRESETS.find((preset) => preset.id === id);
const addControls = (id, controls) => presetById(id).controls.push(...controls);
const component = (id, index, defaults) => Object.assign(presetById(id).definition.components[index], defaults);
const c = (label, path, min, max, step, unit = "") => ({
  label,
  path,
  min,
  max,
  step,
  unit,
});

for (const [id, index] of [
  ["halberd-1540", 1],
  ["pollaxe", 1],
  ["hand-axe", 1],
]) {
  const head = presetById(id).definition.components[index];
  component(id, index, {
    rootWidth: Number(Math.min(head.width * 0.18, 0.055).toFixed(3)),
    upperShoulder: 0.38,
    lowerShoulder: 0.26,
    flare: 0,
    toe: 0,
    heel: 0,
    beardDrop: Number(Math.max(0.04, head.beard * 0.45).toFixed(2)),
  });
  addControls(id, [c("Axe height", `components.${index}.height`, id === "pollaxe" ? 0.14 : 0.16, id === "halberd-1540" ? 0.33 : 0.26, 0.01, "m"), c("Axe plate thickness", `components.${index}.thickness`, 0.014, 0.032, 0.001, "m"), c("Eye / root width", `components.${index}.rootWidth`, 0.018, 0.042, 0.001, "m"), c("Edge flare", `components.${index}.flare`, -0.08, 0.08, 0.01), c("Toe rise", `components.${index}.toe`, -0.04, 0.06, 0.01), c("Heel drop", `components.${index}.heel`, -0.02, 0.08, 0.01), c("Beard drop ratio", `components.${index}.beardDrop`, 0.04, 0.3, 0.01), c("Upper shoulder blend", `components.${index}.upperShoulder`, 0.35, 0.46, 0.01), c("Lower shoulder blend", `components.${index}.lowerShoulder`, 0.18, 0.32, 0.01), c("Axe side", `components.${index}.side`, -1, 1, 2)]);
}

for (const [id, index] of [
  ["halberd-1540", 3],
  ["lucerne-hammer", 3],
  ["pollaxe", 3],
  ["kriegsspiess", 1],
  ["short-spear", 1],
]) {
  const head = presetById(id).definition.components[index];
  const minimumWidth = Math.max(0.026, head.width - 0.02);
  component(id, index, {
    rootWidth: Number((head.width * 0.4).toFixed(3)),
    bellyPosition: head.shoulder,
    acuteness: 1,
  });
  addControls(id, [c("Spear maximum width", `components.${index}.width`, minimumWidth, head.width + 0.035, 0.001, "m"), c("Spear root width", `components.${index}.rootWidth`, 0.01, Math.floor(minimumWidth * 750) / 1000, 0.001, "m"), c("Spear belly position", `components.${index}.bellyPosition`, 0.08, 0.46, 0.01), c("Point acuteness", `components.${index}.acuteness`, 0.55, 2.05, 0.05), c("Spear section depth", `components.${index}.thickness`, 0.012, 0.03, 0.001, "m")]);
}

for (const [id, index] of [
  ["lucerne-hammer", 1],
  ["pollaxe", 2],
  ["reiter-war-hammer", 1],
]) {
  const preset = presetById(id),
    head = preset.definition.components[index];
  preset.controls = preset.controls.filter((control) => control.label !== "Hammer face");
  component(id, index, {
    neckRatio: 0.72,
    faceFlare: 0,
    crownLength: Number((head.length * head.crown).toFixed(3)),
    faceThickness: head.thickness,
  });
  addControls(id, [c("Poll length", `components.${index}.length`, 0.05, 0.12, 0.001, "m"), c("Poll neck length ratio", `components.${index}.neckRatio`, 0.35, 0.85, 0.01), c("Poll neck height", `components.${index}.neck`, 0.025, 0.06, 0.001, "m"), c("Poll face height", `components.${index}.face`, 0.04, 0.1, 0.001, "m"), c("Poll face depth", `components.${index}.faceThickness`, 0.035, 0.085, 0.001, "m"), c("Poll face flare", `components.${index}.faceFlare`, 0, 0.3, 0.02), c("Poll crown reach", `components.${index}.crownLength`, 0, 0.018, 0.001, "m")]);
}

for (const [id, index] of [
  ["halberd-1540", 2],
  ["lucerne-hammer", 2],
]) {
  const head = presetById(id).definition.components[index];
  component(id, index, {
    rootSection: Number((head.radius * 1.5).toFixed(3)),
    tipSection: 0.003,
    bendPosition: 0.55,
    droop: Number((head.curvature * 0.35).toFixed(3)),
  });
  addControls(id, [c("Beak root section", `components.${index}.rootSection`, 0.022, 0.045, 0.001, "m"), c("Beak tip section", `components.${index}.tipSection`, 0.002, 0.008, 0.001, "m"), c("Beak bend position", `components.${index}.bendPosition`, 0.3, 0.75, 0.05), c("Beak tip droop / set", `components.${index}.droop`, -0.03, 0.04, 0.001, "m"), c("Beak section depth", `components.${index}.thickness`, 0.012, 0.026, 0.001, "m")]);
}

component("reiter-war-hammer", 2, { bendPosition: 0.22, tipThickness: 0.014 });
addControls("reiter-war-hammer", [c("Beak root section", "components.2.root", 0.028, 0.046, 0.001, "m"), c("Beak tip section", "components.2.tip", 0.004, 0.012, 0.001, "m"), c("Beak bend position", "components.2.bendPosition", 0.15, 0.55, 0.01), c("Beak section depth", "components.2.tipThickness", 0.01, 0.022, 0.001, "m")]);

component("partisan", 1, {
  bellyPosition: 0.32,
  rootWidth: 0.024,
  lugSweep: 0.055,
  acuteness: 1,
});
addControls("partisan", [c("Blade length", "components.1.length", 0.32, 0.56, 0.01, "m"), c("Blade belly position", "components.1.bellyPosition", 0.22, 0.44, 0.01), c("Blade root width", "components.1.rootWidth", 0.016, 0.038, 0.001, "m"), c("Point acuteness", "components.1.acuteness", 0.6, 1.8, 0.05), c("Lug drop", "components.1.lugDrop", 0.04, 0.12, 0.005), c("Lug sweep", "components.1.lugSweep", 0.03, 0.09, 0.005), c("Blade section depth", "components.1.thickness", 0.014, 0.028, 0.001, "m")]);

component("glaive", 1, {
  edgeCurvature: 0.24,
  spineCurvature: 0.2,
  bellyPosition: 0.42,
  pointLength: 0.24,
  rootLength: 0.08,
});
addControls("glaive", [c("Blade length", "components.1.length", 0.42, 0.68, 0.01, "m"), c("Edge curvature", "components.1.edgeCurvature", 0.08, 0.38, 0.01), c("Spine curvature", "components.1.spineCurvature", 0, 0.4, 0.02), c("Belly position", "components.1.bellyPosition", 0.28, 0.62, 0.02), c("Point length ratio", "components.1.pointLength", 0.18, 0.34, 0.02), c("Tang width", "components.1.root", 0.022, 0.042, 0.001, "m"), c("Tang insertion length", "components.1.rootLength", 0.05, 0.12, 0.005, "m"), c("Blade section depth", "components.1.thickness", 0.012, 0.026, 0.001, "m")]);

component("hooked-bill", 1, {
  bellyPosition: 0.48,
  hookDepth: 0.19,
  hookCurvature: 0.22,
  pointLength: 0.24,
  rootLength: 0.06,
});
addControls("hooked-bill", [c("Bill length", "components.1.length", 0.3, 0.5, 0.01, "m"), c("Body belly position", "components.1.bellyPosition", 0.34, 0.62, 0.02), c("Point length ratio", "components.1.pointLength", 0.16, 0.34, 0.02), c("Root width", "components.1.root", 0.022, 0.042, 0.001, "m"), c("Root insertion length", "components.1.rootLength", 0.04, 0.1, 0.005, "m"), c("Hook depth", "components.1.hookDepth", 0.12, 0.25, 0.01), c("Hook curvature", "components.1.hookCurvature", 0.1, 0.34, 0.02), c("Bill section depth", "components.1.thickness", 0.014, 0.028, 0.001, "m")]);

component("military-fork", 1, {
  tineTaper: 0.55,
  shoulderBlend: 0.2,
  crotchRound: 0.05,
});
addControls("military-fork", [c("Tine width", "components.1.tineWidth", 0.018, 0.036, 0.001, "m"), c("Tine taper", "components.1.tineTaper", 0.35, 0.75, 0.05), c("Crotch depth", "components.1.crotch", 0.25, 0.48, 0.01), c("Crotch rounding", "components.1.crotchRound", 0.02, 0.1, 0.01), c("Root width", "components.1.baseWidth", 0.04, 0.075, 0.005, "m"), c("Shoulder blend", "components.1.shoulderBlend", 0.12, 0.3, 0.02), c("Fork section depth", "components.1.thickness", 0.014, 0.03, 0.001, "m")]);

for (const id of ["halberd-1540", "lucerne-hammer", "pollaxe", "kriegsspiess", "short-spear", "partisan", "glaive", "hooked-bill", "military-fork", "hand-axe"]) {
  const preset = presetById(id),
    socketIndex = preset.definition.components.findIndex((part) => part.label === "head socket");
  if (socketIndex >= 0) {
    preset.definition.components[socketIndex].profile[1][1] = Number(preset.definition.components[socketIndex].profile[1][1].toFixed(3));
    addControls(id, [c("Socket length", `components.${socketIndex}.profile.1.0`, id === "hand-axe" ? 0.08 : 0.14, 0.3, 0.01, "m"), c("Socket wall thickness", `components.${socketIndex}.wall`, 0.002, 0.006, 0.0005, "m")]);
  }
  const langetIndices = preset.definition.components.map((part, index) => (part.label?.includes("langet") ? index : -1)).filter((index) => index >= 0);
  if (langetIndices.length)
    addControls(id, [
      {
        ...c("Langet length", `components.${langetIndices[0]}.size.1`, 0.24, 0.56, 0.01, "m"),
        paths: langetIndices.map((index) => `components.${index}.size.1`),
      },
      {
        ...c("Langet width", `components.${langetIndices[0]}.size.0`, 0.008, 0.02, 0.001, "m"),
        paths: langetIndices.map((index) => `components.${index}.size.0`),
      },
    ]);
}

// Curated close-detail assemblies are normalized here so their component
// indices remain stable for the shared sword control contract.
const reitschwert = PRESETS.find((preset) => preset.id === "reitschwert-1540");
reitschwert.description = "Simplified c.1540 German riding-sword hilt with one side ring, one knuckle bow, a forged cross, and explicit endpoint bosses in separate depth planes.";
const reitschBlade = reitschwert.definition.components.find((component) => component.kind === "sectionBlade");
const reitschCross = reitschwert.definition.components.find((component) => component.label === "crossguard");
const reitschSideRing = {
  ...reitschwert.definition.components.find((component) => component.label === "attached side-ring arc"),
};
const reitschBow = {
  ...reitschwert.definition.components.find((component) => component.label === "knuckle bow"),
};
const boss = (label, x, y) => ({
  kind: "pommel",
  label,
  attach: {
    to: label.startsWith("lower") ? "grip.base" : "guard.center",
    at: "base",
    offset: [x, 0, label.includes("side-ring") ? 0.012 : 0],
  },
  profile: [
    [0, 0.012],
    [0.048, 0.012],
  ],
  material: "darkSteel",
  rotation: [90, 0, 0],
});
reitschwert.definition.components = [reitschwert.definition.components[0], reitschwert.definition.components[1], reitschCross, reitschSideRing, reitschBow, boss("left side-ring boss", -0.055, 0.255), boss("right side-ring boss", 0.055, 0.255), boss("upper knuckle boss", 0, 0.255), boss("lower knuckle boss", 0, 0.085), reitschBlade];
for (const control of reitschwert.controls)
  if ((control.label.startsWith("Blade") || control.label.startsWith("Section")) && control.path?.startsWith("components.3.")) {
    control.path = control.path.replace("components.3.", "components.9.");
    if (control.paths) control.paths = control.paths.map((path) => path.replace("components.3.", "components.9."));
  }
reitschwert.controls = reitschwert.controls.filter((control) => control.label !== "Finger-loop radius");
const reitschBowControl = reitschwert.controls.find((control) => control.label === "Knuckle-bow reach");
reitschBowControl.path = "components.4.width";
reitschBowControl.paths = ["components.4.width"];

const messer = PRESETS.find((preset) => preset.id === "grosse-messer");
const nagelIndex = messer.definition.components.findIndex((component) => component.label === "projecting Nagel");
messer.definition.components[nagelIndex] = {
  kind: "tube",
  id: "nagel-stem",
  label: "45 mm Nagel stem",
  attach: { to: "guard.center", at: "base" },
  points: [
    [0, 0],
    [0, 0.045],
  ],
  radius: 0.005,
  rotation: [90, 0, 0],
  material: "steel",
};
messer.definition.components[nagelIndex + 1] = {
  kind: "pommel",
  id: "nagel-button",
  label: "rounded 15 mm Nagel button",
  attach: { to: "nagel-stem.top", at: "base" },
  profile: [
    [0, 0.006],
    [0.005, 0.0075],
    [0.012, 0.006],
  ],
  material: "steel",
  rotation: [90, 0, 0],
};

PRESETS.find((preset) => preset.id === "katzbalger").definition.components[0].thickness = 0.014;
const reiterPoll = PRESETS.find((preset) => preset.id === "reiter-war-hammer").definition.components[1];
reiterPoll.crown = 0.06;
reiterPoll.neck = 0.026;
reiterPoll.face = 0.046;

export const HAFT_MODULES = [
  {
    id: "wooden-polearm",
    name: "Wooden polearm shaft",
    shaft: {
      length: 1.82,
      radius: 0.022,
      topScale: 0.94,
      bottomScale: 0.9,
      segments: 16,
      material: "wood",
    },
    components: [
      {
        kind: "pommel",
        id: "butt-cap",
        label: "butt cap",
        attach: { to: "weapon.root", at: "top", overlap: 0.005 },
        profile: [
          [0, 0.029],
          [0.04, 0.027],
        ],
        material: "darkSteel",
      },
    ],
  },
  {
    id: "steel-one-hand",
    name: "Steel one-hand haft",
    shaft: {
      length: 0.62,
      radius: 0.013,
      topScale: 0.94,
      bottomScale: 1,
      segments: 16,
      material: "darkSteel",
    },
    components: [
      {
        kind: "grip",
        id: "composer-grip",
        label: "composer grip",
        attach: { to: "weapon.root", at: "base" },
        length: 0.18,
        radius: 0.02,
        topScale: 0.98,
        wraps: 0,
        material: "darkLeather",
      },
      {
        kind: "collar",
        id: "composer-collar",
        label: "composer collar",
        attach: { to: "composer-grip.top", at: "center" },
        width: 0.016,
        radius: 0.023,
        material: "steel",
      },
    ],
  },
];

export const HEAD_ASSEMBLIES = [
  { id: "flanged-mace", name: "Flanged mace" },
  {
    id: "halberd",
    name: "Halberd",
    source: "halberd-1540",
    kinds: ["socket", "axe", "beak", "spear", "box"],
  },
  {
    id: "spear",
    name: "Spear",
    source: "short-spear",
    kinds: ["socket", "spear"],
  },
  {
    id: "hammer",
    name: "Hammer / pick",
    source: "lucerne-hammer",
    kinds: ["socket", "hammer", "beak"],
  },
  { id: "axe", name: "Axe", source: "hand-axe", kinds: ["socket", "axe"] },
  {
    id: "beak",
    name: "Armour beak",
    source: "lucerne-hammer",
    kinds: ["socket", "beak"],
  },
  {
    id: "fork",
    name: "Military fork",
    source: "military-fork",
    kinds: ["socket", "fork"],
  },
  {
    id: "bill",
    name: "Hooked bill",
    source: "hooked-bill",
    kinds: ["socket", "bill"],
  },
  {
    id: "glaive",
    name: "Kuse / glaive",
    source: "glaive",
    kinds: ["socket", "glaive"],
  },
  {
    id: "partisan",
    name: "Partisan",
    source: "partisan",
    kinds: ["socket", "partisan"],
  },
];

export function composeWeapon(haftId, headId) {
  const haft = deepCopy(HAFT_MODULES.find((module) => module.id === haftId) ?? HAFT_MODULES[0]),
    shaftRadius = haft.shaft.radius;
  let head;
  if (headId === "flanged-mace")
    head = [
      {
        kind: "sleeve",
        id: "head-sleeve",
        label: "derived head sleeve",
        mount: "shaft-top-sleeve",
        insertion: 0.012,
        length: 0.11,
        radius: shaftRadius * 1.12,
        topRadius: shaftRadius * 1.02,
        material: "darkSteel",
      },
      {
        kind: "mace",
        id: "head",
        label: "composed flanged head",
        mount: "shaft-top-centered",
        insertion: 0.012,
        length: 0.17,
        rootRadius: shaftRadius * 1.02,
        shoulderRadius: shaftRadius * 0.82,
        cuspRadius: Math.max(0.055, shaftRadius * 2.1),
        cuspHeight: 0.58,
        concavity: 0.55,
        crownLength: 0.012,
        flanges: 6,
        flangeThickness: 0.0025,
        material: "steel",
      },
    ];
  else {
    const assembly = HEAD_ASSEMBLIES.find((candidate) => candidate.id === headId) ?? HEAD_ASSEMBLIES[1];
    head = deepCopy(PRESETS.find((preset) => preset.id === assembly.source).definition.components.filter((component) => component.mount === "shaft-top" && assembly.kinds.includes(component.kind)));
    let primary = 0;
    head.forEach((component, index) => {
      component.id = component.kind === "socket" ? "head-socket" : component.kind === "box" ? `head-langet-${index}` : `head-primary-${primary++}`;
      if (component.kind === "socket") {
        component.fitShaft = true;
        component.wall = 0.003;
      }
      if (component.kind === "box" && component.label?.includes("langet")) {
        component.fitShaftSide = true;
        component.offset[0] = Math.sign(component.offset[0]) * (shaftRadius * (haft.shaft.topScale ?? 0.92) + component.size[0] / 2 - 0.002);
      }
    });
  }
  return { shaft: haft.shaft, components: [...haft.components, ...head] };
}

const componentControl = (label, componentId, key, min, max, step, unit = "") => ({ label, componentId, key, min, max, step, unit });
export function compositionControls(definition) {
  const primary = definition.components.find((part) => part.id?.startsWith("head-primary")) ?? definition.components.find((part) => part.id === "head");
  const controls = [
    {
      label: "Haft length",
      target: "shaft",
      key: "length",
      min: 0.45,
      max: 3.2,
      step: 0.01,
      unit: "m",
    },
    {
      label: "Haft radius",
      target: "shaft",
      key: "radius",
      min: 0.01,
      max: maximumAuthoredGripRadius(definition.shaft),
      step: 0.001,
      unit: "m",
    },
  ];
  if (!primary) return controls;
  const id = primary.id;
  const byKind = {
    mace: [componentControl("Head length", id, "length", 0.11, 0.28, 0.005, "m"), componentControl("Cusp radius", id, "cuspRadius", 0.045, 0.075, 0.001, "m"), componentControl("Flange count", id, "flanges", 4, 10, 1)],
    axe: [componentControl("Axe reach", id, "width", 0.095, 0.2, 0.005, "m"), componentControl("Axe height", id, "height", 0.14, 0.32, 0.01, "m"), componentControl("Edge curvature", id, "curvature", -0.02, 0.18, 0.01)],
    spear: [componentControl("Head length", id, "length", 0.18, 0.48, 0.01, "m"), componentControl("Head width", id, "width", 0.035, 0.12, 0.005, "m"), componentControl("Belly position", id, "bellyPosition", 0.1, 0.42, 0.01)],
    hammer: [componentControl("Poll length", id, "length", 0.05, 0.12, 0.001, "m"), componentControl("Face height", id, "face", 0.04, 0.1, 0.001, "m"), componentControl("Neck length ratio", id, "neckRatio", 0.35, 0.85, 0.01)],
    beak: [componentControl("Beak length", id, "length", 0.09, 0.2, 0.005, "m"), componentControl("Beak curve", id, "curvature", -0.03, 0.07, 0.005, "m"), componentControl("Tip set", id, "droop", -0.03, 0.04, 0.001, "m")],
    fork: [componentControl("Tine length", id, "length", 0.28, 0.52, 0.01, "m"), componentControl("Fork spread", id, "width", 0.09, 0.19, 0.005, "m"), componentControl("Crotch depth", id, "crotch", 0.25, 0.48, 0.01)],
    bill: [componentControl("Bill length", id, "length", 0.3, 0.5, 0.01, "m"), componentControl("Hook projection", id, "hook", 0.06, 0.1, 0.005, "m"), componentControl("Body width", id, "width", 0.07, 0.11, 0.005, "m")],
    glaive: [componentControl("Blade length", id, "length", 0.42, 0.68, 0.01, "m"), componentControl("Blade width", id, "width", 0.07, 0.15, 0.005, "m"), componentControl("Blade curvature", id, "curvature", 0.02, 0.22, 0.005, "m")],
    partisan: [componentControl("Blade length", id, "length", 0.32, 0.56, 0.01, "m"), componentControl("Blade width", id, "width", 0.09, 0.18, 0.005, "m"), componentControl("Lug span", id, "lugWidth", 0.1, 0.18, 0.005, "m")],
  };
  return [...controls, ...(byKind[primary.kind] ?? [])];
}

export const HEAD_KINDS = ["axe", "hammer", "beak", "spear", "blade", "mace"];
export function copyPreset(preset) {
  return {
    ...preset,
    definition: deepCopy(preset.definition),
    controls: deepCopy(preset.controls),
  };
}
export function getPath(object, path) {
  return path.split(".").reduce((current, part) => current[part], object);
}
export function setPath(object, path, value) {
  const parts = path.split(".");
  const key = parts.pop();
  parts.reduce((current, part) => current[part], object)[key] = value;
}
export function getControlValue(object, control) {
  if (control.target === "shaft") return object.shaft?.[control.key];
  if (control.componentId) return object.components.find((part) => part.id === control.componentId)?.[control.key];
  return getPath(object, control.path ?? control.paths[0]);
}
export function setControlValue(object, control, value) {
  if (control.target === "shaft") {
    object.shaft[control.key] = value;
    return;
  }
  if (control.componentId) {
    const part = object.components.find((candidate) => candidate.id === control.componentId);
    if (!part) throw new Error(`missing control component ${control.componentId}`);
    part[control.key] = value;
    return;
  }
  for (const path of control.paths ?? [control.path]) setPath(object, path, value);
}
