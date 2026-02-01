<script lang="ts">
	import PageFrame from '$lib/components/layout/PageFrame.svelte';
	import PortraitMedallion from '$lib/components/characters/PortraitMedallion.svelte';
	import type { Character, Party } from '$lib/spacetimedb/types';

	// Mock data for development
	const mockParty: Party = {
		id: 1n,
		leaderId: 1n,
		questId: null,
		status: 'Idle',
		settlementId: 1n,
		createdAt: BigInt(Date.now())
	};

	const mockMembers: Character[] = [
		{
			id: 1n,
			ownerIdentity: 'mock',
			name: 'Marco',
			race: 'Human',
			isImmortal: false,
			ageDays: 8760,
			favorInvested: 50n,
			currentSettlementId: 1n,
			partyId: 1n,
			gold: 1250n,
			xp: 2450n,
			level: 5,
			createdAt: BigInt(Date.now()),
			updatedAt: BigInt(Date.now())
		},
		{
			id: 2n,
			ownerIdentity: 'mock',
			name: 'Isabella',
			race: 'Elf',
			isImmortal: false,
			ageDays: 36500,
			favorInvested: 30n,
			currentSettlementId: 1n,
			partyId: 1n,
			gold: 800n,
			xp: 1800n,
			level: 4,
			createdAt: BigInt(Date.now()),
			updatedAt: BigInt(Date.now())
		},
		{
			id: 3n,
			ownerIdentity: 'mock',
			name: 'Bjorn',
			race: 'Dwarf',
			isImmortal: false,
			ageDays: 14600,
			favorInvested: 20n,
			currentSettlementId: 1n,
			partyId: 1n,
			gold: 600n,
			xp: 1200n,
			level: 3,
			createdAt: BigInt(Date.now()),
			updatedAt: BigInt(Date.now())
		}
	];

	let selectedMemberId = $state<bigint | null>(null);

	function getStatusIcon(status: string): string {
		const icons: Record<string, string> = {
			Idle: '🏠',
			Traveling: '🚶',
			InMission: '⚔️',
			Resting: '😴'
		};
		return icons[status] || '❓';
	}

	function getRaceIcon(race: string): string {
		const icons: Record<string, string> = {
			Human: '👤',
			Elf: '🧝',
			Dwarf: '🧔',
			Halfling: '🧒',
			Orc: '👹',
			Goblin: '👺',
			Ratling: '🐀'
		};
		return icons[race] || '👤';
	}

	function selectMember(id: bigint) {
		selectedMemberId = selectedMemberId === id ? null : id;
	}

	function isLeader(characterId: bigint): boolean {
		return characterId === mockParty.leaderId;
	}
</script>

<svelte:head>
	<title>Party Management - Adventure Simulator</title>
</svelte:head>

