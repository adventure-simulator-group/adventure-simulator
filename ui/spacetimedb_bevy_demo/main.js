import initWasm, {
  receive_world_snapshot,
  set_connection_identity_hex,
} from "./pkg/as_stdb_bevy_demo.js";

import {
  AlgebraicType,
  BinaryWriter,
  DbConnectionBuilder,
  Identity,
  ProductTypeElement,
} from "https://esm.sh/@clockworklabs/spacetimedb-sdk@1.3.3";

const LS_PREFIX = "as.stdb.bevy.v1:";
const DEFAULTS = {
  host: "https://maincloud.spacetimedb.com",
  db: "",
  characterId: "demo",
  displayName: "Demo Adventurer",
  token: "",
};

function loadSettings() {
  return {
    host: localStorage.getItem(LS_PREFIX + "host") ?? DEFAULTS.host,
    db: localStorage.getItem(LS_PREFIX + "db") ?? DEFAULTS.db,
    characterId: localStorage.getItem(LS_PREFIX + "characterId") ?? DEFAULTS.characterId,
    displayName: localStorage.getItem(LS_PREFIX + "displayName") ?? DEFAULTS.displayName,
    token: localStorage.getItem(LS_PREFIX + "token") ?? DEFAULTS.token,
  };
}

function saveSettings(s) {
  localStorage.setItem(LS_PREFIX + "host", s.host);
  localStorage.setItem(LS_PREFIX + "db", s.db);
  localStorage.setItem(LS_PREFIX + "characterId", s.characterId);
  localStorage.setItem(LS_PREFIX + "displayName", s.displayName);
  if (s.token) localStorage.setItem(LS_PREFIX + "token", s.token);
}

function $(id) {
  const el = document.getElementById(id);
  if (!el) throw new Error(`Missing element #${id}`);
  return el;
}

const elHost = $("stdb-host");
const elDb = $("stdb-db");
const elCharacterId = $("stdb-character-id");
const elDisplayName = $("stdb-display-name");
const elConnect = $("stdb-connect");
const elDisconnect = $("stdb-disconnect");
const elBanner = $("overlay-banner");

let conn = null;
let snapshotTimer = null;

function setBanner(text) {
  elBanner.textContent = text;
}

function setConnectedUI(connected) {
  elConnect.disabled = connected;
  elDisconnect.disabled = !connected;
}

// --- SpacetimeDB schema mirror (manual, kept in sync with the Rust module) ---
const tIdentity = AlgebraicType.createIdentityType();
const tU64 = AlgebraicType.createU64Type();
const tI64 = AlgebraicType.createI64Type();
const tI32 = AlgebraicType.createI32Type();
const tF32 = AlgebraicType.createF32Type();
const tBool = AlgebraicType.createBoolType();
const tString = AlgebraicType.createStringType();
const tScheduleAt = AlgebraicType.createScheduleAtType();

const row_Player = AlgebraicType.createProductType([
  new ProductTypeElement("identity", tIdentity),
  new ProductTypeElement("character_id", tString),
  new ProductTypeElement("display_name", tString),
]);

const row_Character = AlgebraicType.createProductType([
  new ProductTypeElement("identity", tIdentity),
  new ProductTypeElement("name", tString),
  new ProductTypeElement("hp_current", tI32),
  new ProductTypeElement("hp_max", tI32),
  new ProductTypeElement("alive", tBool),
  new ProductTypeElement("deaths", tI32),
  new ProductTypeElement("xp", tI32),
  new ProductTypeElement("respawn_at_micros", tI64),
  new ProductTypeElement("last_damage_at_micros", tI64),
]);

const row_PlayerInput = AlgebraicType.createProductType([
  new ProductTypeElement("identity", tIdentity),
  new ProductTypeElement("dx", tF32),
  new ProductTypeElement("dz", tF32),
]);

const row_PlayerTransform = AlgebraicType.createProductType([
  new ProductTypeElement("identity", tIdentity),
  new ProductTypeElement("x", tF32),
  new ProductTypeElement("y", tF32),
  new ProductTypeElement("z", tF32),
]);

