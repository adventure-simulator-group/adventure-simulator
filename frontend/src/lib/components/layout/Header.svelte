<script lang="ts">
	import TabNav from './TabNav.svelte';

	interface Props {
		gameDate?: string;
		gameTime?: string;
		settlementName?: string;
	}

	let {
		gameDate = '23rd of August',
		gameTime = '3:35 PM',
		settlementName = ''
	}: Props = $props();
</script>

<header class="header">
	<div class="header-top">
		<div class="header-left">
			{#if settlementName}
				<span class="settlement-indicator">{settlementName}</span>
			{/if}
		</div>

		<div class="header-center">
			<div class="game-datetime">
				<span class="game-date">{gameDate}</span>
				<span class="datetime-separator">,</span>
				<span class="game-time">{gameTime}</span>
			</div>
		</div>

		<div class="header-right">
			<button class="header-action" aria-label="Settings">
				<svg
					width="20"
					height="20"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					stroke-width="2"
				>
					<circle cx="12" cy="12" r="3" />
					<path
						d="M12 1v2m0 18v2M4.22 4.22l1.42 1.42m12.72 12.72l1.42 1.42M1 12h2m18 0h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42"
					/>
				</svg>
			</button>
		</div>
	</div>

	<TabNav />
</header>

<style>
	.header {
		position: sticky;
		top: 0;
		z-index: var(--z-sticky);
		background-color: var(--parchment-dark);
		border-bottom: 1px solid var(--parchment-shadow);
	}

	.header-top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--space-2) var(--space-4);
		min-height: var(--header-height);
	}

	.header-left,
	.header-right {
		flex: 1;
		display: flex;
		align-items: center;
	}

	.header-right {
		justify-content: flex-end;
	}

	.header-center {
		flex: 2;
		display: flex;
		justify-content: center;
	}

	.settlement-indicator {
		font-family: var(--font-stats);
		font-size: var(--text-sm);
		font-style: italic;
		color: var(--ink-medium);
	}

	.game-datetime {
		display: flex;
		align-items: center;
		gap: var(--space-1);
		font-family: var(--font-stats);
	}

	.game-date {
		font-size: var(--text-sm);
		font-style: italic;
		color: var(--ink-medium);
	}

	.datetime-separator {
		color: var(--ink-faded);
	}

	.game-time {
		font-size: var(--text-sm);
		color: var(--ink-dark);
		font-weight: var(--font-medium);
	}

	.header-action {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 36px;
		height: 36px;
		border-radius: var(--radius-full);
		color: var(--ink-medium);
		transition: all var(--duration-fast) var(--ease-out);
	}

	.header-action:hover {
		background-color: var(--parchment-base);
		color: var(--ink-dark);
	}

	.header-action:active {
		transform: scale(0.95);
	}

	/* Responsive */
	@media (max-width: 768px) {
		.header-top {
			padding: var(--space-2) var(--space-3);
		}

		.settlement-indicator {
			display: none;
		}
	}
</style>
