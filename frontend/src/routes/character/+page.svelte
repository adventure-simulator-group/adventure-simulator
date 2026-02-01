<script lang="ts">
	import { onMount } from 'svelte';
	import PageFrame from '$lib/components/layout/PageFrame.svelte';
	import CharacterSheet from '$lib/components/character/CharacterSheet.svelte';
	import { characters, currentCharacter, currentSkills, currentBodyParts, currentAttributes } from '$lib/stores/character';
	import type { Character, CharacterSkill, CharacterBodyPart, CharacterAttribute } from '$lib/spacetimedb/types';

	// Mock data for development
	const mockCharacter: Character = {
		id: 1n,
		ownerIdentity: 'mock-identity',
		name: 'Marco Lombardi',
		race: 'Human',
		isImmortal: false,
		ageDays: 8760, // ~24 years
		favorInvested: 50n,
		currentSettlementId: 1n,
		partyId: 1n,
		gold: 1250n,
		xp: 2450n,
		level: 5,
		createdAt: BigInt(Date.now()),
		updatedAt: BigInt(Date.now())
	};

	const mockBodyParts: CharacterBodyPart[] = [
		{ id: 1n, characterId: 1n, partType: 'Head', maxHealth: 100, currentHealth: 95, bandagedDamage: 0, scarredDamage: 5 },
		{ id: 2n, characterId: 1n, partType: 'Chest', maxHealth: 150, currentHealth: 130, bandagedDamage: 10, scarredDamage: 10 },
		{ id: 3n, characterId: 1n, partType: 'Stomach', maxHealth: 100, currentHealth: 100, bandagedDamage: 0, scarredDamage: 0 },
		{ id: 4n, characterId: 1n, partType: 'LeftArm', maxHealth: 80, currentHealth: 60, bandagedDamage: 15, scarredDamage: 5 },
		{ id: 5n, characterId: 1n, partType: 'RightArm', maxHealth: 80, currentHealth: 75, bandagedDamage: 5, scarredDamage: 0 },
		{ id: 6n, characterId: 1n, partType: 'LeftLeg', maxHealth: 100, currentHealth: 85, bandagedDamage: 10, scarredDamage: 5 },
		{ id: 7n, characterId: 1n, partType: 'RightLeg', maxHealth: 100, currentHealth: 100, bandagedDamage: 0, scarredDamage: 0 }
	];

	const mockSkills: CharacterSkill[] = [
		{ id: 1n, characterId: 1n, skill: 'Swordsmanship', category: 'Combat', hoursTrained: 2500, trainingAllocation: 20 },
		{ id: 2n, characterId: 1n, skill: 'Shield Block', category: 'Combat', hoursTrained: 1800, trainingAllocation: 15 },
		{ id: 3n, characterId: 1n, skill: 'Archery', category: 'Combat', hoursTrained: 500, trainingAllocation: 5 },
		{ id: 4n, characterId: 1n, skill: 'Athletics', category: 'Physical', hoursTrained: 1200, trainingAllocation: 10 },
		{ id: 5n, characterId: 1n, skill: 'Stealth', category: 'Physical', hoursTrained: 300, trainingAllocation: 0 },
		{ id: 6n, characterId: 1n, skill: 'First Aid', category: 'Knowledge', hoursTrained: 400, trainingAllocation: 10 },
		{ id: 7n, characterId: 1n, skill: 'Persuasion', category: 'Social', hoursTrained: 200, trainingAllocation: 5 },
		{ id: 8n, characterId: 1n, skill: 'Intimidation', category: 'Social', hoursTrained: 600, trainingAllocation: 5 }
	];

	const mockAttributes: CharacterAttribute[] = [
		{ id: 1n, characterId: 1n, attributeType: 'Endurance', limb: null, geneticMax: 5, currentValue: 4 },
		{ id: 2n, characterId: 1n, attributeType: 'Intelligence', limb: null, geneticMax: 4, currentValue: 3 },
		{ id: 3n, characterId: 1n, attributeType: 'Strength', limb: 'left_arm', geneticMax: 5, currentValue: 4 },
		{ id: 4n, characterId: 1n, attributeType: 'Strength', limb: 'right_arm', geneticMax: 5, currentValue: 5 },
		{ id: 5n, characterId: 1n, attributeType: 'Agility', limb: 'left_leg', geneticMax: 4, currentValue: 3 },
		{ id: 6n, characterId: 1n, attributeType: 'Agility', limb: 'right_leg', geneticMax: 4, currentValue: 4 }
	];

	const mockIncapacitation = {
		imbalance: 0.05,
		exhaustion: 0.1,
		pain: 0.15,
		bloodLoss: 0.08,
		fear: 0,
		fatigue: 0.12
	};

	// Initialize with mock data
	onMount(() => {
		characters.setCurrent(mockCharacter);
		characters.setSkills(mockSkills);
		characters.setBodyParts(mockBodyParts);
		characters.setAttributes(mockAttributes);
	});
</script>

<svelte:head>
	<title>{$currentCharacter?.name ?? 'Character'} - Adventure Simulator</title>
</svelte:head>

<PageFrame variant="full">
	<div class="character-page">
		{#if $currentCharacter}
			<CharacterSheet
				character={$currentCharacter}
				bodyParts={$currentBodyParts}
				skills={$currentSkills}
				attributes={$currentAttributes}
				incapacitation={mockIncapacitation}
			/>
		{:else}
			<div class="loading">
				<p>Loading character...</p>
			</div>
		{/if}
	</div>
</PageFrame>

<style>
	.character-page {
		padding: var(--space-4);
	}

	.loading {
		display: flex;
		justify-content: center;
		align-items: center;
		min-height: 400px;
		color: var(--ink-faded);
		font-family: var(--font-stats);
		font-style: italic;
	}
</style>
