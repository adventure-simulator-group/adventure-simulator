const numericControls = (range = {}) => [
  { label: "Overall length", path: "components.0.length", min: range.lengthMin, max: range.lengthMax, step: 0.001, unit: "m" },
  { label: "Barrel length", path: "components.0.barrelLength", min: range.barrelMin, max: range.barrelMax, step: 0.001, unit: "m" },
  { label: "Bore diameter", path: "components.0.bore", min: range.boreMin, max: range.boreMax, step: 0.0001, unit: "m" },
  { label: "Barrel wall", path: "components.0.barrelWall", min: 0.0030, max: 0.0060, step: 0.0001, unit: "m" },
  { label: "Octagonal breech share", path: "components.0.octagonalRatio", min: 0.20, max: 0.48, step: 0.02, unit: "" },
  { label: "Butt width", path: "components.0.buttWidth", min: range.buttMin, max: range.buttMax, step: 0.002, unit: "m" },
  { label: "Lock waist width", path: "components.0.waistWidth", min: range.waistMin, max: range.waistMax, step: 0.002, unit: "m" },
  { label: "Fore-stock width", path: "components.0.foreWidth", min: range.foreMin, max: range.foreMax, step: 0.002, unit: "m" },
  { label: "Stock depth", path: "components.0.stockDepth", min: 0.044, max: range.depthMax, step: 0.001, unit: "m" },
  { label: "Butt drop", path: "components.0.buttDrop", min: 0.012, max: range.dropMax, step: 0.001, unit: "m" },
  { label: "Wheel / pivot radius", path: "components.0.lockWheelRadius", min: 0.016, max: 0.032, step: 0.002, unit: "m" },
  { label: "Pan width", path: "components.0.panWidth", min: 0.020, max: 0.038, step: 0.002, unit: "m" },
  { label: "Trigger length", path: "components.0.triggerLength", min: 0.055, max: 0.13, step: 0.005, unit: "m" },
  { label: "Ramrod radius", path: "components.0.ramrodRadius", min: 0.003, max: 0.006, step: 0.0005, unit: "m" },
];

const secondaryBarrelControl = { label: "Lower barrel length", path: "components.0.secondaryBarrelLength", min: 0.180, max: 0.208, step: 0.001, unit: "m" };

const definition = (values) => ({ components: [{
  kind: "firearm", id: "firearm", label: "small arm", attach: { to: "weapon.root", at: "base" },
  length: 0.492, firearmFamily: "pistol", stockStyle: "pistol", lockType: "wheellock", barrelCount: 2,
  barrelLength: 0.254, secondaryBarrelLength: 0.194, bore: 0.0117, barrelWall: 0.004, barrelSeparation: 0.018,
  octagonalRatio: 0.34, muzzleStyle: "ringed", muzzleFlare: 0.002, buttWidth: 0.052, waistWidth: 0.040,
  foreWidth: 0.034, stockDepth: 0.060, buttDrop: 0.030, lockPosition: 0.205, lockWheelRadius: 0.024,
  panWidth: 0.028, triggerLength: 0.080, guardWidth: 0.042, ramrodRadius: 0.004, bandCount: 2,
  sightStyle: "bead", facingStyle: "horn", facingThickness: 0.002, samples: 18, radialSegments: 12,
  material: "cherry", barrelMaterial: "darkSteel", lockMaterial: "steel", facingMaterial: "horn", furnitureMaterial: "gold", inlayMaterial: "staghorn", ...values,
}] });

const choicesFor = ({ barrelCount, lockType, stockStyle, sights }) => [
  { label: "Barrel count", path: "components.0.barrelCount", options: [{ label: `${barrelCount} barrel${barrelCount > 1 ? "s" : ""}`, value: barrelCount }] },
  { label: "Lock", path: "components.0.lockType", options: [{ label: lockType === "wheellock" ? "Wheellock" : "Matchlock", value: lockType }] },
  { label: "Stock form", path: "components.0.stockStyle", options: [{ label: stockStyle === "pistol" ? "Pistol grip and ball pommel" : "Shoulder stock", value: stockStyle }] },
  { label: "Muzzle", path: "components.0.muzzleStyle", options: [{ label: "Plain", value: "plain" }, { label: "Ringed / flared", value: "ringed" }] },
  { label: "Sight", path: "components.0.sightStyle", options: sights.map((value) => ({ label: value, value })) },
  { label: "Stock facing", path: "components.0.facingStyle", options: [{ label: "Plain wood", value: "none" }, { label: "Staghorn", value: "horn" }] },
];

