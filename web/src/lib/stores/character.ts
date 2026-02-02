// Character State Store
import { writable, derived } from 'svelte/store';
import type {
	Character,
	CharacterSkill,
	CharacterBodyPart,
	CharacterAttribute
} from '$lib/spacetimedb/types';

interface CharacterState {
	current: Character | null;
	all: Character[];
	skills: CharacterSkill[];
	bodyParts: CharacterBodyPart[];
	attributes: CharacterAttribute[];
	loading: boolean;
	error: string | null;
}

function createCharacterStore() {
	const { subscribe, set, update } = writable<CharacterState>({
		current: null,
		all: [],
		skills: [],
		bodyParts: [],
		attributes: [],
		loading: true,
		error: null
	});

	return {
		subscribe,

		// Set the currently selected character
		setCurrent: (character: Character | null) =>
			update((s) => ({ ...s, current: character })),

		// Set all owned characters
		setAll: (characters: Character[]) =>
			update((s) => ({ ...s, all: characters, loading: false })),

		// Add a character to the list
		addCharacter: (character: Character) =>
			update((s) => ({
				...s,
				all: [...s.all.filter((c) => c.id !== character.id), character]
			})),

		// Update a character
		updateCharacter: (id: bigint, changes: Partial<Character>) =>
			update((s) => ({
				...s,
				current: s.current?.id === id ? { ...s.current, ...changes } : s.current,
				all: s.all.map((c) => (c.id === id ? { ...c, ...changes } : c))
			})),

		// Remove a character
		removeCharacter: (id: bigint) =>
			update((s) => ({
				...s,
				current: s.current?.id === id ? null : s.current,
				all: s.all.filter((c) => c.id !== id)
			})),

		// Set character skills
		setSkills: (skills: CharacterSkill[]) =>
			update((s) => ({ ...s, skills })),

		// Update a skill
		updateSkill: (id: bigint, changes: Partial<CharacterSkill>) =>
			update((s) => ({
				...s,
				skills: s.skills.map((sk) => (sk.id === id ? { ...sk, ...changes } : sk))
			})),

		// Set body parts
		setBodyParts: (bodyParts: CharacterBodyPart[]) =>
			update((s) => ({ ...s, bodyParts })),

		// Update body part
		updateBodyPart: (id: bigint, changes: Partial<CharacterBodyPart>) =>
			update((s) => ({
				...s,
				bodyParts: s.bodyParts.map((bp) => (bp.id === id ? { ...bp, ...changes } : bp))
			})),

		// Set attributes
		setAttributes: (attributes: CharacterAttribute[]) =>
			update((s) => ({ ...s, attributes })),

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
				skills: [],
				bodyParts: [],
				attributes: [],
				loading: true,
				error: null
			})
	};
}

export const characters = createCharacterStore();

// Derived stores
export const currentCharacter = derived(characters, ($chars) => $chars.current);

export const ownedCharacters = derived(characters, ($chars) => $chars.all);

export const characterCount = derived(characters, ($chars) => $chars.all.length);

export const isCharacterLoading = derived(characters, ($chars) => $chars.loading);

// Current character's skills
export const currentSkills = derived(characters, ($chars) => {
	if (!$chars.current) return [];
	return $chars.skills.filter((s) => s.characterId === $chars.current!.id);
});

// Current character's body parts
export const currentBodyParts = derived(characters, ($chars) => {
	if (!$chars.current) return [];
	return $chars.bodyParts.filter((bp) => bp.characterId === $chars.current!.id);
});

// Current character's attributes
export const currentAttributes = derived(characters, ($chars) => {
	if (!$chars.current) return [];
	return $chars.attributes.filter((a) => a.characterId === $chars.current!.id);
});

// Total health percentage
export const totalHealthPercent = derived(characters, ($chars) => {
	if (!$chars.current) return 100;
	const parts = $chars.bodyParts.filter((bp) => bp.characterId === $chars.current!.id);
	if (parts.length === 0) return 100;

	const totalMax = parts.reduce((sum, p) => sum + p.maxHealth, 0);
	const totalCurrent = parts.reduce((sum, p) => sum + p.currentHealth, 0);
	return totalMax > 0 ? Math.round((totalCurrent / totalMax) * 100) : 100;
});
