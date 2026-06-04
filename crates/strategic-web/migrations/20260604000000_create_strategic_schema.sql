CREATE TABLE characters (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    xp INTEGER NOT NULL DEFAULT 0,
    level INTEGER NOT NULL DEFAULT 1,
    gold INTEGER NOT NULL DEFAULT 100,
    current_settlement_id TEXT,
    party_id TEXT,
    active_mission_id TEXT,
    in_mission INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE character_attributes (
    character_id INTEGER PRIMARY KEY REFERENCES characters(id) ON DELETE CASCADE,
    endurance REAL NOT NULL,
    immunity REAL NOT NULL,
    gut REAL NOT NULL,
    strength REAL NOT NULL,
    precision REAL NOT NULL,
    agility REAL NOT NULL,
    intelligence REAL NOT NULL,
    instinct REAL NOT NULL,
    eyesight REAL NOT NULL,
    hearing REAL NOT NULL
);

CREATE TABLE character_stats (
    character_id INTEGER PRIMARY KEY REFERENCES characters(id) ON DELETE CASCADE,
    calories_used REAL NOT NULL,
    focus REAL NOT NULL
);

CREATE TABLE character_skills (
    character_id INTEGER PRIMARY KEY REFERENCES characters(id) ON DELETE CASCADE,
    melee_hours REAL NOT NULL,
    dodge_hours REAL NOT NULL,
    block_hours REAL NOT NULL
);

CREATE TABLE character_limbs (
    character_id INTEGER PRIMARY KEY REFERENCES characters(id) ON DELETE CASCADE,
    left_arm REAL NOT NULL,
    right_arm REAL NOT NULL,
    left_leg REAL NOT NULL,
    right_leg REAL NOT NULL,
    head REAL NOT NULL,
    chest REAL NOT NULL,
    stomach REAL NOT NULL
);

CREATE TABLE character_equip (
    character_id INTEGER PRIMARY KEY REFERENCES characters(id) ON DELETE CASCADE,
    left_hand_item_id INTEGER,
    right_hand_item_id INTEGER,
    left_arm_armor_id INTEGER,
    right_arm_armor_id INTEGER,
    left_leg_armor_id INTEGER,
    right_leg_armor_id INTEGER,
    head_armor_id INTEGER,
    chest_armor_id INTEGER,
    stomach_armor_id INTEGER
);

CREATE TABLE items (
    id TEXT PRIMARY KEY,
    weight REAL NOT NULL,
    slot TEXT NOT NULL DEFAULT 'None',
    kind TEXT NOT NULL DEFAULT 'Simple',
    accuracy REAL NOT NULL DEFAULT 0,
    block REAL NOT NULL DEFAULT 0,
    dodge REAL NOT NULL DEFAULT 0,
    coverage REAL NOT NULL DEFAULT 0
);

CREATE TABLE inventory_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    character_id INTEGER NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    item_id TEXT NOT NULL REFERENCES items(id),
    quantity INTEGER NOT NULL
);

CREATE INDEX inventory_items_character_idx ON inventory_items(character_id);
CREATE INDEX inventory_items_character_item_idx ON inventory_items(character_id, item_id);

CREATE TABLE settlements (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    coord_x REAL NOT NULL,
    coord_y REAL NOT NULL,
    population_level INTEGER NOT NULL,
    scene_key TEXT NOT NULL
);

CREATE TABLE quests (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    difficulty INTEGER NOT NULL,
    gold_reward INTEGER NOT NULL,
    xp_reward INTEGER NOT NULL,
    settlement_id TEXT NOT NULL REFERENCES settlements(id),
    status TEXT NOT NULL,
    accepted_by TEXT,
    enemy_type TEXT NOT NULL,
    enemy_count INTEGER NOT NULL
);

CREATE INDEX quests_settlement_idx ON quests(settlement_id);

CREATE TABLE parties (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    leader_id INTEGER NOT NULL REFERENCES characters(id),
    current_settlement_id TEXT REFERENCES settlements(id),
    active_quest_id TEXT REFERENCES quests(id)
);

CREATE TABLE party_members (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    party_id TEXT NOT NULL REFERENCES parties(id) ON DELETE CASCADE,
    character_id INTEGER NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    role TEXT,
    UNIQUE(party_id, character_id)
);

CREATE INDEX party_members_party_idx ON party_members(party_id);
CREATE INDEX party_members_character_idx ON party_members(character_id);

CREATE TABLE missions (
    id TEXT PRIMARY KEY,
    scene_key TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('requested', 'starting', 'ready', 'ended', 'failed', 'cancelled')),
    party_id TEXT REFERENCES parties(id),
    quest_id TEXT REFERENCES quests(id),
    requester_character_id INTEGER REFERENCES characters(id),
    addr TEXT,
    cert_digest TEXT,
    pid INTEGER,
    success INTEGER,
    xp_gained INTEGER NOT NULL DEFAULT 0,
    result_committed INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    ready_at TEXT,
    ended_at TEXT
);

CREATE INDEX missions_party_idx ON missions(party_id);
CREATE INDEX missions_quest_idx ON missions(quest_id);
CREATE INDEX missions_status_idx ON missions(status);
