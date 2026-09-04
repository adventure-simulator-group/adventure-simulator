const bowControls = () => [
  { label: "Bow length", path: "components.0.length", min: 1.35, max: 2.10, step: 0.01, unit: "m" },
  { label: "Grip length", path: "components.0.gripLength", min: 0.10, max: 0.20, step: 0.01, unit: "m" },
  { label: "Limb width", path: "components.0.limbWidth", min: 0.026, max: 0.052, step: 0.002, unit: "m" },
  { label: "Limb depth", path: "components.0.limbDepth", min: 0.014, max: 0.036, step: 0.001, unit: "m" },
  { label: "Tip taper", path: "components.0.tipScale", min: 0.28, max: 0.60, step: 0.02, unit: "" },
  { label: "Limb reflex", path: "components.0.reflex", min: 0.02, max: 0.18, step: 0.01, unit: "m" },
  { label: "Tip recurve", path: "components.0.recurve", min: -0.12, max: 0.04, step: 0.01, unit: "m" },
  { label: "Brace height", path: "components.0.braceHeight", min: 0.12, max: 0.22, step: 0.01, unit: "m" },
  { label: "Upper limb proportion", path: "components.0.upperRatio", min: 0.48, max: 0.54, step: 0.01, unit: "" },
  { label: "String thickness", path: "components.0.stringRadius", min: 0.0007, max: 0.0015, step: 0.0001, unit: "m" },
  { label: "Nocking-loop radius", path: "components.0.loopRadius", min: 0.003, max: 0.006, step: 0.001, unit: "m" },
  { label: "Nocking-loop spacing", path: "components.0.loopGap", min: 0.008, max: 0.016, step: 0.002, unit: "m" },
];

const bowDefinition = (values) => ({ components: [{
  kind: "archeryBow", id: "bow", label: "bow", attach: { to: "weapon.root", at: "center" },
  construction: "self", limbSection: "dShape", length: 1.92, gripLength: 0.14, limbWidth: 0.036, limbDepth: 0.032,
  gripWidth: 0.035, gripDepth: 0.029, tipScale: 0.38, reflex: 0.07, recurve: 0,
  upperRatio: 0.51, braceHeight: 0.16, stringRadius: 0.001, loopRadius: 0.004,
  loopGap: 0.012, backingThickness: 0.003, hornThickness: 0.004, samples: 18,
  radialSegments: 8, material: "wood", coreMaterial: "wood", backMaterial: "sinew",
  hornMaterial: "horn", stringMaterial: "cord", nockMaterial: "horn", ...values,
}] });

