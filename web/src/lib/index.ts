// Library exports

// Components
export { default as PageFrame } from './components/layout/PageFrame.svelte';
export { default as Header } from './components/layout/Header.svelte';
export { default as TabNav } from './components/layout/TabNav.svelte';
export { default as ChatBar } from './components/layout/ChatBar.svelte';
export { default as PortraitMedallion } from './components/characters/PortraitMedallion.svelte';
export { default as StatsPanel } from './components/settlement/StatsPanel.svelte';

// Stores
export * from './stores';

// SpacetimeDB - explicit exports to avoid conflicts
export {
	// Types
	type User,
	type Account,
	type OidcIdentity,
	type Character,
	type CharacterRace,
	type CharacterSkill,
	type SkillCategory,
	type CharacterBodyPart,
	type BodyPartType,
	type CharacterAttribute,
	type AttributeType,
	type Settlement,
	type Quest,
	type QuestStatus,
	type EnemyType,
	type Party,
	type PartyStatus,
	type PartyMember,
	type TacticalServer,
	type TacticalStatus,
	type InventoryItem,
	type ConnectionState,
	type ConnectionStatus,
	// Client functions
	initConnection,
	disconnect,
	getConnectionState,
	getConnection,
	onConnectionChange,
	createTableSubscription,
	callReducer,
	config
} from './spacetimedb';

// Reducers
export * from './spacetimedb/reducers';
