const controls = (range = {}) => [
  { label: "Tiller length", path: "components.0.length", min: range.lengthMin ?? 0.64, max: range.lengthMax ?? 0.92, step: 0.001, unit: "m" },
  { label: "Butt width", path: "components.0.buttWidth", min: 0.042, max: 0.075, step: 0.001, unit: "m" },
  { label: "Tiller waist", path: "components.0.waistWidth", min: 0.036, max: 0.064, step: 0.002, unit: "m" },
  { label: "Fore-end width", path: "components.0.noseWidth", min: 0.038, max: 0.055, step: 0.001, unit: "m" },
  { label: "Stock thickness", path: "components.0.stockThickness", min: 0.032, max: 0.065, step: 0.001, unit: "m" },
  { label: "Butt drop", path: "components.0.buttDrop", min: 0.018, max: 0.060, step: 0.002, unit: "m" },
  { label: "Lock-table height", path: "components.0.lockTableHeight", min: 0.032, max: 0.060, step: 0.002, unit: "m" },
  { label: "Fore-end rise", path: "components.0.foreEndRise", min: 0.004, max: 0.024, step: 0.002, unit: "m" },
  { label: "Prod span", path: "components.0.prodSpan", min: 0.42, max: 0.82, step: 0.001, unit: "m" },
  { label: "Prod depth", path: "components.0.prodDepth", min: 0.030, max: 0.065, step: 0.001, unit: "m" },
  { label: "Prod thickness", path: "components.0.prodThickness", min: 0.008, max: 0.020, step: 0.001, unit: "m" },
  { label: "Prod sweep", path: "components.0.prodSweep", min: -0.04, max: 0.08, step: 0.01, unit: "m" },
  { label: "Prod tip taper", path: "components.0.prodTipScale", min: 0.38, max: 0.68, step: 0.02, unit: "" },
  { label: "Draw length", path: "components.0.nutPosition", min: range.nutMin ?? 0.28, max: range.nutMax ?? 0.42, step: 0.001, unit: "m" },
  { label: "String thickness", path: "components.0.stringRadius", min: 0.0012, max: 0.0022, step: 0.0002, unit: "m" },
  { label: "Served center width", path: "components.0.servingWidth", min: 0.018, max: 0.042, step: 0.002, unit: "m" },
  { label: "Tip-loop clearance", path: "components.0.tipLoopClearance", min: 0.003, max: 0.008, step: 0.001, unit: "m" },
  { label: "Bridle spacing", path: "components.0.bridleSpacing", min: 0.035, max: 0.075, step: 0.005, unit: "m" },
  { label: "Nut radius", path: "components.0.nutRadius", min: 0.014, max: 0.026, step: 0.002, unit: "m" },
  { label: "Trigger length", path: "components.0.triggerLength", min: 0.10, max: 0.22, step: 0.01, unit: "m" },
  { label: "Stirrup width", path: "components.0.stirrupWidth", min: 0.075, max: 0.14, step: 0.005, unit: "m" },
  { label: "Stirrup length", path: "components.0.stirrupLength", min: 0.08, max: 0.16, step: 0.01, unit: "m" },
];

const definition = (values = {}) => ({ components: [{
  kind: "crossbow", id: "crossbow", label: "crossbow", attach: { to: "weapon.root", at: "base" },
  length: 0.68, buttWidth: 0.058, waistWidth: 0.048, noseWidth: 0.045, stockThickness: 0.048,
  stockStyle: "hunting", prodConstruction: "steel", prodPosition: 0.62, prodSpan: 0.60,
  prodDepth: 0.045, prodThickness: 0.014, prodSweep: 0.02, prodTipScale: 0.50,
  hornThickness: 0.009, sinewThickness: 0.007, stringRadius: 0.0018, servingWidth: 0.028,
  tipLoopClearance: 0.005, bridleSpacing: 0.05, bridleRadius: 0.003, nutPosition: 0.40,
  nutRadius: 0.020, triggerLength: 0.16, grooveWidth: 0.012, stirrupWidth: 0.105,
  stirrupLength: 0.12, stirrupBar: 0.005, spanningMode: "cranequin", sightStyle: "peep",
  buttDrop: 0.036, lockTableHeight: 0.046, foreEndRise: 0.012, nutWidth: 0.036, nutThickness: 0.014,
  railHeight: 0.007, facingStyle: "horn", facingThickness: 0.002, spanningBar: 0.008,
  samples: 18, radialSegments: 8, material: "wood", stringMaterial: "cord", bindingMaterial: "cord",
  coreMaterial: "wood", hornMaterial: "horn", backMaterial: "sinew", ...values,
}] });

