<script lang="ts">
	import { page } from '$app/stores';

	interface Tab {
		label: string;
		href: string;
		icon?: string;
	}

	const tabs: Tab[] = [
		{ label: 'Character', href: '/character' },
		{ label: 'Party', href: '/party' },
		{ label: 'Settlement', href: '/settlement' },
		{ label: 'Quests', href: '/quests' }
	];

	function isActive(href: string, currentPath: string): boolean {
		if (href === '/settlement') {
			return currentPath === '/' || currentPath.startsWith('/settlement');
		}
		return currentPath.startsWith(href);
	}
</script>

<nav class="tab-nav" aria-label="Main navigation">
	<ul class="tab-list">
		{#each tabs as tab}
			<li class="tab-item">
				<a
					href={tab.href}
					class="tab-link"
					class:active={isActive(tab.href, $page.url.pathname)}
					aria-current={isActive(tab.href, $page.url.pathname) ? 'page' : undefined}
				>
					{tab.label}
				</a>
			</li>
		{/each}
	</ul>
</nav>

<style>
	.tab-nav {
		background-color: var(--parchment-medium);
	}

	.tab-list {
		display: flex;
		justify-content: center;
		gap: var(--space-1);
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.tab-item {
		margin: 0;
	}

	.tab-link {
		display: block;
		padding: var(--space-3) var(--space-6);
		font-family: var(--font-display);
		font-size: var(--text-sm);
		font-weight: var(--font-semibold);
		letter-spacing: var(--tracking-widest);
		text-transform: uppercase;
		text-decoration: none;
		color: var(--ink-medium);
		border-bottom: 3px solid transparent;
		transition: all var(--duration-fast) var(--ease-out);
	}

	.tab-link:hover {
		color: var(--ink-dark);
		background-color: var(--parchment-light);
	}

	.tab-link.active {
		color: var(--ink-black);
		border-bottom-color: var(--ornament-gold);
		background-color: var(--parchment-light);
	}

	/* Responsive: Bottom navigation on mobile */
	@media (max-width: 768px) {
		.tab-nav {
			position: fixed;
			bottom: 0;
			left: 0;
			right: 0;
			z-index: var(--z-sticky);
			background-color: var(--parchment-dark);
			border-top: 2px solid var(--ornament-dark);
			box-shadow: 0 -2px 8px rgba(44, 36, 22, 0.15);
			padding-bottom: env(safe-area-inset-bottom);
		}

		.tab-list {
			justify-content: space-around;
		}

		.tab-link {
			padding: var(--space-3) var(--space-4);
			font-size: var(--text-xs);
			border-bottom: none;
			border-top: 3px solid transparent;
		}

		.tab-link.active {
			border-bottom: none;
			border-top-color: var(--ornament-gold);
		}
	}
</style>