const row_HazardBot = AlgebraicType.createProductType([
  new ProductTypeElement("id", tU64),
  new ProductTypeElement("x", tF32),
  new ProductTypeElement("y", tF32),
  new ProductTypeElement("z", tF32),
]);

const row_StaticEntity = AlgebraicType.createProductType([
  new ProductTypeElement("id", tU64),
  new ProductTypeElement("kind", tString),
  new ProductTypeElement("x", tF32),
  new ProductTypeElement("y", tF32),
  new ProductTypeElement("z", tF32),
]);

const row_PickupItem = AlgebraicType.createProductType([
  new ProductTypeElement("id", tU64),
  new ProductTypeElement("item_id", tString),
  new ProductTypeElement("qty", tI32),
  new ProductTypeElement("x", tF32),
  new ProductTypeElement("y", tF32),
  new ProductTypeElement("z", tF32),
]);

const row_CharacterQuest = AlgebraicType.createProductType([
  new ProductTypeElement("id", tU64),
  new ProductTypeElement("owner", tIdentity),
  new ProductTypeElement("quest_id", tString),
  new ProductTypeElement("status", tString),
  new ProductTypeElement("updated_at_micros", tI64),
]);

const row_InventoryItem = AlgebraicType.createProductType([
  new ProductTypeElement("id", tU64),
  new ProductTypeElement("owner", tIdentity),
  new ProductTypeElement("item_id", tString),
  new ProductTypeElement("qty", tI32),
]);

const row_LootBag = AlgebraicType.createProductType([
  new ProductTypeElement("id", tU64),
  new ProductTypeElement("owner", tIdentity),
  new ProductTypeElement("created_at_micros", tI64),
  new ProductTypeElement("x", tF32),
  new ProductTypeElement("y", tF32),
  new ProductTypeElement("z", tF32),
]);

const row_LootBagItem = AlgebraicType.createProductType([
  new ProductTypeElement("id", tU64),
  new ProductTypeElement("bag_id", tU64),
  new ProductTypeElement("item_id", tString),
  new ProductTypeElement("qty", tI32),
]);

const row_WorldTickSchedule = AlgebraicType.createProductType([
  new ProductTypeElement("scheduled_id", tU64),
  new ProductTypeElement("scheduled_at", tScheduleAt),
]);

function serializeArgs(argsType, argsObj) {
  const w = new BinaryWriter(256);
  argsType.serialize(w, argsObj);
  return w.getBuffer();
}

class RemoteTables {
  constructor(connection) {
    this.connection = connection;
  }
  get player() {
    return this.connection.clientCache.getOrCreateTable(REMOTE_MODULE.tables.player);
  }
  get character() {
    return this.connection.clientCache.getOrCreateTable(REMOTE_MODULE.tables.character);
  }
  get player_input() {
    return this.connection.clientCache.getOrCreateTable(REMOTE_MODULE.tables.player_input);
  }
  get player_transform() {
    return this.connection.clientCache.getOrCreateTable(REMOTE_MODULE.tables.player_transform);
  }
  get hazard_bot() {
    return this.connection.clientCache.getOrCreateTable(REMOTE_MODULE.tables.hazard_bot);
  }
  get static_entity() {
    return this.connection.clientCache.getOrCreateTable(REMOTE_MODULE.tables.static_entity);
  }
  get pickup_item() {
    return this.connection.clientCache.getOrCreateTable(REMOTE_MODULE.tables.pickup_item);
  }
  get character_quest() {
    return this.connection.clientCache.getOrCreateTable(REMOTE_MODULE.tables.character_quest);
  }
  get inventory_item() {
    return this.connection.clientCache.getOrCreateTable(REMOTE_MODULE.tables.inventory_item);
  }
  get loot_bag() {
    return this.connection.clientCache.getOrCreateTable(REMOTE_MODULE.tables.loot_bag);
  }
  get loot_bag_item() {
    return this.connection.clientCache.getOrCreateTable(REMOTE_MODULE.tables.loot_bag_item);
  }
}

