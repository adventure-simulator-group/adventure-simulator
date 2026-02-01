<script lang="ts">
	interface StatItem {
		icon?: string;
		label: string;
		value: string | number;
		subtext?: string;
		progress?: number; // 0-100
	}

	interface Props {
		title?: string;
		stats: StatItem[];
		class?: string;
	}

	let { title = '', stats, class: className = '' }: Props = $props();
</script>

<div class="stats-panel {className}">
	{#if title}
		<h3 class="panel-title">{title}</h3>
	{/if}

	<div class="stats-list">
		{#each stats as stat}
			<div class="stat-item">
				<div class="stat-header">
					{#if stat.icon}
						<span class="stat-icon">{stat.icon}</span>
					{/if}
					<span class="stat-label">{stat.label}</span>
				</div>

				<div class="stat-content">
					<span class="stat-value">{stat.value}</span>
					{#if stat.subtext}
						<span class="stat-subtext">{stat.subtext}</span>
					{/if}
				</div>

				{#if stat.progress !== undefined}
					<div class="stat-progress">
						<div class="progress-bar" style="width: {stat.progress}%"></div>
					</div>
				{/if}
			</div>
		{/each}
	</div>
</div>

<style>
	.stats-panel {
		padding: var(--space-4);
	}

	.panel-title {
		font-family: var(--font-display);
		font-size: var(--text-sm);
		font-weight: var(--font-semibold);
		text-transform: uppercase;
		letter-spacing: var(--tracking-wider);
		color: var(--ink-dark);
		margin-bottom: var(--space-4);
		padding-bottom: var(--space-2);
		border-bottom: 1px solid var(--parchment-shadow);
	}

	.stats-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	.stat-item {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.stat-header {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.stat-icon {
		font-size: var(--text-lg);
		line-height: 1;
	}

	.stat-label {
		font-family: var(--font-stats);
		font-size: var(--text-sm);
		font-style: italic;
		color: var(--ink-medium);
	}

	.stat-content {
		display: flex;
		align-items: baseline;
		gap: var(--space-2);
		padding-left: var(--space-6);
	}

	.stat-value {
		font-family: var(--font-body);
		font-size: var(--text-base);
		font-weight: var(--font-semibold);
		color: var(--ink-dark);
	}

	.stat-subtext {
		font-family: var(--font-stats);
		font-size: var(--text-xs);
		color: var(--ink-faded);
	}

	.stat-progress {
		height: 6px;
		background-color: var(--parchment-shadow);
		border-radius: var(--radius-full);
		overflow: hidden;
		margin-left: var(--space-6);
		margin-top: var(--space-1);
	}

	.progress-bar {
		height: 100%;
		background: linear-gradient(
			to right,
			var(--ornament-gold-light),
			var(--ornament-gold)
		);
		border-radius: var(--radius-full);
		transition: width var(--duration-slow) var(--ease-out);
	}
</style>