<PageFrame variant="full">
	<div class="party-page">
		<header class="page-header">
			<h1>Party Management</h1>
			<div class="party-status">
				<span class="status-icon">{getStatusIcon(mockParty.status)}</span>
				<span class="status-text">{mockParty.status}</span>
			</div>
		</header>

		<div class="party-content">
			<!-- Party Roster -->
			<section class="roster-section panel">
				<h2 class="section-title">Party Roster</h2>
				<div class="roster">
					{#each mockMembers as member}
						<div
							class="member-card"
							class:selected={selectedMemberId === member.id}
							class:leader={isLeader(member.id)}
							onclick={() => selectMember(member.id)}
						>
							<PortraitMedallion
								name={member.name}
								size="md"
								selected={selectedMemberId === member.id}
							/>
							<div class="member-info">
								<div class="member-name">
									{member.name}
									{#if isLeader(member.id)}
										<span class="leader-badge">Leader</span>
									{/if}
								</div>
								<div class="member-details">
									<span class="race-badge">
										{getRaceIcon(member.race)} {member.race}
									</span>
									<span class="level-badge">Lvl {member.level}</span>
								</div>
							</div>
						</div>
					{/each}
				</div>

				<!-- Add member button -->
				<button class="add-member-btn">
					<span class="add-icon">+</span>
					<span>Recruit Member</span>
				</button>
			</section>

			<!-- Selected Member Details -->
			<section class="details-section panel">
				{#if selectedMemberId}
					{@const member = mockMembers.find((m) => m.id === selectedMemberId)}
					{#if member}
						<h2 class="section-title">{member.name}</h2>
						<div class="member-details-grid">
							<div class="detail-row">
								<span class="detail-label">Race</span>
								<span class="detail-value">{getRaceIcon(member.race)} {member.race}</span>
							</div>
							<div class="detail-row">
								<span class="detail-label">Level</span>
								<span class="detail-value">{member.level}</span>
							</div>
							<div class="detail-row">
								<span class="detail-label">XP</span>
								<span class="detail-value">{member.xp.toString()}</span>
							</div>
							<div class="detail-row">
								<span class="detail-label">Gold</span>
								<span class="detail-value">{member.gold.toString()}</span>
							</div>
							<div class="detail-row">
								<span class="detail-label">Age</span>
								<span class="detail-value">{Math.floor(member.ageDays / 365)} years</span>
							</div>
							<div class="detail-row">
								<span class="detail-label">Favor</span>
								<span class="detail-value">{member.favorInvested.toString()}</span>
							</div>
						</div>

						<div class="member-actions">
							<a href="/character" class="action-btn primary">View Full Sheet</a>
							{#if !isLeader(member.id)}
								<button class="action-btn">Promote to Leader</button>
								<button class="action-btn danger">Remove from Party</button>
							{/if}
						</div>
					{/if}
				{:else}
					<div class="no-selection">
						<p>Select a party member to view details</p>
					</div>
				{/if}
			</section>

			<!-- Party Actions -->
			<section class="actions-section panel">
				<h2 class="section-title">Party Actions</h2>
				<div class="action-buttons">
					<a href="/quests" class="action-btn large">
						<span class="action-icon">📜</span>
						<span class="action-label">Browse Quests</span>
					</a>
					<button class="action-btn large" disabled={mockParty.questId === null}>
						<span class="action-icon">⚔️</span>
						<span class="action-label">Enter Mission</span>
					</button>
					<button class="action-btn large">
						<span class="action-icon">😴</span>
						<span class="action-label">Rest Party</span>
					</button>
					<button class="action-btn large danger">
						<span class="action-icon">🚪</span>
						<span class="action-label">Disband Party</span>
					</button>
				</div>
			</section>
		</div>
	</div>
</PageFrame>

<style>
	.party-page {
		padding: var(--space-4);
	}

	.page-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: var(--space-6);
	}

	.page-header h1 {
		font-family: var(--font-display);
		font-size: var(--text-3xl);
		color: var(--ink-black);
		text-transform: uppercase;
		letter-spacing: var(--tracking-widest);
	}

	.party-status {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-4);
		background: var(--parchment-dark);
		border-radius: var(--radius-md);
	}

	.status-icon {
		font-size: var(--text-xl);
	}

	.status-text {
		font-family: var(--font-stats);
		color: var(--ink-dark);
	}

	.party-content {
		display: grid;
		grid-template-columns: 1fr 1fr;
		grid-template-rows: auto auto;
		gap: var(--space-4);
	}

	@media (max-width: 900px) {
		.party-content {
			grid-template-columns: 1fr;
		}
	}

	.panel {
		padding: var(--space-4);
		background: var(--parchment-light);
		border: 1px solid var(--parchment-shadow);
		border-radius: var(--radius-md);
	}

	.section-title {
		font-family: var(--font-display);
		font-size: var(--text-lg);
		color: var(--ink-dark);
		margin-bottom: var(--space-4);
		text-transform: uppercase;
		letter-spacing: var(--tracking-wider);
	}

	/* Roster */
	.roster {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		margin-bottom: var(--space-4);
	}

	.member-card {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-3);
		background: var(--parchment-base);
		border: 2px solid var(--parchment-shadow);
		border-radius: var(--radius-md);
		cursor: pointer;
		transition: all var(--duration-fast);
	}

	.member-card:hover {
		border-color: var(--ornament-dark);
	}

	.member-card.selected {
		border-color: var(--ornament-gold);
		background: var(--parchment-medium);
	}

	.member-card.leader {
		border-left: 4px solid var(--ornament-gold);
	}

	.member-info {
		flex: 1;
	}

	.member-name {
		font-family: var(--font-display);
		font-size: var(--text-base);
		color: var(--ink-dark);
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.leader-badge {
		font-size: var(--text-xs);
		padding: var(--space-1) var(--space-2);
		background: var(--ornament-gold);
		color: var(--ink-black);
		border-radius: var(--radius-sm);
		font-family: var(--font-stats);
	}

	.member-details {
		display: flex;
		gap: var(--space-2);
		margin-top: var(--space-1);
	}

	.race-badge,
	.level-badge {
		font-family: var(--font-stats);
		font-size: var(--text-xs);
		color: var(--ink-faded);
	}

	.add-member-btn {
		width: 100%;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: var(--space-2);
		padding: var(--space-3);
		background: var(--parchment-dark);
		border: 2px dashed var(--parchment-shadow);
		border-radius: var(--radius-md);
		font-family: var(--font-display);
		color: var(--ink-brown);
		cursor: pointer;
		transition: all var(--duration-fast);
	}

	.add-member-btn:hover {
		background: var(--parchment-medium);
		border-color: var(--ornament-dark);
	}

	.add-icon {
		font-size: var(--text-xl);
	}

	/* Details section */
	.member-details-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: var(--space-3);
		margin-bottom: var(--space-4);
	}

	.detail-row {
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
		font-size: var(--text-base);
		color: var(--ink-dark);
	}

	.member-actions {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.no-selection {
		display: flex;
		align-items: center;
		justify-content: center;
		min-height: 200px;
		color: var(--ink-faded);
		font-family: var(--font-stats);
		font-style: italic;
	}

	/* Actions section */
	.actions-section {
		grid-column: 1 / -1;
	}

	.action-buttons {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
		gap: var(--space-3);
	}

	.action-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: var(--space-2);
		padding: var(--space-3);
		background: var(--parchment-dark);
		border: 1px solid var(--parchment-shadow);
		border-radius: var(--radius-sm);
		font-family: var(--font-display);
		font-size: var(--text-sm);
		color: var(--ink-brown);
		cursor: pointer;
		text-decoration: none;
		transition: all var(--duration-fast);
	}

	.action-btn:hover:not(:disabled) {
		background: var(--parchment-medium);
		border-color: var(--ornament-dark);
	}

	.action-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.action-btn.primary {
		background: var(--ornament-green);
		border-color: var(--ornament-green);
		color: var(--parchment-lightest);
	}

	.action-btn.primary:hover {
		background: var(--ornament-green-light);
	}

	.action-btn.danger {
		background: var(--ornament-red-muted);
		border-color: var(--ornament-red);
		color: var(--parchment-lightest);
	}

	.action-btn.danger:hover:not(:disabled) {
		background: var(--ornament-red);
	}

	.action-btn.large {
		flex-direction: column;
		padding: var(--space-4);
	}

	.action-icon {
		font-size: var(--text-2xl);
	}

	.action-label {
		font-size: var(--text-sm);
	}
</style>
