<script lang="ts">
	import type { User } from '@workos/authkit-sveltekit';

	interface Props {
		user: User | null;
		signInUrl: string;
		signUpUrl: string;
	}

	let { user, signInUrl, signUpUrl }: Props = $props();

	// Display name priority: WorkOS name > email prefix
	const displayName = $derived(() => {
		if (user) {
			if (user.firstName) {
				return user.lastName ? `${user.firstName} ${user.lastName}` : user.firstName;
			}
			return user.email.split('@')[0];
		}
		return 'Adventurer';
	});
</script>

<div class="auth-panel">
	{#if user}
		<!-- Authenticated state -->
		<div class="user-info">
			<span class="display-name">{displayName()}</span>
			<span class="email">{user.email}</span>
		</div>
		<div class="auth-buttons">
			<a href="/auth/signout" class="auth-btn signout">Sign Out</a>
		</div>
	{:else}
		<!-- Guest state -->
		<div class="guest-info">
			<span class="display-name">Guest Adventurer</span>
			<span class="hint">Sign in to save your progress</span>
		</div>
		<div class="auth-buttons">
			<a href={signInUrl} class="auth-btn signin">Sign In</a>
			<a href={signUpUrl} class="auth-btn signup">Sign Up</a>
		</div>
	{/if}
</div>

<style>
	.auth-panel {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-3);
		background: var(--parchment-base);
		border: 1px solid var(--parchment-shadow);
		border-radius: var(--radius-md);
		font-family: var(--font-body);
		min-width: 180px;
		box-shadow: var(--shadow-md);
	}

	.user-info,
	.guest-info {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.display-name {
		color: var(--ink-brown);
		font-weight: 600;
		font-size: var(--text-base);
		font-family: var(--font-display);
	}

	.email {
		color: var(--ink-faded);
		font-size: var(--text-sm);
	}

	.hint {
		color: var(--ink-faded);
		font-size: var(--text-xs);
		font-style: italic;
	}

	.auth-buttons {
		display: flex;
		gap: var(--space-2);
		margin-top: var(--space-1);
	}

	.auth-btn {
		padding: var(--space-2) var(--space-3);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
		font-weight: 500;
		font-family: var(--font-display);
		text-decoration: none;
		text-align: center;
		cursor: pointer;
		transition: all var(--duration-fast) var(--ease-out);
		border: 1px solid transparent;
	}

	.auth-btn.signin {
		background: var(--ornament-green);
		color: var(--parchment-lightest);
		border-color: var(--ornament-green-dark);
	}

	.auth-btn.signin:hover {
		background: var(--ornament-green-light);
	}

	.auth-btn.signup {
		background: var(--parchment-medium);
		color: var(--ink-brown);
		border-color: var(--parchment-shadow);
	}

	.auth-btn.signup:hover {
		background: var(--parchment-dark);
	}

	.auth-btn.signout {
		background: var(--ornament-red-muted);
		color: var(--parchment-lightest);
		border-color: var(--ornament-red-dark);
	}

	.auth-btn.signout:hover {
		background: var(--ornament-red);
	}
</style>
