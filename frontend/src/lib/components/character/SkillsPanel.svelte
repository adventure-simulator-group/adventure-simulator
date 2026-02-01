<script lang="ts">
	import '$lib/styles/compat.css';
	import type { CharacterSkill } from '$lib/spacetimedb/types';

	interface Props {
		skills: CharacterSkill[];
	}

	let { skills }: Props = $props();

	type SkillFilter = 'All' | 'Physical' | 'Combat' | 'Social' | 'Knowledge';
	let activeFilter = $state<SkillFilter>('All');

	const MAX_SKILL_LEVEL = 5;
	const HOURS_TO_MAX = 5000; // Hours to reach max skill level

	function calculateSkillLevel(hoursTrained: number): number {
		// Diminishing returns formula
		return Math.min((hoursTrained / (hoursTrained + HOURS_TO_MAX)) * MAX_SKILL_LEVEL, MAX_SKILL_LEVEL);
	}

	function getSkillColor(category: string): string {
		const colors: Record<string, string> = {
			Physical: 'var(--color-skill-balance)',
			Combat: 'var(--color-skill-melee)',
			Survival: 'var(--color-skill-stealth)',
			Social: 'var(--color-skill-charisma)',
			Knowledge: 'var(--color-skill-medicine)',
			Crafting: 'var(--color-skill-surgeon)',
			Magic: 'var(--color-skill-faith)'
		};
		return colors[category] || 'var(--ornament-gold)';
	}

	function getSkillIcon(category: string): string {
		const icons: Record<string, string> = {
			Physical: 'P',
			Combat: 'C',
			Survival: 'S',
			Social: 'O',
			Knowledge: 'K',
			Crafting: 'R',
			Magic: 'M'
		};
		return icons[category] || '?';
	}

	const filteredSkills = $derived(
		activeFilter === 'All'
			? skills
			: skills.filter((s) => s.category === activeFilter)
	);

	const skillCategories = $derived([...new Set(skills.map((s) => s.category))]);

	const totalAllocation = $derived(skills.reduce((sum, s) => sum + s.trainingAllocation, 0));

	function setFilter(filter: SkillFilter) {
		activeFilter = filter;
	}
</script>