export const FIREARM_PRESETS = [
  {
    id: "peter-peck-double-wheellock-pistol-1545", name: "Peter Peck double-barreled wheellock pistol",
    family: "Munich wheellock pistol · c. 1540–1545", description: "Parametric small arm anchored to Met 14.25.1425: 49.2 cm overall, stacked 25.4/19.4 cm barrels of 11.7 mm caliber, paired ignition trains, shallow swept cherry stock, gilt steel, and staghorn furniture.",
    definition: definition({}), controls: [...numericControls({ lengthMin: 0.46, lengthMax: 0.54, barrelMin: 0.240, barrelMax: 0.270, boreMin: 0.0105, boreMax: 0.0130, buttMin: 0.048, buttMax: 0.064, waistMin: 0.038, waistMax: 0.046, foreMin: 0.028, foreMax: 0.036, depthMax: 0.072, dropMax: 0.046 }), secondaryBarrelControl],
    choiceControls: choicesFor({ barrelCount: 2, lockType: "wheellock", stockStyle: "pistol", sights: ["none", "bead"] }),
  },
  {
    id: "german-matchlock-arquebus-16c", name: "German matchlock arquebus",
    family: "German matchlock long arm · 16th century", description: "Shoulder-fired matchlock anchored to Met 28.100.6: 160.3 cm overall, 121.6 cm barrel, 17.7 mm bore, and approximately 6.15 kg.",
    definition: definition({ length: 1.603, firearmFamily: "arquebus", stockStyle: "shoulder", lockType: "matchlock", barrelCount: 1, barrelLength: 1.216, secondaryBarrelLength: 0, bore: 0.0177, barrelWall: 0.0041, barrelSeparation: 0, octagonalRatio: 0.42, buttWidth: 0.080, waistWidth: 0.046, foreWidth: 0.032, stockDepth: 0.075, buttDrop: 0.055, lockPosition: 0.49, lockWheelRadius: 0.024, panWidth: 0.034, triggerLength: 0.115, guardWidth: 0.052, ramrodRadius: 0.005, bandCount: 3, sightStyle: "bead", facingStyle: "horn", material: "walnut", lockMaterial: "latten", facingMaterial: "bone", furnitureMaterial: "brass", inlayMaterial: "motherOfPearl" }),
    controls: numericControls({ lengthMin: 1.48, lengthMax: 1.70, barrelMin: 1.10, barrelMax: 1.30, boreMin: 0.015, boreMax: 0.020, buttMin: 0.074, buttMax: 0.106, waistMin: 0.042, waistMax: 0.054, foreMin: 0.028, foreMax: 0.040, depthMax: 0.135, dropMax: 0.075 }),
    choiceControls: choicesFor({ barrelCount: 1, lockType: "matchlock", stockStyle: "shoulder", sights: ["none", "bead", "notch"] }),
  },
  {
    id: "single-wheellock-pistol-study", name: "Single-barrel wheellock pistol study",
    family: "Comparative 16th-century pistol family study", description: "Honestly labeled single-barrel wheellock endpoint for exploring the shared generator; not an exact reconstruction of Met 14.25.1425.",
    definition: definition({ barrelCount: 1, barrelSeparation: 0, length: 0.47, barrelLength: 0.315, secondaryBarrelLength: 0, bore: 0.013, muzzleStyle: "plain", facingStyle: "none", lockMaterial: "steel", furnitureMaterial: "brass", inlayMaterial: "horn" }),
    controls: numericControls({ lengthMin: 0.44, lengthMax: 0.54, barrelMin: 0.28, barrelMax: 0.32, boreMin: 0.010, boreMax: 0.015, buttMin: 0.048, buttMax: 0.064, waistMin: 0.038, waistMax: 0.046, foreMin: 0.028, foreMax: 0.036, depthMax: 0.072, dropMax: 0.046 }),
    choiceControls: choicesFor({ barrelCount: 1, lockType: "wheellock", stockStyle: "pistol", sights: ["none", "bead"] }),
  },
  {
    id: "lead-round-ball", name: "Lead round ball", family: "Small-arms ammunition", description: "Independent spherical lead projectile. Radius is its only adjustable parameter; choose bore radius minus documented windage.",
    definition: { components: [{ kind: "leadBall", id: "ball", label: "lead round ball", attach: { to: "weapon.root", at: "base" }, radius: 0.0057, segments: 16, material: "lead" }] },
    controls: [{ label: "Ball radius", path: "components.0.radius", min: 0.0048, max: 0.0095, step: 0.0001, unit: "m" }], choiceControls: [],
  },
  {
    id: "small-arms-ball-pouch", name: "Small-arms ball pouch", family: "Projectile carrier · 16th century", description: "Independent leather ball pouch with open mouth, hinged flap, toggle, and two belt loops; contains no powder geometry.",
    definition: { components: [{ kind: "ballPouch", id: "pouch", label: "ball pouch", attach: { to: "weapon.root", at: "base" }, width: 0.15, height: 0.13, depth: 0.055, wall: 0.003, flapLength: 0.085, flapOverlap: 0.025, flapAngle: 0, beltLoopWidth: 0.028, beltLoopGap: 0.070, closureStyle: "toggle", material: "leather", hardwareMaterial: "horn" }] },
    controls: [
      { label: "Pouch width", path: "components.0.width", min: 0.11, max: 0.20, step: 0.01, unit: "m" },
      { label: "Pouch height", path: "components.0.height", min: 0.10, max: 0.18, step: 0.01, unit: "m" },
      { label: "Pouch depth", path: "components.0.depth", min: 0.04, max: 0.08, step: 0.005, unit: "m" },
      { label: "Flap length", path: "components.0.flapLength", min: 0.06, max: 0.12, step: 0.005, unit: "m" },
      { label: "Flap overlap", path: "components.0.flapOverlap", min: 0.015, max: 0.040, step: 0.005, unit: "m" },
      { label: "Flap angle", path: "components.0.flapAngle", min: 0, max: 120, step: 5, unit: "deg" },
    ],
    choiceControls: [{ label: "Closure", path: "components.0.closureStyle", options: [{ label: "Horn toggle", value: "toggle" }, { label: "Buckle tongue", value: "buckle" }] }],
  },
];
