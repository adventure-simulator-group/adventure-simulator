import {
  AlgebraicType,
  BinaryWriter,
  DbConnectionBuilder,
  Identity,
  ProductTypeElement,
} from "https://esm.sh/@clockworklabs/spacetimedb-sdk@1.3.3";

const LS_PREFIX = "as.stdb.demo.v1:";

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
const elRoot = $("overlay-root");
const canvas = $("game-canvas");

/** @type {import("https://esm.sh/@clockworklabs/spacetimedb-sdk@1.3.3").DbConnectionImpl | null} */
let conn = null;
let connectedIdentityHex = null;
let loopsStarted = false;

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

const row_QuestDef = AlgebraicType.createProductType([
  new ProductTypeElement("quest_id", tString),
  new ProductTypeElement("title", tString),
  new ProductTypeElement("description", tString),
  new ProductTypeElement("reward_text", tString),
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
  get quest_def() {
    return this.connection.clientCache.getOrCreateTable(REMOTE_MODULE.tables.quest_def);
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
    quest_def: { tableName: "quest_def", rowType: row_QuestDef, primaryKeyInfo: { colName: "quest_id", colType: tString } },
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

function setBanner(text) {
  elBanner.textContent = text;
}

function setConnectedUI(connected) {
  elConnect.disabled = connected;
  elDisconnect.disabled = !connected;
}

function currentPlayerIdentity() {
  if (!conn?.identity) return null;
  return conn.identity;
}

function renderOverlay() {
  if (!conn) {
    elRoot.innerHTML = `<div id="overlay-banner" class="banner">Disconnected</div><div class="panel">Press Connect.</div>`;
    return;
  }

  const me = currentPlayerIdentity();
  const players = conn.db.player.iter();
  const characters = conn.db.character.iter();
  const inventory = conn.db.inventory_item.iter();
  const quests = conn.db.character_quest.iter();
  const lootBags = conn.db.loot_bag.iter();

  const meChar = me ? characters.find((c) => c.identity.isEqual(me)) : null;
  const meInv = me ? inventory.filter((it) => it.owner.isEqual(me)) : [];
  const meQuest = me ? quests.find((q) => q.owner.isEqual(me) && q.quest_id === "quest.pet_cat") : null;

  const status = conn.isActive ? "Connected" : "Disconnected";
  const idHex = conn.identity ? conn.identity.toHexString() : "(pending)";

  const respawn = meChar && !meChar.alive && meChar.respawn_at_micros
    ? ` · respawn_at: ${meChar.respawn_at_micros}`
    : "";

  const meLine = meChar
    ? `${meChar.name} · HP ${meChar.hp_current}/${meChar.hp_max} · ${meChar.alive ? "ALIVE" : "DEAD"}${respawn}`
    : "No character row (call join_world).";

  elRoot.innerHTML = `
    <div id="overlay-banner" class="banner">${escapeHtml(status)} · <code>${escapeHtml(idHex)}</code></div>
    <div class="panel">
      <div><strong>You</strong></div>
      <div class="muted">${escapeHtml(meLine)}</div>
      <div>Quest <code>quest.pet_cat</code>: <strong>${escapeHtml(meQuest?.status ?? "not-started")}</strong></div>
      <div>Inventory: <strong>${meInv.length}</strong> items</div>
      <div>Loot bags: <strong>${lootBags.length}</strong></div>
      <div class="muted">WASD move · E interact · R respawn</div>
    </div>
    <div class="panel">
      <div><strong>Players</strong> (${players.length})</div>
      <ul class="list">${players.map((p) => `<li><code>${escapeHtml(p.identity.toHexString())}</code> · ${escapeHtml(p.display_name)}</li>`).join("")}</ul>
    </div>
  `;
}

function resizeCanvas() {
  const dpr = window.devicePixelRatio || 1;
  const rect = canvas.getBoundingClientRect();
  canvas.width = Math.max(1, Math.floor(rect.width * dpr));
  canvas.height = Math.max(1, Math.floor(rect.height * dpr));
}

function drawWorld() {
  if (!conn) return;

  const ctx2d = canvas.getContext("2d");
  if (!ctx2d) return;

  const dpr = window.devicePixelRatio || 1;
  ctx2d.setTransform(1, 0, 0, 1, 0, 0);
  ctx2d.clearRect(0, 0, canvas.width, canvas.height);
  ctx2d.scale(dpr, dpr);

  const w = canvas.width / dpr;
  const h = canvas.height / dpr;
  const cx = w * 0.5;
  const cy = h * 0.5;
  const scale = 28;

  const me = currentPlayerIdentity();

  const transforms = conn.db.player_transform.iter();
  const chars = conn.db.character.iter();
  const bot = conn.db.hazard_bot.iter().find((b) => b.id === 1n);
  const statics = conn.db.static_entity.iter();
  const pickups = conn.db.pickup_item.iter();
  const bags = conn.db.loot_bag.iter();

  const aliveById = new Map(chars.map((c) => [c.identity.toHexString(), c.alive]));

  function worldToScreen(x, z) {
    return [cx + x * scale, cy + z * scale];
  }

  // Grid
  ctx2d.globalAlpha = 0.22;
  ctx2d.strokeStyle = "rgba(255, 215, 140, 0.25)";
  for (let gx = -20; gx <= 20; gx += 1) {
    const [sx] = worldToScreen(gx, 0);
    ctx2d.beginPath();
    ctx2d.moveTo(sx, 0);
    ctx2d.lineTo(sx, h);
    ctx2d.stroke();
  }
  for (let gz = -20; gz <= 20; gz += 1) {
    const [, sz] = worldToScreen(0, gz);
    ctx2d.beginPath();
    ctx2d.moveTo(0, sz);
    ctx2d.lineTo(w, sz);
    ctx2d.stroke();
  }
  ctx2d.globalAlpha = 1.0;

  // Statics
  for (const s of statics) {
    const [sx, sy] = worldToScreen(s.x, s.z);
    ctx2d.fillStyle = s.kind === "quest_giver" ? "#d77be8" : "#f2c94c";
    drawCircle(ctx2d, sx, sy, 10);
  }

  // Pickups
  for (const p of pickups) {
    const [sx, sy] = worldToScreen(p.x, p.z);
    ctx2d.fillStyle = "#38d26f";
    drawRect(ctx2d, sx, sy, 10);
  }

  // Loot bags
  for (const b of bags) {
    const [sx, sy] = worldToScreen(b.x, b.z);
    ctx2d.fillStyle = "rgba(255,255,255,0.85)";
    drawRect(ctx2d, sx, sy, 12);
  }

  // Players
  for (const t of transforms) {
    const alive = aliveById.get(t.identity.toHexString()) ?? true;
    const [sx, sy] = worldToScreen(t.x, t.z);
    if (!alive) {
      ctx2d.fillStyle = "rgba(120,120,120,0.7)";
      drawCircle(ctx2d, sx, sy, 10);
      continue;
    }
    const isMe = me && t.identity.isEqual(me);
    ctx2d.fillStyle = isMe ? "#4aa3ff" : "rgba(180, 205, 255, 0.75)";
    drawCircle(ctx2d, sx, sy, isMe ? 12 : 10);
  }

  // Hazard bot
  if (bot) {
    const [sx, sy] = worldToScreen(bot.x, bot.z);
    ctx2d.fillStyle = "#ff4a4a";
    drawCircle(ctx2d, sx, sy, 12);
  }
}

function drawCircle(ctx2d, x, y, r) {
  ctx2d.beginPath();
  ctx2d.arc(x, y, r, 0, Math.PI * 2);
  ctx2d.fill();
}

function drawRect(ctx2d, x, y, s) {
  ctx2d.fillRect(x - s * 0.5, y - s * 0.5, s, s);
}

function escapeHtml(s) {
  return String(s)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll("\"", "&quot;")
    .replaceAll("'", "&#39;");
}

const keys = new Set();
let lastSent = { dx: 0, dz: 0 };

function computeInput() {
  let dx = 0;
  let dz = 0;
  if (keys.has("KeyA")) dx -= 1;
  if (keys.has("KeyD")) dx += 1;
  if (keys.has("KeyW")) dz -= 1;
  if (keys.has("KeyS")) dz += 1;
  return { dx, dz };
}

function pumpInput() {
  if (!conn || !conn.identity) return;
  const { dx, dz } = computeInput();
  if (dx === lastSent.dx && dz === lastSent.dz) return;
  lastSent = { dx, dz };
  conn.reducers.set_input(dx, dz);
}

function startLoops() {
  if (loopsStarted) return;
  loopsStarted = true;

  const animate = () => {
    drawWorld();
    requestAnimationFrame(animate);
  };
  requestAnimationFrame(animate);

  setInterval(() => {
    renderOverlay();
    pumpInput();
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

  const settings = loadSettings();
  const token = settings.token || "";

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
      connectedIdentityHex = identity.toHexString();
      setConnectedUI(true);
      setBanner(`Connected as ${connectedIdentityHex}`);
      saveSettings({ host, db, characterId, displayName, token: newToken });

      // Subscribe to all public tables and join.
      c.subscriptionBuilder().onApplied(() => {
        renderOverlay();
      }).subscribeToAllTables();

      c.reducers.join_world(characterId, displayName);
      startLoops();
    })
    .onConnectError((_ctx, err) => {
      console.error(err);
      conn = null;
      connectedIdentityHex = null;
      setConnectedUI(false);
      setBanner(`Connect error: ${String(err)}`);
      renderOverlay();
    })
    .onDisconnect((_ctx, err) => {
      console.warn("Disconnected", err);
      conn = null;
      connectedIdentityHex = null;
      setConnectedUI(false);
      setBanner("Disconnected");
      renderOverlay();
    })
    .build();
}

function disconnect() {
  if (conn) conn.disconnect();
}

function initUI() {
  const s = loadSettings();
  elHost.value = s.host;
  elDb.value = s.db;
  elCharacterId.value = s.characterId;
  elDisplayName.value = s.displayName;

  elConnect.addEventListener("click", connect);
  elDisconnect.addEventListener("click", disconnect);

  window.addEventListener("resize", () => {
    resizeCanvas();
  });
  resizeCanvas();

  window.addEventListener("keydown", (e) => {
    if (e.code === "KeyE") {
      conn?.reducers?.interact?.();
      e.preventDefault();
      return;
    }
    if (e.code === "KeyR") {
      conn?.reducers?.respawn?.();
      e.preventDefault();
      return;
    }
    keys.add(e.code);
    pumpInput();
  });
  window.addEventListener("keyup", (e) => {
    keys.delete(e.code);
    pumpInput();
  });

  renderOverlay();
}

initUI();