<div class="skills-panel">
	<!-- Filter Tabs -->
	<div class="skill-tabs">
		<button
			class="tab"
			class:active={activeFilter === 'All'}
			onclick={() => setFilter('All')}
		>
			All ({skills.length})
		</button>
		{#each skillCategories as category}
			<button
				class="tab"
				class:active={activeFilter === category}
				onclick={() => setFilter(category as SkillFilter)}
			>
				{category}
			</button>
		{/each}
	</div>

	<!-- Skills List -->
	<div class="skills-list">
		{#each filteredSkills as skill}
			{@const level = calculateSkillLevel(skill.hoursTrained)}
			{@const percentage = (level / MAX_SKILL_LEVEL) * 100}
			<div class="skill-row">
				<div class="skill-header">
					<div class="skill-icon" style="background: {getSkillColor(skill.category)}">
						{getSkillIcon(skill.category)}
					</div>
					<span class="skill-name">{skill.skill}</span>
					<span class="skill-category">{skill.category}</span>
				</div>

				<!-- Skill Bar -->
				<div class="skill-bars">
					<div class="bar-container">
						<div class="bar-label">Level</div>
						<div class="skill-bar">
							<div
								class="skill-fill"
								style="width: {percentage}%; background: {getSkillColor(skill.category)}"
							></div>
							<!-- Tick marks -->
							{#each [1, 2, 3, 4] as tick}
								<div class="tick" style="left: {(tick / MAX_SKILL_LEVEL) * 100}%"></div>
							{/each}
						</div>
						<div class="bar-value">{level.toFixed(1)}</div>
					</div>
				</div>

				<!-- Training Stats -->
				<div class="skill-stats">
					<div class="stat">
						<span class="stat-label">Hours</span>
						<span class="stat-value">{skill.hoursTrained.toLocaleString()}</span>
					</div>
					<div class="stat">
						<span class="stat-label">Allocation</span>
						<span class="stat-value">{skill.trainingAllocation}%</span>
					</div>
				</div>

				<!-- Allocation Bar -->
				{#if skill.trainingAllocation > 0}
					<div class="allocation-bar">
						<div
							class="allocation-fill"
							style="width: {skill.trainingAllocation}%; background: {getSkillColor(skill.category)}"
						></div>
					</div>
				{/if}
			</div>
		{/each}
	</div>

	<!-- Total Allocation Summary -->
	<div class="allocation-summary">
		<span class="summary-label">Total Training Allocation:</span>
		<span class="summary-value" class:over={totalAllocation > 100}>
			{totalAllocation}%
		</span>
	</div>
</div>

<style>
	.skills-panel {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	.skill-tabs {
		display: flex;
		gap: var(--space-2);
		border-bottom: 1px solid var(--parchment-shadow);
		padding-bottom: var(--space-2);
		flex-wrap: wrap;
	}

	.tab {
		padding: var(--space-2) var(--space-3);
		background: transparent;
		border: none;
		border-radius: var(--radius-md);
		color: var(--ink-brown);
		cursor: pointer;
		font-size: var(--text-sm);
		font-weight: var(--font-medium);
		font-family: var(--font-display);
		transition: all var(--duration-fast);
	}

	.tab:hover {
		background: var(--parchment-medium);
		color: var(--ink-dark);
	}

	.tab.active {
		background: var(--ornament-gold);
		color: var(--ink-black);
	}

	.skills-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		max-height: 500px;
		overflow-y: auto;
		padding-right: var(--space-2);
	}

	.skill-row {
		padding: var(--space-3);
		background: var(--parchment-light);
		border: 1px solid var(--parchment-shadow);
		border-radius: var(--radius-md);
		transition: all var(--duration-fast);
	}

	.skill-row:hover {
		border-color: var(--ornament-dark);
		box-shadow: var(--shadow-sm);
	}

	.skill-header {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-bottom: var(--space-3);
	}

	.skill-icon {
		width: 28px;
		height: 28px;
		border-radius: var(--radius-sm);
		display: flex;
		align-items: center;
		justify-content: center;
		font-weight: var(--font-bold);
		font-size: var(--text-sm);
		color: var(--parchment-lightest);
		font-family: var(--font-display);
	}

	.skill-name {
		font-weight: var(--font-semibold);
		flex: 1;
		font-family: var(--font-display);
	}

	.skill-category {
		font-size: var(--text-xs);
		color: var(--ink-faded);
		padding: var(--space-1) var(--space-2);
		background: var(--parchment-dark);
		border-radius: var(--radius-sm);
		font-family: var(--font-stats);
	}

	.skill-bars {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin-bottom: var(--space-3);
	}

	.bar-container {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.bar-label {
		font-size: var(--text-xs);
		color: var(--ink-faded);
		width: 40px;
		font-family: var(--font-stats);
	}

	.skill-bar {
		flex: 1;
		height: 12px;
		background: var(--parchment-dark);
		border-radius: var(--radius-full);
		position: relative;
		overflow: hidden;
	}

	.skill-fill {
		height: 100%;
		border-radius: var(--radius-full);
		transition: width var(--duration-normal);
	}

	.tick {
		position: absolute;
		top: 0;
		bottom: 0;
		width: 1px;
		background: var(--ornament-dark);
	}

	.bar-value {
		font-size: var(--text-sm);
		font-weight: var(--font-bold);
		width: 32px;
		text-align: right;
		font-family: var(--font-stats);
	}

	.skill-stats {
		display: flex;
		gap: var(--space-4);
		font-size: var(--text-xs);
	}

	.stat {
		display: flex;
		flex-direction: column;
	}

	.stat-label {
		color: var(--ink-faded);
		font-family: var(--font-stats);
	}

	.stat-value {
		font-weight: var(--font-medium);
		font-family: var(--font-stats);
	}

	.allocation-bar {
		margin-top: var(--space-2);
		height: 4px;
		background: var(--parchment-dark);
		border-radius: var(--radius-full);
		overflow: hidden;
	}

	.allocation-fill {
		height: 100%;
		border-radius: var(--radius-full);
		opacity: 0.5;
	}

	.allocation-summary {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: var(--space-3);
		background: var(--parchment-dark);
		border-radius: var(--radius-md);
		border: 1px solid var(--parchment-shadow);
	}

	.summary-label {
		font-size: var(--text-sm);
		color: var(--ink-brown);
		font-family: var(--font-stats);
	}

	.summary-value {
		font-weight: var(--font-bold);
		color: var(--ornament-green);
		font-family: var(--font-stats);
	}

	.summary-value.over {
		color: var(--ornament-red);
	}

	/* Scrollbar styling */
	.skills-list::-webkit-scrollbar {
		width: 6px;
	}

	.skills-list::-webkit-scrollbar-track {
		background: var(--parchment-dark);
		border-radius: var(--radius-full);
	}

	.skills-list::-webkit-scrollbar-thumb {
		background: var(--ornament-dark);
		border-radius: var(--radius-full);
	}

	.skills-list::-webkit-scrollbar-thumb:hover {
		background: var(--ornament-gold);
	}
</style>
