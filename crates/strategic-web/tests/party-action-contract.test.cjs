const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const { readRustModuleSource } = require("./rust-module-source.cjs");

const web = fs.readFileSync(
  "crates/strategic-web/src/routes/party_actions.rs",
  "utf8",
);
const moduleSource = readRustModuleSource(
  "crates/adventuresim-stdb-module/src/strategic/mod.rs",
);
const tacticalModule = fs.readFileSync(
  "crates/adventuresim-stdb-module/src/tactical.rs",
  "utf8",
);
const tacticalServer = fs.readFileSync(
  "crates/adventuresim-tactical-server/src/player_projection.rs",
  "utf8",
);
const characterModule = fs.readFileSync(
  "crates/adventuresim-stdb-module/src/character.rs",
  "utf8",
);

const contract = [
  ["TravelToSettlement", "travel"],
  ["TravelToCaseSite", "travel"],
  ["RemovePartyMember", "kick"],
  ["CreateRecruitmentRole", "add_role"],
  ["UpdateRecruitmentRole", "edit_role"],
  ["DeleteRecruitmentRole", "delete_role"],
  ["AcceptJoinRequest", "accept_join"],
  ["RejectJoinRequest", "reject_join"],
  ["AcceptContract", "accept_contract"],
  ["AbandonContract", "abandon_contract"],
  ["ReportContract", "report_contract"],
  ["AutoresolveMission", "autoresolve"],
  ["UpdatePartyCheckTargets", "party_checks"],
  ["SetInventoryQuantityTarget", "party_inventory"],
  ["DisbandParty", "disband_party"],
  ["RequestTacticalServer", "initiate_combat"],
  ["CancelMission", "cancel_mission"],
  ["PerformInvestigation", "investigate"],
];

test("web and module retain the complete party-action variant/kind contract", () => {
  for (const [variant, kind] of contract) {
    for (const [boundary, source] of [
      ["web", web],
      ["module", moduleSource],
    ]) {
      assert.match(source, new RegExp(`Self::${variant}[^=]*=>\\s*"${kind}"`), `${boundary}: ${variant}`);
    }
  }
});

test("tactical enrollment and departure retain server authority without creating join rows", () => {
  assert.match(tacticalModule, /character\.in_server[\s\S]*character\.server != server\.identity[\s\S]*tactical_server_authority\(\)/);
  assert.match(tacticalModule, /Only a registered tactical server can remove characters/);
  const joinHandler = tacticalServer.match(/pub\(crate\) fn on_join_request[\s\S]*?\n}\n\n\/\/\/ Standalone-mode/)?.[0];
  assert.ok(joinHandler, "join handler source boundary");
  assert.doesNotMatch(joinHandler, /create_character/);
  assert.match(joinHandler, /enter_mission/);
});

test("temporary-character cascade covers smith custody and death provenance", () => {
  const cleanup = characterModule.match(/pub\(crate\) fn delete_temporary_character[\s\S]*?\/\/\/ Create a new character/)?.[0];
  assert.ok(cleanup, "temporary-character cleanup source boundary");
  assert.match(cleanup, /repair_order\(\)[\s\S]*owner_id\(\)[\s\S]*inventory_item_id/);
  assert.match(cleanup, /character_death\(\)[\s\S]*character_id\(\)[\s\S]*delete/);
});

test("inventory transfer and liquidation use checked arithmetic before mutation", () => {
  assert.match(moduleSource, /merged_quantity[\s\S]*checked_add\(quantity\)/);
  assert.match(moduleSource, /liquidation line value overflow/);
  assert.match(moduleSource, /liquidation total overflow/);
  assert.match(moduleSource, /u32::try_from\(proceeds\)/);
});
