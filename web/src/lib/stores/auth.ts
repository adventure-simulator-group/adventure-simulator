import { writable, derived } from 'svelte/store';
import { browser } from '$app/environment';

// WorkOS user info from page data
export const workosUser = writable<{
	id: string;
	email: string;
	firstName: string | null;
	lastName: string | null;
} | null>(null);

// Sign-in URLs from page data
export const signInUrl = writable<string | null>(null);
export const signUpUrl = writable<string | null>(null);

// Derived: is user authenticated with WorkOS?
export const isAuthenticated = derived(workosUser, ($user) => $user !== null);

// Derived: display name
export const displayName = derived(workosUser, ($user) => {
	if (!$user) return null;
	if ($user.firstName && $user.lastName) {
		return `${$user.firstName} ${$user.lastName}`;
	}
	if ($user.firstName) return $user.firstName;
	return $user.email.split('@')[0];
});

// Pending auth for SpacetimeDB linking
export const pendingAuth = writable<{
	workosUserId: string;
	email: string;
} | null>(null);

// Set pending auth (called when WorkOS user is available)
export function setPendingAuth(workosUserId: string, email: string) {
	pendingAuth.set({ workosUserId, email });
}

// Clear pending auth (called on sign out)
export function clearPendingAuth() {
	pendingAuth.set(null);
}

// Sign out - redirect to sign out endpoint
export function signOut() {
	if (browser) {
		clearPendingAuth();
		window.location.href = '/auth/signout';
	}
}
