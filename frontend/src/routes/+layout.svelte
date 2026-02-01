<script lang="ts">
	import { onMount } from 'svelte';
	import '$lib/styles/global.css';
	import favicon from '$lib/assets/favicon.svg';
	import ChatBar from '$lib/components/layout/ChatBar.svelte';
	import { connection } from '$lib/stores/connection';
	import { characters } from '$lib/stores/character';

	let { children } = $props();

	// Initialize connection on mount
	onMount(async () => {
		try {
			await connection.connect();

			// Set up a mock current character for chat functionality
			characters.setCurrent({
				id: 1n,
				ownerIdentity: 'mock-identity',
				name: 'Player',
				race: 'Human',
				isImmortal: false,
				ageDays: 7300,
				favorInvested: 0n,
				currentSettlementId: 1n,
				partyId: null,
				gold: 100n,
				xp: 0n,
				level: 1,
				createdAt: BigInt(Date.now()),
				updatedAt: BigInt(Date.now())
			});
		} catch (error) {
			console.error('Failed to connect:', error);
		}
	});
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>Adventure Simulator</title>
	<meta name="description" content="Strategic layer interface for Adventure Simulator" />
</svelte:head>

<div class="app parchment-texture">
	{@render children()}
	<ChatBar />
</div>

<style>
	.app {
		min-height: 100vh;
		display: flex;
		flex-direction: column;
	}
</style>
