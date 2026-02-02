<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		variant?: 'full' | 'panel' | 'modal' | 'card';
		class?: string;
		children: Snippet;
	}

	let { variant = 'full', class: className = '', children }: Props = $props();
</script>

<div class="page-frame {variant} {className}" class:parchment-bg={variant === 'full'}>
	<div class="frame-content">
		{@render children()}
	</div>
</div>

<style>
	.page-frame {
		position: relative;
		background-color: var(--parchment-base);
	}

	/* Full variant - main page wrapper */
	.page-frame.full {
		min-height: 100vh;
	}

	/* Panel variant - for sidebars and sections */
	.page-frame.panel {
		padding: var(--space-3);
		border: 1px solid var(--parchment-shadow);
		border-radius: var(--radius-sm);
	}

	/* Modal variant - for dialogs */
	.page-frame.modal {
		padding: var(--space-4);
		max-width: 90vw;
		max-height: 90vh;
		overflow: auto;
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-xl);
	}

	/* Card variant - for smaller content blocks */
	.page-frame.card {
		padding: var(--space-2);
		border: 1px solid var(--parchment-shadow);
		border-radius: var(--radius-sm);
		box-shadow: var(--shadow-sm);
	}

	/* Content area */
	.frame-content {
		position: relative;
		padding: var(--space-4);
		min-height: 100%;
	}

	/* Responsive adjustments */
	@media (max-width: 768px) {
		.frame-content {
			padding: var(--space-3);
		}
	}

	@media (max-width: 480px) {
		.frame-content {
			padding: var(--space-2);
		}
	}
</style>
