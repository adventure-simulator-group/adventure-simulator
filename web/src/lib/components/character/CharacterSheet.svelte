<script lang="ts">
	import '$lib/styles/compat.css';
	import BodyPartDiagram from './BodyPartDiagram.svelte';
	import SkillsPanel from './SkillsPanel.svelte';
	import HealthWheel from './HealthWheel.svelte';
	import type { Character, CharacterSkill, CharacterBodyPart, CharacterAttribute } from '$lib/spacetimedb/types';

	interface Incapacitation {
		imbalance: number;
		exhaustion: number;
		pain: number;
		bloodLoss: number;
		fear: number;
		fatigue: number;
	}

	interface Props {
		character: Character;
		bodyParts: CharacterBodyPart[];
		skills: CharacterSkill[];
		attributes: CharacterAttribute[];
		incapacitation?: Incapacitation;
	}

	let { character, bodyParts, skills, attributes, incapacitation = {
		imbalance: 0,
		exhaustion: 0,
		pain: 0,
		bloodLoss: 0,
		fear: 0,
		fatigue: 0
	} }: Props = $props();

	const totalIncapacitation = $derived(
		incapacitation.imbalance +
			incapacitation.exhaustion +
			incapacitation.pain +
			incapacitation.bloodLoss +
			incapacitation.fear +
			incapacitation.fatigue
	);

	const ageYears = $derived(Math.floor(character.ageDays / 365));

	function getRaceIcon(race: string): string {
		const icons: Record<string, string> = {
			Human: '👤',
			Orc: '👹',
			Goblin: '👺',
			Dwarf: '🧔',
			Halfling: '🧒',
			Ratling: '🐀',
			Elf: '🧝'
		};
		return icons[race] || '👤';
	}

	function getAttributesForPart(partType: string): CharacterAttribute[] {
		// Some attributes are limb-specific
		const limbMap: Record<string, string> = {
			LeftArm: 'left_arm',
			RightArm: 'right_arm',
			LeftLeg: 'left_leg',
			RightLeg: 'right_leg'
		};

		const limb = limbMap[partType];
		if (limb) {
			return attributes.filter((a) => a.limb === limb);
		}

		// Core body parts get non-limb attributes
		return attributes.filter((a) => !a.limb);
	}
</script>

