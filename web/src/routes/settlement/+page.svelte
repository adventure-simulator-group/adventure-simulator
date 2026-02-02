<script lang="ts">
	import { onMount } from 'svelte';
	import PageFrame from '$lib/components/layout/PageFrame.svelte';
	import StatsPanel from '$lib/components/settlement/StatsPanel.svelte';
	import PortraitMedallion from '$lib/components/characters/PortraitMedallion.svelte';
	import { settlement, currentSettlement } from '$lib/stores/settlement';
	import { characters, ownedCharacters } from '$lib/stores/character';
	import type { Settlement, Character } from '$lib/spacetimedb/types';

	// Mock settlement data for development (extended with UI fields)
	interface SettlementDisplay extends Settlement {
		population: number;
		description: string;
		notableFeatures: string[];
		governance: {
			type: string;
			leaders: { name: string; title: string }[];
		};
		religion: {
			primary: string;
			influence: number;
		};
	}

	const mockSettlement: SettlementDisplay = {
		id: 1n,
		name: 'Brescia',
		coordX: 45.5,
		coordY: 10.2,
		populationLevel: 4,
		sceneKey: 'brescia',
		population: 32500,
		governance: {
			type: 'Republic',
			leaders: [
				{ name: 'Lord Vincenzo Vicentini', title: 'Podesta' },
				{ name: 'Mario Balacci', title: 'Capitano' },
				{ name: 'Father Tommaso', title: 'Bishop' },
				{ name: 'Ser Nicci', title: 'Merchant Prince' }
			]
		},
		religion: {
			primary: 'Catholic',
			influence: 75
		},
		notableFeatures: [
			'The Venetian Arsenal',
			"St. Mark's Cathedral",
			'The Iron Forge District',
			'Grand Market Square'
		],
		description:
			'On the slopes of Brescia rests the famed blade-city of Christendom. From the mineral-rich mountains flow the finest steels, destined for warriors across Europe.'
	};

	// Mock party data for development
	const mockParty: Character[] = [
		{
			id: 1n,
			ownerIdentity: 'mock',
			name: 'Marco',
			race: 'Human',
			isImmortal: false,
			ageDays: 7300,
			favorInvested: 0n,
			currentSettlementId: 1n,
			partyId: 1n,
			gold: 150n,
			xp: 150n,
			level: 3,
			createdAt: BigInt(Date.now()),
			updatedAt: BigInt(Date.now())
		},
		{
			id: 2n,
			ownerIdentity: 'mock',
			name: 'Isabella',
			race: 'Human',
			isImmortal: false,
			ageDays: 6570,
			favorInvested: 0n,
			currentSettlementId: 1n,
			partyId: 1n,
			gold: 80n,
			xp: 120n,
			level: 2,
			createdAt: BigInt(Date.now()),
			updatedAt: BigInt(Date.now())
		},
		{
			id: 3n,
			ownerIdentity: 'mock',
			name: 'Giovanni',
			race: 'Dwarf',
			isImmortal: false,
			ageDays: 14600,
			favorInvested: 0n,
			currentSettlementId: 1n,
			partyId: 1n,
			gold: 200n,
			xp: 200n,
			level: 4,
			createdAt: BigInt(Date.now()),
			updatedAt: BigInt(Date.now())
		},
		{
			id: 4n,
			ownerIdentity: 'mock',
			name: 'Lucia',
			race: 'Elf',
			isImmortal: false,
			ageDays: 36500,
			favorInvested: 0n,
			currentSettlementId: 1n,
			partyId: 1n,
			gold: 60n,
			xp: 80n,
			level: 2,
			createdAt: BigInt(Date.now()),
			updatedAt: BigInt(Date.now())
		}
	];

	let selectedCharacterId = $state<bigint | null>(null);

	// Extended settlement for display
	let displaySettlement = $state<SettlementDisplay | null>(null);

	// Initialize with mock data
	onMount(() => {
		settlement.setCurrent(mockSettlement);
		displaySettlement = mockSettlement;
		characters.setAll(mockParty);
	});

	// Reactive stats for the sidebar
	const populationStats = $derived([
		{
			label: 'Population',
			value: displaySettlement?.population?.toLocaleString() ?? '—',
			progress: 70
		},
		{
			label: 'Prosperity',
			value: '78%',
			progress: 78
		}
	]);

	const governanceStats = $derived(
		displaySettlement?.governance?.leaders.map((leader) => ({
			label: leader.title,
			value: leader.name
		})) ?? []
	);

	const religionStats = $derived([
		{
			label: 'Religion',
			value: displaySettlement?.religion?.primary ?? '—',
			progress: displaySettlement?.religion?.influence ?? 0
		}
	]);

	function selectCharacter(id: bigint) {
		selectedCharacterId = selectedCharacterId === id ? null : id;
	}