export const BOW_PRESETS = [
  {
    id: "german-self-bow-1544", name: "Central European self bow", family: "Archery bow · c. 1500–1550",
    description: "A tall, mildly reflexed wooden self bow with a historically grounded D-section working limb, horn tip overlays, and a served string with compact nocking loops.",
    definition: bowDefinition({}), controls: bowControls(),
    choiceControls: [
      { label: "Limb section", path: "components.0.limbSection", options: [{ label: "D-section", value: "dShape" }, { label: "Oval", value: "oval" }, { label: "Flat", value: "flat" }] },
      { label: "Construction", path: "components.0.construction", options: [{ label: "Self bow", value: "self" }, { label: "Horn-wood-sinew composite", value: "composite" }] },
    ],
  },
  {
    id: "composite-recurve-bow-1544", name: "Reflex-recurve composite bow", family: "Archery bow · Central/Eastern European and Ottoman family",
    description: "A shorter reflex-recurve bow whose wood core, horn belly, and sinew backing remain visibly separate generated layers.",
    definition: bowDefinition({ construction: "composite", limbSection: "flat", length: 1.42, limbWidth: 0.044, limbDepth: 0.022, tipScale: 0.46, reflex: 0.15, recurve: -0.10, braceHeight: 0.19 }),
    controls: [...bowControls(),
      { label: "Horn belly thickness", path: "components.0.hornThickness", min: 0.002, max: 0.006, step: 0.001, unit: "m" },
      { label: "Sinew backing thickness", path: "components.0.backingThickness", min: 0.002, max: 0.006, step: 0.001, unit: "m" },
    ],
    choiceControls: [
      { label: "Limb section", path: "components.0.limbSection", options: [{ label: "Flat laminate", value: "flat" }, { label: "Oval", value: "oval" }, { label: "D-section", value: "dShape" }] },
      { label: "Construction", path: "components.0.construction", options: [{ label: "Horn-wood-sinew composite", value: "composite" }, { label: "Self bow", value: "self" }] },
    ],
  },
  {
    id: "flight-arrow-1544", name: "Fletched flight arrow", family: "Bow ammunition · c. 1544",
    description: "A parameterized wooden arrow with iron head, separate fletchings, and a real open nock slot sized to admit the bowstring.",
    definition: { components: [{
      kind: "arrow", id: "arrow", label: "flight arrow", attach: { to: "weapon.root", at: "base" },
      length: 0.76, shaftRadius: 0.0045, headLength: 0.050, headWidth: 0.022, headThickness: 0.004,
      fletchingLength: 0.13, fletchingHeight: 0.018, fletchingCount: 3, nockLength: 0.018,
      nockSlotWidth: 0.0036, maximumStringRadius: 0.0015, nockClearance: 0.0002,
      headStyle: "broadhead", nockStyle: "reinforced", segments: 12, material: "wood", headMaterial: "steel",
      fletchingMaterial: "feather", nockMaterial: "horn",
    }] },
    controls: [
      { label: "Arrow length", path: "components.0.length", min: 0.62, max: 0.88, step: 0.01, unit: "m" },
      { label: "Arrow shaft radius", path: "components.0.shaftRadius", min: 0.0035, max: 0.0055, step: 0.0005, unit: "m" },
      { label: "Arrowhead length", path: "components.0.headLength", min: 0.035, max: 0.075, step: 0.005, unit: "m" },
      { label: "Arrowhead width", path: "components.0.headWidth", min: 0.014, max: 0.032, step: 0.002, unit: "m" },
      { label: "Fletching length", path: "components.0.fletchingLength", min: 0.09, max: 0.18, step: 0.01, unit: "m" },
      { label: "Fletching height", path: "components.0.fletchingHeight", min: 0.010, max: 0.026, step: 0.002, unit: "m" },
      { label: "Nock slot width", path: "components.0.nockSlotWidth", min: 0.0032, max: 0.0044, step: 0.0002, unit: "m" },
    ],
    choiceControls: [
      { label: "Arrowhead", path: "components.0.headStyle", options: [{ label: "Broadhead", value: "broadhead" }, { label: "Bodkin", value: "bodkin" }] },
      { label: "Nock", path: "components.0.nockStyle", options: [{ label: "Horn reinforced", value: "reinforced" }, { label: "Self nock", value: "self" }] },
      { label: "Fletching count", path: "components.0.fletchingCount", options: [3, 4, 6].map((value) => ({ label: String(value), value })) },
      { label: "Fletching", path: "components.0.fletchingMaterial", options: [{ label: "Light feather", value: "feather" }, { label: "Dark feather", value: "darkFeather" }] },
    ],
  },
  {
    id: "arrow-quiver-1544", name: "Leather arrow quiver", family: "Archery carrier · c. 1544",
    description: "A tapered leather quiver with a capped bottom, hollow interior and mouth, bound rim, and attached shoulder strap.",
    definition: { components: [{
      kind: "arrowQuiver", id: "quiver", label: "arrow quiver", attach: { to: "weapon.root", at: "base" }, carrierStyle: "rigid",
      length: 0.60, mouthRadius: 0.060, bottomScale: 0.56, wall: 0.003, rimRadius: 0.006,
      strapWidth: 0.026, strapThickness: 0.004, strapDrop: 0.16, segments: 16,
      material: "leather", rimMaterial: "darkLeather", strapMaterial: "darkLeather",
    }] },
    controls: [
      { label: "Quiver length", path: "components.0.length", min: 0.48, max: 0.72, step: 0.02, unit: "m" },
      { label: "Quiver mouth radius", path: "components.0.mouthRadius", min: 0.045, max: 0.075, step: 0.005, unit: "m" },
      { label: "Quiver taper", path: "components.0.bottomScale", min: 0.42, max: 0.74, step: 0.02, unit: "" },
      { label: "Quiver wall", path: "components.0.wall", min: 0.002, max: 0.005, step: 0.001, unit: "m" },
      { label: "Quiver strap width", path: "components.0.strapWidth", min: 0.018, max: 0.040, step: 0.002, unit: "m" },
    ],
    choiceControls: [{ label: "Carrier", path: "components.0.carrierStyle", options: [{ label: "Rigid quiver", value: "rigid" }, { label: "Soft arrow bag / sheaf", value: "bag" }] }],
  },
];
