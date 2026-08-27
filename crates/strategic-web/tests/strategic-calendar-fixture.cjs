const fs = require("node:fs");
const path = require("node:path");

const source = fs.readFileSync(
  path.join(__dirname, "..", "..", "adventuresim-core", "src", "strategic_time.rs"),
  "utf8",
);

const integerProduct = (name) => {
  const declaration = new RegExp(`pub const ${name}: u64 = ([^;]+);`).exec(source);
  if (!declaration) throw new Error(`missing strategic-time constant ${name}`);
  return declaration[1]
    .split("*")
    .map((factor) => Number(factor.trim().replaceAll("_", "")))
    .reduce((product, factor) => product * factor, 1);
};

module.exports = Object.freeze({
  minutesPerDay: integerProduct("MINUTES_PER_DAY"),
  daysPerYear: integerProduct("DAYS_PER_YEAR"),
  lunarCycleMinutes: integerProduct("LUNAR_CYCLE_MINUTES"),
});