</script>

<svelte:head>
	<title>{$currentSettlement?.name ?? 'Settlement'} - Adventure Simulator</title>
</svelte:head>

<PageFrame variant="full">
	<div class="settlement-page">
		<!-- Left Sidebar: Stats & Services -->
		<aside class="sidebar sidebar-left">
			<div class="sidebar-section">
				<StatsPanel title="Population" stats={populationStats} />
			</div>

			<div class="sidebar-section">
				<StatsPanel title="Religion" stats={religionStats} />
			</div>

			<div class="sidebar-section services">
				<h3 class="section-title">Services</h3>
				<nav class="services-nav">
					<a href="/settlement/merchants" class="service-link">
						<span class="service-name">Merchants</span>
					</a>
					<a href="/settlement/tavern" class="service-link">
						<span class="service-name">Tavern</span>
					</a>
					<a href="/settlement/quests" class="service-link">
						<span class="service-name">Quest Board</span>
					</a>
					<a href="/settlement/services" class="service-link">
						<span class="service-name">Smith</span>
					</a>
				</nav>
			</div>
		</aside>

		<!-- Main Content -->
		<main class="main-content">
			<!-- Settlement Header -->
			<header class="settlement-header">
				<h1 class="settlement-name">{$currentSettlement?.name ?? 'Settlement'}</h1>
			</header>

			<!-- Settlement Artwork / Description -->
			<div class="settlement-showcase">
				<div class="settlement-art">
					<!-- Placeholder for settlement artwork -->
					<div class="art-placeholder">
						<span class="art-label">Settlement View</span>
					</div>
				</div>
			</div>

			<!-- Party Portraits -->
			<section class="party-section">
				<h2 class="section-title">Your Party</h2>
				<div class="party-portraits">
					{#each $ownedCharacters as character}
						<PortraitMedallion
							name={character.name}
							size="md"
							selected={selectedCharacterId === character.id}
							onclick={() => selectCharacter(character.id)}
						/>
					{/each}
				</div>
			</section>
		</main>

		<!-- Right Sidebar: About & Governance -->
		<aside class="sidebar sidebar-right">
			<div class="sidebar-section">
				<h3 class="section-title">About {$currentSettlement?.name ?? 'Settlement'}</h3>
				<p class="about-text">
					{displaySettlement?.description ?? 'A settlement in the realm.'}
				</p>
			</div>

			<div class="sidebar-section">
				<StatsPanel title="Governance" stats={governanceStats} />
			</div>

			<div class="sidebar-section">
				<h3 class="section-title">Notable Features</h3>
				<ul class="features-list">
					{#each displaySettlement?.notableFeatures ?? [] as feature}
						<li class="feature-item">{feature}</li>
					{/each}
				</ul>
			</div>
		</aside>
	</div>
</PageFrame>

<style>
	.settlement-page {
		display: flex;
		flex-direction: column;
		gap: var(--space-6);
		min-height: 100%;
	}

	/* Sidebar styles */
	.sidebar {
		background-color: var(--parchment-light);
		border: 1px solid var(--parchment-shadow);
		border-radius: var(--radius-sm);
	}

	.sidebar-section {
		padding: var(--space-4);
		border-bottom: 1px solid var(--parchment-shadow);
	}

	.sidebar-section:last-child {
		border-bottom: none;
	}

	.section-title {
		font-family: var(--font-display);
		font-size: var(--text-sm);
		font-weight: var(--font-semibold);
		text-transform: uppercase;
		letter-spacing: var(--tracking-wider);
		color: var(--ink-dark);
		margin-bottom: var(--space-3);
	}

	/* Services navigation */
	.services-nav {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.service-link {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-2) var(--space-3);
		text-decoration: none;
		color: var(--ink-brown);
		border-radius: var(--radius-sm);
		transition: all var(--duration-fast) var(--ease-out);
	}

	.service-link:hover {
		background-color: var(--parchment-base);
		color: var(--ink-dark);
	}

	.service-name {
		font-family: var(--font-stats);
		font-size: var(--text-sm);
	}

	/* Main content */
	.main-content {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: var(--space-6);
	}

	.settlement-header {
		text-align: center;
		padding: var(--space-4) 0;
	}

	.settlement-name {
		font-family: var(--font-display);
		font-size: var(--text-4xl);
		font-weight: var(--font-bold);
		letter-spacing: var(--tracking-widest);
		text-transform: uppercase;
		color: var(--ink-black);
		text-shadow:
			1px 1px 0 var(--parchment-light),
			2px 2px 4px var(--ornament-shadow);
		margin: 0;
	}

	/* Settlement showcase */
	.settlement-showcase {
		display: flex;
		justify-content: center;
	}

	.settlement-art {
		width: 100%;
		max-width: 600px;
		aspect-ratio: 16 / 10;
		background-color: var(--parchment-dark);
		border: 2px solid var(--ornament-dark);
		border-radius: var(--radius-sm);
		overflow: hidden;
	}

	.art-placeholder {
		width: 100%;
		height: 100%;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: var(--space-2);
		color: var(--ink-faded);
	}

	.art-label {
		font-family: var(--font-stats);
		font-style: italic;
	}

	/* Party section */
	.party-section {
		text-align: center;
	}

	.party-portraits {
		display: flex;
		justify-content: center;
		flex-wrap: wrap;
		gap: var(--space-4);
		margin-top: var(--space-4);
	}

	/* About section */
	.about-text {
		font-family: var(--font-body);
		font-size: var(--text-base);
		line-height: var(--leading-relaxed);
		color: var(--ink-brown);
	}

	.features-list {
		list-style: none;
		padding: 0;
		margin: 0;
	}

	.feature-item {
		position: relative;
		padding-left: var(--space-5);
		margin-bottom: var(--space-2);
		font-family: var(--font-stats);
		font-size: var(--text-sm);
		color: var(--ink-medium);
	}

	.feature-item::before {
		content: '◆';
		position: absolute;
		left: 0;
		color: var(--ornament-gold);
		font-size: var(--text-xs);
	}

	/* Desktop: Three-column layout */
	@media (min-width: 1024px) {
		.settlement-page {
			display: grid;
			grid-template-columns: 360px 1fr 420px;
			gap: var(--space-6);
		}

		.sidebar-left {
			order: 1;
		}

		.main-content {
			order: 2;
		}

		.sidebar-right {
			order: 3;
		}
	}

	/* Tablet: Two-column layout */
	@media (min-width: 768px) and (max-width: 1023px) {
		.settlement-page {
			display: grid;
			grid-template-columns: 330px 1fr;
			gap: var(--space-4);
		}

		.sidebar-right {
			grid-column: 1 / -1;
			display: grid;
			grid-template-columns: 1fr 1fr;
			gap: var(--space-4);
		}

		.sidebar-right .sidebar-section {
			border-bottom: none;
		}
	}

	/* Mobile: Stacked layout */
	@media (max-width: 767px) {
		.sidebar-left {
			order: 2;
		}

		.main-content {
			order: 1;
		}

		.sidebar-right {
			order: 3;
		}

		.settlement-name {
			font-size: var(--text-2xl);
		}

		.party-portraits {
			gap: var(--space-3);
		}
	}
</style>
