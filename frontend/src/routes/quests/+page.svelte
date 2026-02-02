<script lang="ts">
	import { onMount } from 'svelte';
	import PageFrame from '$lib/components/layout/PageFrame.svelte';
	import { settlement, availableQuests } from '$lib/stores/settlement';
	import type { Quest } from '$lib/spacetimedb/types';

	// Mock quests for development
	const mockQuests: Quest[] = [
		{
			id: 1n,
			settlementId: 1n,
			title: 'Clear the Bandit Camp',
			description: 'A group of bandits has set up camp in the hills north of town. The merchant guild offers a bounty for their removal.',
			difficulty: 3,
			recommendedLevel: 3,
			rewardGold: 150n,
			rewardXp: 200n,
			rewardFavor: 10n,
			enemyType: 'Bandits',
			enemyCount: 8,
			status: 'Available',
			createdAt: BigInt(Date.now())
		},
		{
			id: 2n,
			settlementId: 1n,
			title: 'Wolf Pack Menace',
			description: 'Wolves have been attacking travelers on the eastern road. The local farmers need protection.',
			difficulty: 2,
			recommendedLevel: 2,
			rewardGold: 80n,
			rewardXp: 100n,
			rewardFavor: 5n,
			enemyType: 'Wolves',
			enemyCount: 5,
			status: 'Available',
			createdAt: BigInt(Date.now())
		},
		{
			id: 3n,
			settlementId: 1n,
			title: 'Goblin Raiding Party',
			description: 'A goblin raiding party has been spotted in the nearby forest. Stop them before they reach the village.',
			difficulty: 4,
			recommendedLevel: 4,
			rewardGold: 200n,
			rewardXp: 300n,
			rewardFavor: 15n,
			enemyType: 'Goblins',
			enemyCount: 12,
			status: 'Available',
			createdAt: BigInt(Date.now())
		},
		{
			id: 4n,
			settlementId: 1n,
			title: 'The Undead Crypt',
			description: 'Restless spirits have been emerging from the old crypt. A priest requests aid in putting them to rest.',
			difficulty: 5,
			recommendedLevel: 5,
			rewardGold: 350n,
			rewardXp: 500n,
			rewardFavor: 25n,
			enemyType: 'Undead',
			enemyCount: 15,
			status: 'Available',
			createdAt: BigInt(Date.now())
		}
	];

	let selectedQuest = $state<Quest | null>(null);
	let filterDifficulty = $state<number | null>(null);

	const filteredQuests = $derived(
		filterDifficulty === null
			? mockQuests
			: mockQuests.filter((q) => q.difficulty <= filterDifficulty!)
	);

	// Initialize with mock data
	onMount(() => {
		settlement.setQuests(mockQuests);
	});

	function getDifficultyLabel(difficulty: number): string {
		if (difficulty <= 2) return 'Easy';
		if (difficulty <= 3) return 'Medium';
		if (difficulty <= 4) return 'Hard';
		return 'Very Hard';
	}

	function getDifficultyColor(difficulty: number): string {
		if (difficulty <= 2) return 'var(--ornament-green)';
		if (difficulty <= 3) return 'var(--ornament-gold)';
		if (difficulty <= 4) return 'var(--ornament-red-muted)';
		return 'var(--ornament-red)';
	}


	function acceptQuest(quest: Quest) {
		console.log('Accepting quest:', quest.title);
		// Would call reducers.acceptQuest(partyId, quest.id)
	}
</script>

<svelte:head>
	<title>Quest Board - Adventure Simulator</title>
</svelte:head>