class RemoteReducers {
  constructor(connection) {
    this.connection = connection;
  }
  join_world(character_id, display_name) {
    const args = serializeArgs(REMOTE_MODULE.reducers.join_world.argsType, { character_id, display_name });
    this.connection.callReducer("join_world", args, "FullUpdate");
  }
  set_input(dx, dz) {
    const args = serializeArgs(REMOTE_MODULE.reducers.set_input.argsType, { dx, dz });
    this.connection.callReducer("set_input", args, "NoSuccessNotify");
  }
  interact() {
    const args = serializeArgs(REMOTE_MODULE.reducers.interact.argsType, {});
    this.connection.callReducer("interact", args, "FullUpdate");
  }
  respawn() {
    const args = serializeArgs(REMOTE_MODULE.reducers.respawn.argsType, {});
    this.connection.callReducer("respawn", args, "FullUpdate");
  }
}

function eventContext(connection, event) {
  return {
    db: connection.db,
    reducers: connection.reducers,
    setReducerFlags: connection.setReducerFlags,
    isActive: connection.isActive,
    subscriptionBuilder: () => connection.subscriptionBuilder(),
    disconnect: () => connection.disconnect(),
    event,
  };
}

const REMOTE_MODULE = {
  tables: {
    player: { tableName: "player", rowType: row_Player, primaryKeyInfo: { colName: "identity", colType: tIdentity } },
    character: { tableName: "character", rowType: row_Character, primaryKeyInfo: { colName: "identity", colType: tIdentity } },
    player_input: { tableName: "player_input", rowType: row_PlayerInput, primaryKeyInfo: { colName: "identity", colType: tIdentity } },
    player_transform: { tableName: "player_transform", rowType: row_PlayerTransform, primaryKeyInfo: { colName: "identity", colType: tIdentity } },
    hazard_bot: { tableName: "hazard_bot", rowType: row_HazardBot, primaryKeyInfo: { colName: "id", colType: tU64 } },
    static_entity: { tableName: "static_entity", rowType: row_StaticEntity, primaryKeyInfo: { colName: "id", colType: tU64 } },
    pickup_item: { tableName: "pickup_item", rowType: row_PickupItem, primaryKeyInfo: { colName: "id", colType: tU64 } },
    character_quest: { tableName: "character_quest", rowType: row_CharacterQuest, primaryKeyInfo: { colName: "id", colType: tU64 } },
    inventory_item: { tableName: "inventory_item", rowType: row_InventoryItem, primaryKeyInfo: { colName: "id", colType: tU64 } },
    loot_bag: { tableName: "loot_bag", rowType: row_LootBag, primaryKeyInfo: { colName: "id", colType: tU64 } },
    loot_bag_item: { tableName: "loot_bag_item", rowType: row_LootBagItem, primaryKeyInfo: { colName: "id", colType: tU64 } },
  },
  reducers: {
    join_world: {
      reducerName: "join_world",
      argsType: AlgebraicType.createProductType([
        new ProductTypeElement("character_id", tString),
        new ProductTypeElement("display_name", tString),
      ]),
    },
    set_input: {
      reducerName: "set_input",
      argsType: AlgebraicType.createProductType([
        new ProductTypeElement("dx", tF32),
        new ProductTypeElement("dz", tF32),
      ]),
    },
    interact: { reducerName: "interact", argsType: AlgebraicType.createProductType([]) },
    respawn: { reducerName: "respawn", argsType: AlgebraicType.createProductType([]) },
    world_tick: { reducerName: "world_tick", argsType: row_WorldTickSchedule },
  },
  eventContextConstructor: (imp, event) => eventContext(imp, event),
  dbViewConstructor: (imp) => new RemoteTables(imp),
  reducersConstructor: (imp) => new RemoteReducers(imp),
  setReducerFlagsConstructor: () => ({}),
  // The JS SDK expects a semver-ish string here.
  versionInfo: { cliVersion: "1.3.3" },
};

