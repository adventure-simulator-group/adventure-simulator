// Settlement State Store
import { writable, derived } from 'svelte/store';
import type { Settlement, Quest, Party, PartyMember } from '$lib/spacetimedb/types';

interface SettlementState {
	current: Settlement | null;
	all: Settlement[];
	quests: Quest[];
	parties: Party[];
	partyMembers: PartyMember[];
	loading: boolean;
	error: string | null;
}

function createSettlementStore() {
	const { subscribe, set, update } = writable<SettlementState>({
		current: null,
		all: [],
		quests: [],
		parties: [],
		partyMembers: [],
		loading: true,
		error: null
	});

	return {
		subscribe,

		// Set the current settlement
		setCurrent: (settlement: Settlement | null) =>
			update((s) => ({ ...s, current: settlement })),

		// Set all settlements
		setAll: (settlements: Settlement[]) =>
			update((s) => ({ ...s, all: settlements, loading: false })),

		// Update current settlement
		updateCurrent: (changes: Partial<Settlement>) =>
			update((s) => ({
				...s,
				current: s.current ? { ...s.current, ...changes } : null
			})),

		// Set quests
		setQuests: (quests: Quest[]) =>
			update((s) => ({ ...s, quests })),

		// Add a quest
		addQuest: (quest: Quest) =>
			update((s) => ({
				...s,
				quests: [...s.quests.filter((q) => q.id !== quest.id), quest]
			})),

		// Update a quest
		updateQuest: (id: bigint, changes: Partial<Quest>) =>
			update((s) => ({
				...s,
				quests: s.quests.map((q) => (q.id === id ? { ...q, ...changes } : q))
			})),

		// Remove a quest
		removeQuest: (id: bigint) =>
			update((s) => ({
				...s,
				quests: s.quests.filter((q) => q.id !== id)
			})),

		// Set parties
		setParties: (parties: Party[]) =>
			update((s) => ({ ...s, parties })),

		// Set party members
		setPartyMembers: (partyMembers: PartyMember[]) =>
			update((s) => ({ ...s, partyMembers })),

		// Set loading state
		setLoading: (loading: boolean) =>
			update((s) => ({ ...s, loading })),

		// Set error
		setError: (error: string | null) =>
			update((s) => ({ ...s, error })),

		// Reset store
		reset: () =>
			set({
				current: null,
				all: [],
				quests: [],
				parties: [],
				partyMembers: [],
				loading: true,
				error: null
			})
	};
}

export const settlement = createSettlementStore();

// Derived stores
export const currentSettlement = derived(settlement, ($s) => $s.current);

export const allSettlements = derived(settlement, ($s) => $s.all);

export const settlementName = derived(settlement, ($s) => $s.current?.name ?? '');

export const settlementQuests = derived(settlement, ($s) => $s.quests);

export const availableQuests = derived(settlement, ($s) =>
	$s.quests.filter((q) => q.status === 'Available')
);

export const currentSettlementQuests = derived(settlement, ($s) => {
	if (!$s.current) return [];
	return $s.quests.filter((q) => q.settlementId === $s.current!.id);
});

export const isSettlementLoading = derived(settlement, ($s) => $s.loading);

// Party-related derived stores
export const allParties = derived(settlement, ($s) => $s.parties);

export const settlementParties = derived(settlement, ($s) => {
	if (!$s.current) return [];
	return $s.parties.filter((p) => p.settlementId === $s.current!.id);
});