<PageFrame variant="full">
	<div class="quests-page">
		<header class="page-header">
			<h1>Quest Board</h1>
			<p class="subtitle">Available bounties and missions</p>
		</header>

		<!-- Filters -->
		<div class="filters">
			<span class="filter-label">Difficulty:</span>
			<button
				class="filter-btn"
				class:active={filterDifficulty === null}
				onclick={() => (filterDifficulty = null)}
			>
				All
			</button>
			<button
				class="filter-btn"
				class:active={filterDifficulty === 2}
				onclick={() => (filterDifficulty = 2)}
			>
				Easy
			</button>
			<button
				class="filter-btn"
				class:active={filterDifficulty === 3}
				onclick={() => (filterDifficulty = 3)}
			>
				Medium
			</button>
			<button
				class="filter-btn"
				class:active={filterDifficulty === 5}
				onclick={() => (filterDifficulty = 5)}
			>
				Hard+
			</button>
		</div>

		<!-- Quest List -->
		<div class="quest-grid">
			{#each filteredQuests as quest}
				<div
					class="quest-card"
					class:selected={selectedQuest?.id === quest.id}
					onclick={() => (selectedQuest = selectedQuest?.id === quest.id ? null : quest)}
				>
					<div class="quest-header">
						<h3 class="quest-title">{quest.title}</h3>
						<span
							class="difficulty-badge"
							style="background: {getDifficultyColor(quest.difficulty)}"
						>
							{getDifficultyLabel(quest.difficulty)}
						</span>
					</div>

					<p class="quest-description">{quest.description}</p>

					<div class="quest-details">
						<div class="detail">
							<span class="detail-label">Enemies</span>
							<span class="detail-value">{quest.enemyCount} {quest.enemyType}</span>
						</div>
						<div class="detail">
							<span class="detail-label">Level</span>
							<span class="detail-value">{quest.recommendedLevel}+</span>
						</div>
					</div>

					<div class="quest-rewards">
						<div class="reward">
							<span class="reward-value">{quest.rewardGold.toString()} Gold</span>
						</div>
						<div class="reward">
							<span class="reward-value">{quest.rewardXp.toString()} XP</span>
						</div>
						<div class="reward">
							<span class="reward-value">{quest.rewardFavor.toString()} Favor</span>
						</div>
					</div>

					{#if selectedQuest?.id === quest.id}
						<button class="accept-btn" onclick={() => acceptQuest(quest)}>
							Accept Quest
						</button>
					{/if}
				</div>
			{/each}
		</div>

		{#if filteredQuests.length === 0}
			<div class="no-quests">
				<p>No quests match your filter.</p>
			</div>
		{/if}
	</div>
</PageFrame>

<style>
	.quests-page {
		padding: var(--space-4);
	}

	.page-header {
		text-align: center;
		margin-bottom: var(--space-6);
	}

	.page-header h1 {
		font-family: var(--font-display);
		font-size: var(--text-3xl);
		color: var(--ink-black);
		text-transform: uppercase;
		letter-spacing: var(--tracking-widest);
		margin-bottom: var(--space-2);
	}

	.subtitle {
		font-family: var(--font-stats);
		color: var(--ink-faded);
		font-style: italic;
	}

	/* Filters */
	.filters {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-bottom: var(--space-6);
		flex-wrap: wrap;
	}

	.filter-label {
		font-family: var(--font-display);
		font-size: var(--text-sm);
		color: var(--ink-brown);
	}

	.filter-btn {
		padding: var(--space-2) var(--space-3);
		background: var(--parchment-dark);
		border: 1px solid var(--parchment-shadow);
		border-radius: var(--radius-sm);
		font-family: var(--font-stats);
		font-size: var(--text-sm);
		color: var(--ink-brown);
		cursor: pointer;
		transition: all var(--duration-fast);
	}

	.filter-btn:hover {
		background: var(--parchment-medium);
	}

	.filter-btn.active {
		background: var(--ornament-gold);
		border-color: var(--ornament-gold);
		color: var(--ink-black);
	}

	/* Quest Grid */
	.quest-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
		gap: var(--space-4);
	}

	.quest-card {
		background: var(--parchment-light);
		border: 2px solid var(--parchment-shadow);
		border-radius: var(--radius-md);
		padding: var(--space-4);
		cursor: pointer;
		transition: all var(--duration-fast);
	}

	.quest-card:hover {
		border-color: var(--ornament-dark);
		box-shadow: var(--shadow-md);
	}

	.quest-card.selected {
		border-color: var(--ornament-gold);
		box-shadow: 0 0 0 2px var(--ornament-gold-muted);
	}

	.quest-header {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-bottom: var(--space-3);
	}

	.quest-title {
		flex: 1;
		font-family: var(--font-display);
		font-size: var(--text-lg);
		color: var(--ink-dark);
	}

	.difficulty-badge {
		padding: var(--space-1) var(--space-2);
		border-radius: var(--radius-sm);
		font-family: var(--font-stats);
		font-size: var(--text-xs);
		color: var(--parchment-lightest);
		text-transform: uppercase;
	}

	.quest-description {
		font-family: var(--font-body);
		font-size: var(--text-sm);
		color: var(--ink-brown);
		line-height: var(--leading-relaxed);
		margin-bottom: var(--space-4);
	}

	.quest-details {
		display: flex;
		gap: var(--space-4);
		margin-bottom: var(--space-4);
	}

	.detail {
		display: flex;
		flex-direction: column;
	}

	.detail-label {
		font-family: var(--font-stats);
		font-size: var(--text-xs);
		color: var(--ink-faded);
		text-transform: uppercase;
	}

	.detail-value {
		font-family: var(--font-stats);
		font-size: var(--text-sm);
		color: var(--ink-dark);
	}

	.quest-rewards {
		display: flex;
		gap: var(--space-4);
		padding: var(--space-3);
		background: var(--parchment-dark);
		border-radius: var(--radius-sm);
		margin-bottom: var(--space-3);
	}

	.reward {
		display: flex;
		align-items: center;
		gap: var(--space-1);
	}

	.reward-value {
		font-family: var(--font-stats);
		font-size: var(--text-sm);
		color: var(--ink-dark);
	}

	.accept-btn {
		width: 100%;
		padding: var(--space-3);
		background: var(--ornament-green);
		border: none;
		border-radius: var(--radius-sm);
		font-family: var(--font-display);
		font-size: var(--text-base);
		color: var(--parchment-lightest);
		cursor: pointer;
		transition: all var(--duration-fast);
	}

	.accept-btn:hover {
		background: var(--ornament-green-light);
	}

	.no-quests {
		text-align: center;
		padding: var(--space-8);
		color: var(--ink-faded);
		font-family: var(--font-stats);
		font-style: italic;
	}
</style>
