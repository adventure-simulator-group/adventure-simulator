// SpacetimeDB Types for Adventure Simulator
// These types mirror the backend database schema
// Note: After running `spacetime generate`, these will be auto-generated

// ============================================================================
// Auth Types
// ============================================================================

export interface User {
	identity: string;
	accountId: bigint | null;
	displayName: string;
	online: boolean;
	createdAt: bigint;
	lastSeen: bigint;
}

export interface Account {
	id: bigint;
	username: string | null;
	usernameLower: string | null;
	createdAt: bigint;
	updatedAt: bigint;
}

export interface OidcIdentity {
	id: bigint;
	workosUserId: string;
	accountId: bigint;
	email: string;
	linkedAt: bigint;
}

// ============================================================================
// Character Types
// ============================================================================

export type CharacterRace =
	| 'Human'
	| 'Elf'
	| 'Dwarf'
	| 'Halfling'
	| 'Orc'
	| 'Goblin'
	| 'Ratling';

export interface Character {
	id: bigint;
	ownerIdentity: string;
	name: string;
	race: CharacterRace;
	isImmortal: boolean;
	ageDays: number;
	favorInvested: bigint;
	currentSettlementId: bigint | null;
	partyId: bigint | null;
	gold: bigint;
	xp: bigint;
	level: number;
	createdAt: bigint;
	updatedAt: bigint;
}

export type SkillCategory =
	| 'Physical'
	| 'Combat'
	| 'Survival'
	| 'Social'
	| 'Knowledge'
	| 'Crafting'
	| 'Magic';

export interface CharacterSkill {
	id: bigint;
	characterId: bigint;
	skill: string;
	category: SkillCategory;
	hoursTrained: number;
	trainingAllocation: number;
}

export type BodyPartType =
	| 'Head'
	| 'Chest'
	| 'Stomach'
	| 'LeftArm'
	| 'RightArm'
	| 'LeftLeg'
	| 'RightLeg';

export interface CharacterBodyPart {
	id: bigint;
	characterId: bigint;
	partType: BodyPartType;
	maxHealth: number;
	currentHealth: number;
	bandagedDamage: number;
	scarredDamage: number;
}

export type AttributeType =
	| 'Endurance'
	| 'Immunity'
	| 'Gut'
	| 'Strength'
	| 'Agility'
	| 'Intelligence'
	| 'Instinct'
	| 'Eyesight'
	| 'Hearing';

export interface CharacterAttribute {
	id: bigint;
	characterId: bigint;
	attributeType: AttributeType;
	limb: string | null;
	geneticMax: number;
	currentValue: number;
}

// ============================================================================
// Strategic Types
// ============================================================================

export interface Settlement {
	id: bigint;
	name: string;
	coordX: number;
	coordY: number;
	populationLevel: number;
	sceneKey: string;
}

export type QuestStatus = 'Available' | 'Accepted' | 'InProgress' | 'Completed' | 'Failed';
export type EnemyType = 'Bandits' | 'Wolves' | 'Goblins' | 'Undead' | 'Orcs' | 'Beasts';

export interface Quest {
	id: bigint;
	settlementId: bigint;
	title: string;
	description: string;
	difficulty: number;
	recommendedLevel: number;
	rewardGold: bigint;
	rewardXp: bigint;
	rewardFavor: bigint;
	enemyType: EnemyType;
	enemyCount: number;
	status: QuestStatus;
	createdAt: bigint;
}

export type PartyStatus = 'Idle' | 'Traveling' | 'InMission' | 'Resting';

export interface Party {
	id: bigint;
	leaderId: bigint;
	questId: bigint | null;
	status: PartyStatus;
	settlementId: bigint | null;
	createdAt: bigint;
}

export interface PartyMember {
	id: bigint;
	partyId: bigint;
	characterId: bigint;
	joinedAt: bigint;
}

// ============================================================================
// Tactical Types
// ============================================================================

export type TacticalStatus = 'Pending' | 'Ready' | 'Ended';

export interface TacticalServer {
	missionId: string;
	sceneKey: string;
	status: TacticalStatus;
	addr: string;
	certDigest: string;
	partyId: bigint;
}

// ============================================================================
// Inventory Types
// ============================================================================

export interface InventoryItem {
	id: bigint;
	characterId: bigint;
	itemKey: string;
	quantity: number;
	equipped: boolean;
}

// ============================================================================
// Chat Types
// ============================================================================

export interface ChatMessage {
	id: string;
	senderId: string;
	senderName: string;
	content: string;
	timestamp: number;
	channel: ChatChannel;
}

export type ChatChannel = 'global' | 'party' | 'local' | 'whisper';

// ============================================================================
// Time Types
// ============================================================================

export interface GameTime {
	day: number;
	month: string;
	year: number;
	hour: number;
	minute: number;
}

// ============================================================================
// Connection Types
// ============================================================================

export type ConnectionStatus = 'disconnected' | 'connecting' | 'connected' | 'error';

export interface ConnectionState {
	status: ConnectionStatus;
	identity: string | null;
	error: string | null;
}
