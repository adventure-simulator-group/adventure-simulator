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
	<!-- Corner ornaments -->
	<div class="frame-corners" aria-hidden="true">
		<div class="corner corner-tl"></div>
		<div class="corner corner-tr"></div>
		<div class="corner corner-bl"></div>
		<div class="corner corner-br"></div>
	</div>

	<!-- Edge borders -->
	<div class="frame-edges" aria-hidden="true">
		<div class="edge edge-top"></div>
		<div class="edge edge-bottom"></div>
		<div class="edge edge-left"></div>
		<div class="edge edge-right"></div>
	</div>

	<!-- Content area -->
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
		padding: var(--frame-border-width);
	}

	/* Panel variant - for sidebars and sections */
	.page-frame.panel {
		padding: var(--space-3);
	}

	/* Modal variant - for dialogs */
	.page-frame.modal {
		padding: var(--space-4);
		max-width: 90vw;
		max-height: 90vh;
		overflow: auto;
	}

	/* Card variant - for smaller content blocks */
	.page-frame.card {
		padding: var(--space-2);
	}

	/* Frame corners */
	.frame-corners {
		position: absolute;
		inset: 0;
		pointer-events: none;
		overflow: hidden;
		z-index: var(--z-raised);
	}

	.corner {
		position: absolute;
		width: var(--frame-corner-size);
		height: var(--frame-corner-size);
		background-image: url('/borders/corner-ornament.svg');
		background-size: contain;
		background-repeat: no-repeat;
	}

	.corner-tl {
		top: 0;
		left: 0;
	}

	.corner-tr {
		top: 0;
		right: 0;
		transform: scaleX(-1);
	}

	.corner-bl {
		bottom: 0;
		left: 0;
		transform: scaleY(-1);
	}

	.corner-br {
		bottom: 0;
		right: 0;
		transform: scale(-1, -1);
	}

	/* Frame edges */
	.frame-edges {
		position: absolute;
		inset: 0;
		pointer-events: none;
		z-index: var(--z-base);
	}

	.edge {
		position: absolute;
		background-repeat: repeat;
	}

	.edge-top {
		top: 0;
		left: var(--frame-corner-size);
		right: var(--frame-corner-size);
		height: var(--frame-border-width);
		background-image: url('/borders/border-horizontal.svg');
		background-size: auto 100%;
	}

	.edge-bottom {
		bottom: 0;
		left: var(--frame-corner-size);
		right: var(--frame-corner-size);
		height: var(--frame-border-width);
		background-image: url('/borders/border-horizontal.svg');
		background-size: auto 100%;
		transform: scaleY(-1);
	}

	.edge-left {
		top: var(--frame-corner-size);
		bottom: var(--frame-corner-size);
		left: 0;
		width: var(--frame-border-width);
		background-image: url('/borders/border-vertical.svg');
		background-size: 100% auto;
		background-repeat: repeat-y;
	}

	.edge-right {
		top: var(--frame-corner-size);
		bottom: var(--frame-corner-size);
		right: 0;
		width: var(--frame-border-width);
		background-image: url('/borders/border-vertical.svg');
		background-size: 100% auto;
		background-repeat: repeat-y;
		transform: scaleX(-1);
	}

	/* Content area */
	.frame-content {
		position: relative;
		z-index: var(--z-base);
		padding: var(--frame-padding);
		min-height: 100%;
	}

	.full .frame-content {
		padding: calc(var(--frame-border-width) + var(--frame-padding));
	}

	/* Panel variant - simplified borders */
	.panel .frame-corners,
	.panel .frame-edges {
		display: none;
	}

	.panel {
		border: 2px solid var(--ornament-dark);
		border-radius: var(--radius-sm);
	}

	.panel::before {
		content: '';
		position: absolute;
		inset: 2px;
		border: 1px solid var(--ornament-gold);
		border-radius: var(--radius-sm);
		pointer-events: none;
	}

	/* Card variant - even simpler */
	.card .frame-corners,
	.card .frame-edges {
		display: none;
	}

	.card {
		border: 1px solid var(--parchment-shadow);
		border-radius: var(--radius-sm);
		box-shadow: var(--shadow-sm);
	}

	/* Modal variant */
	.modal {
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-xl);
	}

	/* Responsive adjustments */
	@media (max-width: 768px) {
		.corner {
			width: var(--frame-corner-size-mobile);
			height: var(--frame-corner-size-mobile);
		}

		.edge-top,
		.edge-bottom {
			left: var(--frame-corner-size-mobile);
			right: var(--frame-corner-size-mobile);
			height: var(--frame-border-width-mobile);
		}

		.edge-left,
		.edge-right {
			top: var(--frame-corner-size-mobile);
			bottom: var(--frame-corner-size-mobile);
			width: var(--frame-border-width-mobile);
		}

		.frame-content {
			padding: var(--frame-padding-mobile);
		}

		.full .frame-content {
			padding: calc(var(--frame-border-width-mobile) + var(--frame-padding-mobile));
		}
	}

	/* Mobile: simplify to CSS-only borders */
	@media (max-width: 480px) {
		.full .frame-corners,
		.full .frame-edges {
			display: none;
		}

		.full {
			border: 3px solid var(--ornament-dark);
			padding: var(--space-2);
		}

		.full::before {
			content: '';
			position: absolute;
			inset: 3px;
			border: 1px solid var(--ornament-gold);
			pointer-events: none;
		}

		.full .frame-content {
			padding: var(--space-4);
		}
	}
</style>
