const fs = require("node:fs");
const path = require("node:path");

/**
 * Expand a Rust facade's `include!` implementation fragments in place.
 * Source-contract tests should observe the logical module in declaration
 * order, not the physical file chosen for a behavior domain. `include_str!`
 * calls are deliberately left alone because they describe test fixtures, not
 * compiled module contents.
 */
function readRustModuleSource(facadePath) {
  const expand = (sourcePath) => {
    const directory = path.dirname(sourcePath);
    return fs.readFileSync(sourcePath, "utf8").replace(
      /include!\("([^"]+)"\);?/g,
      (_invocation, relativePath) => expand(path.join(directory, relativePath)),
    );
  };
  return expand(facadePath);
}

module.exports = { readRustModuleSource };
