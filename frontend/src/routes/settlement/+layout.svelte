<script lang="ts">
	import Header from '$lib/components/layout/Header.svelte';
	import { formattedDate, formattedTime } from '$lib/stores/time';
	import { settlementName } from '$lib/stores/settlement';

	let { children } = $props();
</script>

<div class="settlement-layout">
	<Header gameDate={$formattedDate} gameTime={$formattedTime} settlementName={$settlementName} />

	<main class="settlement-main">
		{@render children()}
	</main>
</div>

<style>
	.settlement-layout {
		display: flex;
		flex-direction: column;
		min-height: 100vh;
	}

	.settlement-main {
		flex: 1;
		display: flex;
		flex-direction: column;
	}

	/* Account for fixed bottom nav on mobile */
	@media (max-width: 768px) {
		.settlement-main {
			padding-bottom: calc(60px + env(safe-area-inset-bottom));
		}
	}
</style>
