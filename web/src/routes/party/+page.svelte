<script lang="ts">
	import PageFrame from '$lib/components/layout/PageFrame.svelte';
	import PortraitMedallion from '$lib/components/characters/PortraitMedallion.svelte';
	import type { Character, Party } from '$lib/spacetimedb/types';

	// Extended character type with time tracking
	interface CharacterWithTime extends Character {
		currentTimeDays: number; // Days since year 0 in game world
	}

	// Renaissance month names
	const MONTH_NAMES = [
		'Januarius',
		'Februarius',
		'Martius',
		'Aprilis',
		'Maius',
		'Junius',
		'Julius',
		'Augustus',
		'September',
		'October',
		'November',
		'December'
	];

	// Convert days to renaissance-style date
	function formatRenaissanceDate(totalDays: number): { year: number; month: string; day: number } {
		const year = Math.floor(totalDays / 365);
		const dayOfYear = totalDays % 365;

		// Calculate month and day (simplified 30-day months + remainder)
		const daysInMonths = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
		let remainingDays = dayOfYear;
		let monthIndex = 0;

		for (let i = 0; i < 12; i++) {
			if (remainingDays < daysInMonths[i]) {
				monthIndex = i;
				break;
			}
			remainingDays -= daysInMonths[i];
			monthIndex = i + 1;
		}

		return {
			year: year,
			month: MONTH_NAMES[monthIndex % 12],
			day: remainingDays + 1
		};
	}

	// Mock data for development
	const mockParty: Party = {
		id: 1n,
		leaderId: 1n,
		questId: null,
		status: 'Idle',
		settlementId: 1n,
		createdAt: BigInt(Date.now())
	};

	// Official time (server time) - all players should be within 1 year of this
	const officialTimeDays = 568234; // Year 1556, around August

	const mockMembers: CharacterWithTime[] = [
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
			updatedAt: BigInt(Date.now()),
			currentTimeDays: 568231 // 3 days behind official time
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
			updatedAt: BigInt(Date.now()),
			currentTimeDays: 568089 // About 5 months behind
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
			updatedAt: BigInt(Date.now()),
			currentTimeDays: 567869 // About 1 year behind (max allowed)
		}
	];

	let selectedMemberId = $state<bigint | null>(null);

	function selectMember(id: bigint) {
		selectedMemberId = selectedMemberId === id ? null : id;
	}

	function isLeader(characterId: bigint): boolean {
		return characterId === mockParty.leaderId;
	}

	function getTimeSyncStatus(memberTimeDays: number): 'synced' | 'behind' | 'far-behind' {
		const daysBehind = officialTimeDays - memberTimeDays;
		if (daysBehind <= 30) return 'synced';
		if (daysBehind <= 180) return 'behind';
		return 'far-behind';
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
				<span class="status-text">{mockParty.status}</span>
			</div>
		</header>

		<div class="party-content">
			<!-- Party Roster -->
			<section class="roster-section panel">
				<h2 class="section-title">Party Roster</h2>
				<div class="roster">
					{#each mockMembers as member}
						{@const date = formatRenaissanceDate(member.currentTimeDays)}
						{@const syncStatus = getTimeSyncStatus(member.currentTimeDays)}
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
										{member.race}
									</span>
									<span class="level-badge">Lvl {member.level}</span>
								</div>
							</div>
							<div class="member-time" class:synced={syncStatus === 'synced'} class:behind={syncStatus === 'behind'} class:far-behind={syncStatus === 'far-behind'}>
								<div class="time-ornament-top"></div>
								<div class="time-year">{date.year}</div>
								<div class="time-divider"></div>
								<div class="time-month">{date.month}</div>
								<div class="time-day">{date.day}</div>
								<div class="time-ornament-bottom"></div>
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
								<span class="detail-value">{member.race}</span>
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
						<span class="action-label">Browse Quests</span>
					</a>
					<button class="action-btn large" disabled={mockParty.questId === null}>
						<span class="action-label">Enter Mission</span>
					</button>
					<button class="action-btn large">
						<span class="action-label">Rest Party</span>
					</button>
					<button class="action-btn large danger">
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

	/* Time Display - Renaissance Style */
	.member-time {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		padding: var(--space-2) var(--space-3);
		min-width: 90px;
		background: linear-gradient(
			135deg,
			var(--parchment-light) 0%,
			var(--parchment-base) 50%,
			var(--parchment-light) 100%
		);
		border: 1px solid var(--parchment-shadow);
		border-radius: var(--radius-sm);
		position: relative;
		box-shadow:
			inset 0 1px 0 rgba(255, 255, 255, 0.5),
			inset 0 -1px 0 rgba(0, 0, 0, 0.05);
	}

	.member-time::before,
	.member-time::after {
		content: '';
		position: absolute;
		left: 50%;
		transform: translateX(-50%);
		width: 60%;
		height: 1px;
		background: linear-gradient(
			90deg,
			transparent 0%,
			var(--ornament-gold) 20%,
			var(--ornament-gold) 80%,
			transparent 100%
		);
	}

	.member-time::before {
		top: 4px;
	}

	.member-time::after {
		bottom: 4px;
	}

	.member-time.synced {
		border-color: var(--ornament-green);
	}

	.member-time.behind {
		border-color: var(--ornament-gold);
	}

	.member-time.far-behind {
		border-color: var(--ornament-red-muted);
		background: linear-gradient(
			135deg,
			var(--parchment-light) 0%,
			#f5ebe0 50%,
			var(--parchment-light) 100%
		);
	}

	.time-ornament-top,
	.time-ornament-bottom {
		width: 24px;
		height: 6px;
		background: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 6'%3E%3Cpath d='M0 3 L4 1 L8 3 L12 0 L16 3 L20 1 L24 3' stroke='%23b8860b' fill='none' stroke-width='0.5'/%3E%3C/svg%3E") center/contain no-repeat;
		opacity: 0.6;
	}

	.time-year {
		font-family: var(--font-display);
		font-size: var(--text-lg);
		font-weight: 600;
		color: var(--ink-dark);
		letter-spacing: var(--tracking-wider);
		line-height: 1;
		margin-top: var(--space-1);
	}

	.time-divider {
		width: 100%;
		height: 1px;
		background: linear-gradient(
			90deg,
			transparent 0%,
			var(--ink-faded) 30%,
			var(--ink-faded) 70%,
			transparent 100%
		);
		margin: 2px 0;
	}

	.time-month {
		font-family: var(--font-body);
		font-size: var(--text-xs);
		font-style: italic;
		color: var(--ink-brown);
		letter-spacing: var(--tracking-wide);
		text-transform: capitalize;
	}

	.time-day {
		font-family: var(--font-stats);
		font-size: var(--text-sm);
		font-weight: 500;
		color: var(--ink-dark);
		line-height: 1;
		margin-bottom: var(--space-1);
	}

	/* Hover effect for time display */
	.member-card:hover .member-time {
		box-shadow:
			inset 0 1px 0 rgba(255, 255, 255, 0.5),
			inset 0 -1px 0 rgba(0, 0, 0, 0.05),
			0 2px 4px rgba(0, 0, 0, 0.1);
	}

	.member-time.far-behind .time-year {
		color: var(--ornament-red);
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

	.action-label {
		font-size: var(--text-sm);
	}
</style>
