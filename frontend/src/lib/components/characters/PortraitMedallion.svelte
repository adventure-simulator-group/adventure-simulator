<script lang="ts">
	interface Props {
		name: string;
		portraitUrl?: string;
		size?: 'sm' | 'md' | 'lg' | 'xl';
		selected?: boolean;
		showName?: boolean;
		showFrame?: boolean;
		interactive?: boolean;
		class?: string;
		onclick?: () => void;
	}

	let {
		name,
		portraitUrl = '',
		size = 'md',
		selected = false,
		showName = true,
		showFrame = true,
		interactive = true,
		class: className = '',
		onclick
	}: Props = $props();

	// Generate initials from name as fallback
	function getInitials(name: string): string {
		return name
			.split(' ')
			.map((word) => word[0])
			.join('')
			.toUpperCase()
			.slice(0, 2);
	}
</script>

{#if interactive}
	<button
		class="medallion size-{size} {className}"
		class:selected
		class:has-frame={showFrame}
		onclick={onclick}
		aria-label="Select {name}"
		aria-pressed={selected}
	>
		<div class="medallion-frame">
			<div class="medallion-inner">
				{#if portraitUrl}
					<img src={portraitUrl} alt={name} class="portrait-image" loading="lazy" />
				{:else}
					<div class="portrait-placeholder">
						<span class="initials">{getInitials(name)}</span>
					</div>
				{/if}
			</div>
		</div>

		{#if showName}
			<span class="medallion-name">{name}</span>
		{/if}
	</button>
{:else}
	<div class="medallion size-{size} {className}" class:selected class:has-frame={showFrame}>
		<div class="medallion-frame">
			<div class="medallion-inner">
				{#if portraitUrl}
					<img src={portraitUrl} alt={name} class="portrait-image" loading="lazy" />
				{:else}
					<div class="portrait-placeholder">
						<span class="initials">{getInitials(name)}</span>
					</div>
				{/if}
			</div>
		</div>

		{#if showName}
			<span class="medallion-name">{name}</span>
		{/if}
	</div>
{/if}

<style>
	.medallion {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--space-2);
		text-decoration: none;
		color: inherit;
		transition: transform var(--duration-fast) var(--ease-out);
	}

	button.medallion {
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
	}

	button.medallion:hover {
		transform: scale(1.05);
	}

	button.medallion:active {
		transform: scale(0.98);
	}

	/* Size variants */
	.size-sm .medallion-frame {
		width: var(--medallion-sm);
		height: var(--medallion-sm);
	}

	.size-md .medallion-frame {
		width: var(--medallion-md);
		height: var(--medallion-md);
	}

	.size-lg .medallion-frame {
		width: var(--medallion-lg);
		height: var(--medallion-lg);
	}

	.size-xl .medallion-frame {
		width: var(--medallion-xl);
		height: var(--medallion-xl);
	}

	/* Frame styling */
	.medallion-frame {
		position: relative;
		border-radius: var(--radius-full);
		padding: 3px;
		background: linear-gradient(
			135deg,
			var(--ornament-gold-light) 0%,
			var(--ornament-dark) 40%,
			var(--ornament-gold) 100%
		);
		box-shadow:
			0 2px 4px rgba(0, 0, 0, 0.2),
			inset 0 1px 2px rgba(255, 255, 255, 0.2);
		transition: box-shadow var(--duration-fast) var(--ease-out);
	}

	.has-frame .medallion-frame {
		background-image: url('/borders/medallion-frame.svg');
		background-size: cover;
		background-position: center;
		padding: 8%;
	}

	button.medallion:hover .medallion-frame {
		box-shadow:
			0 4px 8px rgba(0, 0, 0, 0.25),
			inset 0 1px 2px rgba(255, 255, 255, 0.3),
			0 0 12px rgba(196, 164, 78, 0.3);
	}

	/* Selected state */
	.selected .medallion-frame {
		box-shadow:
			0 0 0 3px var(--ornament-gold),
			0 4px 8px rgba(0, 0, 0, 0.3),
			0 0 16px rgba(196, 164, 78, 0.4);
	}

	/* Inner circle (portrait container) */
	.medallion-inner {
		width: 100%;
		height: 100%;
		border-radius: var(--radius-full);
		overflow: hidden;
		background-color: var(--parchment-base);
	}

	.portrait-image {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	/* Placeholder when no portrait */
	.portrait-placeholder {
		width: 100%;
		height: 100%;
		display: flex;
		align-items: center;
		justify-content: center;
		background: linear-gradient(135deg, var(--parchment-light) 0%, var(--parchment-aged) 100%);
	}

	.initials {
		font-family: var(--font-display);
		font-weight: var(--font-bold);
		color: var(--ink-medium);
	}

	.size-sm .initials {
		font-size: var(--text-xs);
	}

	.size-md .initials {
		font-size: var(--text-base);
	}

	.size-lg .initials {
		font-size: var(--text-xl);
	}

	.size-xl .initials {
		font-size: var(--text-2xl);
	}

	/* Name label */
	.medallion-name {
		font-family: var(--font-stats);
		font-size: var(--text-sm);
		color: var(--ink-dark);
		text-align: center;
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.size-sm .medallion-name {
		font-size: var(--text-xs);
		max-width: calc(var(--medallion-sm) + var(--space-4));
	}

	.size-md .medallion-name {
		max-width: calc(var(--medallion-md) + var(--space-4));
	}

	.size-lg .medallion-name {
		max-width: calc(var(--medallion-lg) + var(--space-4));
	}

	.size-xl .medallion-name {
		font-size: var(--text-base);
		max-width: calc(var(--medallion-xl) + var(--space-4));
	}

	/* Focus state for accessibility */
	button.medallion:focus-visible {
		outline: none;
	}

	button.medallion:focus-visible .medallion-frame {
		box-shadow:
			0 0 0 3px var(--ornament-gold),
			0 0 0 5px var(--parchment-base),
			0 0 0 7px var(--ornament-dark);
	}
</style>