function snapshotFromCache(connection) {
  const players = connection.db.player.iter().map((p) => ({
    identity_hex: p.identity.toHexString(),
    display_name: p.display_name,
  }));

  const characters = connection.db.character.iter().map((c) => ({
    identity_hex: c.identity.toHexString(),
    name: c.name,
    hp_current: c.hp_current,
    hp_max: c.hp_max,
    alive: c.alive,
    deaths: c.deaths,
    xp: c.xp,
    respawn_at_micros: String(c.respawn_at_micros),
  }));

  const transforms = connection.db.player_transform.iter().map((t) => ({
    identity_hex: t.identity.toHexString(),
    x: t.x,
    y: t.y,
    z: t.z,
  }));

  const hazard_bots = connection.db.hazard_bot.iter().map((b) => ({
    id: String(b.id),
    x: b.x,
    y: b.y,
    z: b.z,
  }));

  const static_entities = connection.db.static_entity.iter().map((s) => ({
    id: String(s.id),
    kind: s.kind,
    x: s.x,
    y: s.y,
    z: s.z,
  }));

  const pickups = connection.db.pickup_item.iter().map((p) => ({
    id: String(p.id),
    item_id: p.item_id,
    qty: p.qty,
    x: p.x,
    y: p.y,
    z: p.z,
  }));

  const loot_bags = connection.db.loot_bag.iter().map((b) => ({
    id: String(b.id),
    owner_identity_hex: b.owner.toHexString(),
    x: b.x,
    y: b.y,
    z: b.z,
  }));

  return { players, characters, transforms, hazard_bots, static_entities, pickups, loot_bags };
}

function stopSnapshotLoop() {
  if (snapshotTimer) {
    clearInterval(snapshotTimer);
    snapshotTimer = null;
  }
}

function startSnapshotLoop(connection) {
  stopSnapshotLoop();
  snapshotTimer = setInterval(() => {
    if (!connection?.isActive) return;
    const snap = snapshotFromCache(connection);
    receive_world_snapshot(JSON.stringify(snap));
  }, 80);
}

function connect() {
  const host = elHost.value.trim();
  const db = elDb.value.trim();
  const characterId = elCharacterId.value.trim() || DEFAULTS.characterId;
  const displayName = elDisplayName.value.trim() || DEFAULTS.displayName;

  if (!host || !db) {
    setBanner("Host + DB are required.");
    return;
  }

  const saved = loadSettings();
  const token = saved.token || "";
  saveSettings({ host, db, characterId, displayName, token });

  setBanner("Connecting…");

  const builder = new DbConnectionBuilder(REMOTE_MODULE, (imp) => imp)
    .withUri(host)
    .withModuleName(db)
    .withCompression("gzip")
    .withLightMode(true);
  if (token) builder.withToken(token);

  conn = builder
    .onConnect((c, identity, newToken) => {
      setConnectedUI(true);
      const hex = identity.toHexString();
      setBanner(`Connected as ${hex}`);
      saveSettings({ host, db, characterId, displayName, token: newToken });
      set_connection_identity_hex(hex);

      // Global callbacks for the Bevy WASM module.
      window.stdb_set_input = (dx, dz) => c.reducers.set_input(dx, dz);
      window.stdb_interact = () => c.reducers.interact();
      window.stdb_respawn = () => c.reducers.respawn();

      c.subscriptionBuilder().onApplied(() => {
        c.reducers.join_world(characterId, displayName);
        startSnapshotLoop(c);
      }).subscribeToAllTables();
    })
    .onConnectError((_ctx, err) => {
      console.error(err);
      conn = null;
      stopSnapshotLoop();
      setConnectedUI(false);
      setBanner(`Connect error: ${String(err)}`);
    })
    .onDisconnect((_ctx, err) => {
      console.warn("Disconnected", err);
      conn = null;
      stopSnapshotLoop();
      setConnectedUI(false);
      setBanner("Disconnected");
    })
    .build();
}

function disconnect() {
  if (conn) conn.disconnect();
}

async function boot() {
  // Initialize WASM (Bevy starts automatically via #[wasm_bindgen(start)]).
  try {
    await initWasm();
  } catch (e) {
    setBanner("WASM init failed (did you build pkg/?)");
    console.error(e);
  }

  // Provide stub globals so the WASM side can call without throwing before connect.
  window.stdb_set_input = () => {};
  window.stdb_interact = () => {};
  window.stdb_respawn = () => {};

  const s = loadSettings();
  elHost.value = s.host;
  elDb.value = s.db;
  elCharacterId.value = s.characterId;
  elDisplayName.value = s.displayName;

  elConnect.addEventListener("click", connect);
  elDisconnect.addEventListener("click", disconnect);
}

boot();