<div class="character-sheet">
	<!-- Header Section -->
	<section class="header-section panel">
		<div class="character-portrait">
			<div class="portrait-placeholder">
				<span class="race-icon">{getRaceIcon(character.race)}</span>
			</div>
		</div>

		<div class="character-info">
			<h2 class="character-name">{character.name}</h2>
			<div class="character-details">
				<span class="badge">{character.race}</span>
				{#if character.isImmortal}
					<span class="badge badge-gold">Immortal</span>
				{:else}
					<span class="badge">Age {ageYears}</span>
				{/if}
				<span class="badge">Level {character.level}</span>
			</div>
		</div>

		<div class="character-stats">
			<div class="stat">
				<span class="stat-value">{character.xp.toString()}</span>
				<span class="stat-label">XP</span>
			</div>
			<div class="stat">
				<span class="stat-value">{character.gold.toString()}</span>
				<span class="stat-label">Gold</span>
			</div>
			<div class="stat">
				<span class="stat-value">{character.favorInvested.toString()}</span>
				<span class="stat-label">Favor</span>
			</div>
		</div>
	</section>

	<!-- Main Content Grid -->
	<div class="main-grid">
		<!-- Left Column: Body & Health -->
		<div class="body-column">
			<section class="panel">
				<h3 class="panel-title">Body Condition</h3>
				<div class="body-health-container">
					<BodyPartDiagram {bodyParts} />
					<HealthWheel {incapacitation} />
				</div>
				<div class="incap-summary">
					<span class="incap-label">Total Incapacitation:</span>
					<span
						class="incap-value"
						class:warning={totalIncapacitation > 0.5}
						class:danger={totalIncapacitation > 0.8}
					>
						{Math.round(totalIncapacitation * 100)}%
					</span>
				</div>
			</section>
		</div>

		<!-- Right Column: Skills -->
		<div class="skills-column">
			<section class="panel">
				<h3 class="panel-title">Skills</h3>
				<SkillsPanel {skills} />
			</section>
		</div>
	</div>

	<!-- Attributes Summary -->
	<section class="attributes-section panel">
		<h3 class="panel-title">Attributes by Body Part</h3>
		<div class="attributes-grid">
			{#each bodyParts as part}
				{@const partAttributes = getAttributesForPart(part.partType)}
				{@const healthRatio = part.maxHealth > 0 ? part.currentHealth / part.maxHealth : 1}
				<div class="attribute-card">
					<h4>{part.partType.replace(/([A-Z])/g, ' $1').trim()}</h4>
					<div class="health-bar">
						<div
							class="health-fill"
							style="width: {healthRatio * 100}%"
							class:healthy={healthRatio > 0.7}
							class:injured={healthRatio <= 0.7 && healthRatio > 0.3}
							class:critical={healthRatio <= 0.3}
						></div>
					</div>
					<div class="attribute-list">
						{#each partAttributes as attr}
							<div class="attribute-row">
								<span class="attr-name">{attr.attributeType}</span>
								<span class="attr-value">{attr.currentValue}/{attr.geneticMax}</span>
							</div>
						{/each}
					</div>
				</div>
			{/each}
		</div>
	</section>
</div>

<style>
	.character-sheet {
		display: flex;
		flex-direction: column;
		gap: var(--space-6);
	}

	.panel {
		padding: var(--space-4);
		background: var(--parchment-light);
		border: 1px solid var(--parchment-shadow);
		border-radius: var(--radius-md);
	}

	.panel-title {
		font-family: var(--font-display);
		font-size: var(--text-lg);
		font-weight: var(--font-semibold);
		color: var(--ink-dark);
		margin-bottom: var(--space-4);
		text-transform: uppercase;
		letter-spacing: var(--tracking-wider);
	}

	/* Header Section */
	.header-section {
		display: flex;
		align-items: center;
		gap: var(--space-6);
		flex-wrap: wrap;
	}

	.character-portrait {
		flex-shrink: 0;
	}

	.portrait-placeholder {
		width: 100px;
		height: 100px;
		border-radius: var(--radius-lg);
		background: var(--parchment-dark);
		border: 2px solid var(--ornament-dark);
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.race-icon {
		font-size: 48px;
	}

	.character-info {
		flex: 1;
		min-width: 200px;
	}

	.character-name {
		font-family: var(--font-display);
		font-size: var(--text-2xl);
		font-weight: var(--font-bold);
		color: var(--ink-black);
		margin-bottom: var(--space-2);
	}

	.character-details {
		display: flex;
		gap: var(--space-2);
		flex-wrap: wrap;
	}

	.badge {
		display: inline-block;
		padding: var(--space-1) var(--space-3);
		background: var(--parchment-dark);
		border: 1px solid var(--parchment-shadow);
		border-radius: var(--radius-sm);
		font-family: var(--font-stats);
		font-size: var(--text-sm);
		color: var(--ink-brown);
	}

	.badge-gold {
		background: var(--ornament-gold-muted);
		border-color: var(--ornament-gold);
		color: var(--ink-black);
	}

	.character-stats {
		display: flex;
		gap: var(--space-8);
	}

	.stat {
		text-align: center;
	}

	.stat-value {
		display: block;
		font-family: var(--font-stats);
		font-size: var(--text-xl);
		font-weight: var(--font-bold);
		color: var(--ink-dark);
	}

	.stat-label {
		font-family: var(--font-stats);
		font-size: var(--text-xs);
		color: var(--ink-faded);
		text-transform: uppercase;
	}

	/* Main Grid */
	.main-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: var(--space-6);
	}

	@media (max-width: 900px) {
		.main-grid {
			grid-template-columns: 1fr;
		}
	}

	.body-health-container {
		display: flex;
		gap: var(--space-6);
		align-items: center;
		justify-content: center;
		flex-wrap: wrap;
		margin-bottom: var(--space-4);
	}

	.incap-summary {
		display: flex;
		justify-content: center;
		gap: var(--space-2);
		padding-top: var(--space-4);
		border-top: 1px solid var(--parchment-shadow);
		font-family: var(--font-stats);
	}

	.incap-label {
		color: var(--ink-brown);
	}

	.incap-value {
		font-weight: var(--font-bold);
		color: var(--ornament-green);
	}

	.incap-value.warning {
		color: var(--ornament-gold);
	}

	.incap-value.danger {
		color: var(--ornament-red);
	}

	/* Attributes Section */
	.attributes-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
		gap: var(--space-4);
	}

	.attribute-card {
		padding: var(--space-4);
		background: var(--parchment-base);
		border-radius: var(--radius-md);
		border: 1px solid var(--parchment-shadow);
	}

	.attribute-card h4 {
		font-family: var(--font-display);
		font-size: var(--text-sm);
		margin-bottom: var(--space-2);
		color: var(--ornament-gold);
	}

	.health-bar {
		height: 6px;
		background: var(--parchment-dark);
		border-radius: var(--radius-full);
		margin-bottom: var(--space-3);
		overflow: hidden;
	}

	.health-fill {
		height: 100%;
		border-radius: var(--radius-full);
		transition: width var(--duration-normal);
	}

	.health-fill.healthy {
		background: var(--ornament-green);
	}

	.health-fill.injured {
		background: var(--ornament-gold);
	}

	.health-fill.critical {
		background: var(--ornament-red);
	}

	.attribute-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.attribute-row {
		display: flex;
		justify-content: space-between;
		font-size: var(--text-sm);
		font-family: var(--font-stats);
	}

	.attr-name {
		color: var(--ink-brown);
	}

	.attr-value {
		color: var(--ink-dark);
		font-weight: var(--font-medium);
	}
</style>
