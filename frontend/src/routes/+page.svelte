<script lang="ts">
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';

	// Redirect to settlement on mount
	onMount(() => {
		goto('/settlement');
	});
</script>

<div class="loading-screen">
	<div class="loading-content">
		<h1 class="title">Adventure Simulator</h1>
		<p class="subtitle">Loading...</p>
		<div class="spinner"></div>
	</div>
</div>

<style>
	.loading-screen {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: var(--space-4);
	}

	.loading-content {
		text-align: center;
	}

	.title {
		font-family: var(--font-display);
		font-size: var(--text-4xl);
		font-weight: var(--font-bold);
		letter-spacing: var(--tracking-widest);
		text-transform: uppercase;
		color: var(--ink-black);
		margin-bottom: var(--space-4);
	}

	.subtitle {
		font-family: var(--font-stats);
		font-style: italic;
		color: var(--ink-medium);
		margin-bottom: var(--space-6);
	}

	.spinner {
		width: 40px;
		height: 40px;
		margin: 0 auto;
		border: 3px solid var(--parchment-shadow);
		border-top-color: var(--ornament-gold);
		border-radius: var(--radius-full);
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