const choices = [
  { label: "Stock profile", path: "components.0.stockStyle", options: [{ label: "Straight", value: "straight" }, { label: "Hunting waist", value: "hunting" }, { label: "Swollen target", value: "swollen" }] },
  { label: "Prod construction", path: "components.0.prodConstruction", options: [{ label: "Steel prod", value: "steel" }, { label: "Horn-wood-sinew composite", value: "composite" }] },
  { label: "Spanning accommodation", path: "components.0.spanningMode", options: [{ label: "Cranequin rest", value: "cranequin" }, { label: "Goat's-foot lever ring", value: "goatsFoot" }, { label: "Belt-hook notches", value: "beltHook" }] },
  { label: "Sight", path: "components.0.sightStyle", options: [{ label: "None", value: "none" }, { label: "Folding peep", value: "peep" }, { label: "Post", value: "post" }] },
  { label: "Stock facing", path: "components.0.facingStyle", options: [{ label: "Plain walnut", value: "none" }, { label: "Staghorn facing", value: "horn" }] },
];

const choicesFor = ({ construction, spanning, sights }) => choices.map((choice) => {
  if (choice.label === "Prod construction") return { ...choice, options: choice.options.filter((option) => option.value === construction) };
  if (choice.label === "Spanning accommodation") return { ...choice, options: choice.options.filter((option) => spanning.includes(option.value)) };
  if (choice.label === "Sight") return { ...choice, options: choice.options.filter((option) => sights.includes(option.value)) };
  return choice;
});

