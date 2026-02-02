/**
 * SpacetimeDB Reducer Wrappers
 *
 * These functions provide a typed interface for calling server reducers.
 * They match the reducers defined in crates/strategic-db/src/lib.rs
 */

import { callReducer } from './client';
import type { CharacterRace } from './types';

// ============================================================================
// Auth Reducers
// ============================================================================

/**
 * Link a WorkOS identity to an account
 * Creates a new account if one doesn't exist for this identity
 */
export async function linkAccount(workosUserId: string, email: string): Promise<boolean> {
	const result = await callReducer('link_account', [workosUserId, email]);
	return result.success;
}

/**
 * Unlink WorkOS identity from account (for account recovery)
 */
export async function unlinkAccount(): Promise<boolean> {
	const result = await callReducer('unlink_account', []);
	return result.success;
}

/**
 * Set username for the current account
 */
export async function setUsername(username: string): Promise<boolean> {
	const result = await callReducer('set_username', [username]);
	return result.success;
}

// ============================================================================
// Character Reducers
// ============================================================================

/**
 * Create a new character
 */
export async function createCharacter(name: string, race: CharacterRace): Promise<boolean> {
	const result = await callReducer('create_character', [name, race]);
	return result.success;
}

/**
 * Set training allocation for a skill
 * Allocation is 0-100, representing percentage of training time
 */
export async function setTrainingAllocation(
	characterId: bigint,
	skill: string,
	allocation: number
): Promise<boolean> {
	const result = await callReducer('set_training_allocation', [
		characterId.toString(),
		skill,
		allocation
	]);
	return result.success;
}

// ============================================================================
// Party Reducers
// ============================================================================

/**
 * Create a new party with the specified character as leader
 */
export async function createParty(characterId: bigint): Promise<boolean> {
	const result = await callReducer('create_party', [characterId.toString()]);
	return result.success;
}

/**
 * Join an existing party
 */
export async function joinParty(characterId: bigint, partyId: bigint): Promise<boolean> {
	const result = await callReducer('join_party', [characterId.toString(), partyId.toString()]);
	return result.success;
}

/**
 * Leave current party
 */
export async function leaveParty(characterId: bigint): Promise<boolean> {
	const result = await callReducer('leave_party', [characterId.toString()]);
	return result.success;
}

// ============================================================================
// Quest Reducers
// ============================================================================

/**
 * Accept a quest for a party
 */
export async function acceptQuest(partyId: bigint, questId: bigint): Promise<boolean> {
	const result = await callReducer('accept_quest', [partyId.toString(), questId.toString()]);
	return result.success;
}

// ============================================================================
// Mission Reducers
// ============================================================================

/**
 * Enter a tactical mission (triggers server spawning)
 */
export async function enterMission(partyId: bigint): Promise<boolean> {
	const result = await callReducer('enter_mission', [partyId.toString()]);
	return result.success;
}

/**
 * Called by tactical server when ready to accept connections
 */
export async function tacticalServerReady(
	missionId: string,
	addr: string,
	certDigest: string
): Promise<boolean> {
	const result = await callReducer('tactical_server_ready', [missionId, addr, certDigest]);
	return result.success;
}

/**
 * Commit mission results (success or failure)
 */
export async function commitMission(
	missionId: string,
	success: boolean,
	xpGained: bigint,
	goldGained: bigint
): Promise<boolean> {
	const result = await callReducer('commit_mission', [
		missionId,
		success,
		xpGained.toString(),
		goldGained.toString()
	]);
	return result.success;
}

/**
 * Leave a mission early (abandon)
 */
export async function leaveMission(missionId: string): Promise<boolean> {
	const result = await callReducer('leave_mission', [missionId]);
	return result.success;
}

// ============================================================================
// Admin/Init Reducers
// ============================================================================

/**
 * Initialize the world with seed data (admin only)
 */
export async function initWorld(): Promise<boolean> {
	const result = await callReducer('init', []);
	return result.success;
}

// ============================================================================
// Chat Reducers
// ============================================================================

/**
 * Send a chat message
 */
export async function sendChatMessage(
	senderId: string,
	content: string,
	channel: string
): Promise<boolean> {
	const result = await callReducer('send_chat_message', [senderId, content, channel]);
	return result.success;
}