export const CROSSBOW_PRESETS = [
  { id: "german-cranequin-crossbow-1544", name: "German cranequin hunting crossbow", family: "Heavy steel-prod crossbow · c. 1540–1560", description: "A heavy walnut-tiller hunting crossbow anchored to Met 14.25.1572a, with steel prod, hemp bridle, functional nut lock, stirrup, and cranequin rack purchase.", definition: definition({ length: 0.612, prodPosition: 0.565, prodSpan: 0.624, prodDepth: 0.043, prodThickness: 0.011, nutPosition: 0.385, stirrupLength: 0.12, stirrupWidth: 0.105, spanningBar: 0.007 }), controls: controls({ lengthMin: 0.58, lengthMax: 0.70, nutMin: 0.30, nutMax: 0.40 }), choiceControls: choicesFor({ construction: "steel", spanning: ["cranequin"], sights: ["none", "peep"] }) },
  { id: "central-composite-arbalest", name: "Composite-prod arbalest", family: "Retained Central European composite family", description: "A horn-wood-sinew prod family retained alongside early steel prods, with substantial bridle binding, a working nut lock, and goat's-foot pivot lugs.", definition: definition({ prodConstruction: "composite", spanningMode: "goatsFoot", sightStyle: "none", length: 0.76, prodSpan: 0.72, prodDepth: 0.060, prodThickness: 0.016, prodSweep: 0.06, nutPosition: 0.44 }), controls: controls({ lengthMin: 0.64, lengthMax: 0.92, nutMin: 0.30, nutMax: 0.44 }), choiceControls: choicesFor({ construction: "composite", spanning: ["goatsFoot"], sights: ["none"] }) },
  { id: "light-target-crossbow-comparative", name: "Comparative light target crossbow", family: "Undated light crossbow family study", description: "A compact comparative study with a slender steel prod and belt-hook purchase surfaces; it is not presented as a sourced 1544 German object.", definition: definition({ length: 0.56, prodPosition: 0.51, prodSpan: 0.46, prodDepth: 0.034, prodThickness: 0.009, prodSweep: 0, nutPosition: 0.30, stockThickness: 0.036, buttWidth: 0.046, waistWidth: 0.038, spanningMode: "beltHook", sightStyle: "none", stirrupWidth: 0.08, stirrupLength: 0.09, facingStyle: "none" }), controls: controls({ lengthMin: 0.52, lengthMax: 0.64, nutMin: 0.28, nutMax: 0.42 }), choiceControls: choicesFor({ construction: "steel", spanning: ["beltHook"], sights: ["none"] }) },
  { id: "crossbow-bolt-1544", name: "Crossbow bolt / quarrel", family: "Crossbow ammunition · c. 1544", description: "A short heavy quarrel with a flattened horn-reinforced butt that bears on the runner and nut shelf, never an arrow-style string nock.", definition: { components: [{ kind: "crossbowBolt", id: "bolt", label: "crossbow bolt", attach: { to: "weapon.root", at: "base" }, length: 0.42, shaftRadius: 0.006, headLength: 0.055, headWidth: 0.024, headThickness: 0.006, headStyle: "bodkin", boltUse: "war", fletchingLength: 0.09, fletchingHeight: 0.018, fletchingCount: 2, buttLength: 0.024, buttWidth: 0.010, buttHeight: 0.006, segments: 12, material: "wood", headMaterial: "steel", fletchingMaterial: "leather", buttMaterial: "horn" }] }, controls: [
    { label: "Bolt length", path: "components.0.length", min: 0.28, max: 0.48, step: 0.01, unit: "m" }, { label: "Bolt radius", path: "components.0.shaftRadius", min: 0.0045, max: 0.0075, step: 0.0005, unit: "m" }, { label: "Head length", path: "components.0.headLength", min: 0.035, max: 0.075, step: 0.005, unit: "m" }, { label: "Head width", path: "components.0.headWidth", min: 0.016, max: 0.038, step: 0.002, unit: "m" }, { label: "Butt width", path: "components.0.buttWidth", min: 0.010, max: 0.012, step: 0.001, unit: "m" },
  ], choiceControls: [{ label: "Bolt head", path: "components.0.headStyle", options: ["bodkin", "broadhead", "hunting"].map((value) => ({ label: value, value })) }, { label: "Bolt use / vanes", path: "components.0.boltUse", options: [{ label: "War quarrel: two stiff vanes", value: "war" }, { label: "Hunting bolt: three feathers", value: "hunting" }] }] },
  { id: "bolt-quiver-1544", name: "Broad German bolt quiver", family: "Crossbow bolt carrier · early 16th century", description: "Layered wood, paper, hide, and leather carrier based on Met 29.158.646a-l: broad tapered body, open mouth, closed base, and attached strap.", definition: { components: [{ kind: "boltQuiver", id: "bolt-quiver", label: "bolt quiver", attach: { to: "weapon.root", at: "base" }, carrierStyle: "rigid", length: 0.446, bottomWidth: 0.29, mouthWidth: 0.19, depth: 0.07, wall: 0.0018, lining: 0.0003, hideCover: 0.0004, strapWidth: 0.03, strapThickness: 0.003, strapDrop: 0.12, material: "wood", rimMaterial: "leather", strapMaterial: "darkLeather" }] }, controls: [
    { label: "Carrier length", path: "components.0.length", min: 0.40, max: 0.50, step: 0.001, unit: "m" }, { label: "Bottom width", path: "components.0.bottomWidth", min: 0.25, max: 0.32, step: 0.01, unit: "m" }, { label: "Mouth width", path: "components.0.mouthWidth", min: 0.16, max: 0.22, step: 0.01, unit: "m" }, { label: "Body depth", path: "components.0.depth", min: 0.055, max: 0.085, step: 0.005, unit: "m" }, { label: "Wood wall", path: "components.0.wall", min: 0.0014, max: 0.0030, step: 0.0002, unit: "m" },
  ], choiceControls: [{ label: "Carrier construction", path: "components.0.carrierStyle", options: [{ label: "Layered rigid German quiver", value: "rigid" }] }] },
];
