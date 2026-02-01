import type { Handle } from '@sveltejs/kit';
import { env } from '$env/dynamic/private';
import { PUBLIC_APP_URL } from '$env/static/public';

// Only import and configure WorkOS if credentials are present
const workosConfigured = !!(env.WORKOS_CLIENT_ID && env.WORKOS_API_KEY);

let authHandle: Handle | null = null;

if (workosConfigured) {
	// Dynamic import to avoid errors when not configured
	const { configureAuthKit, authKitHandle } = await import('@workos/authkit-sveltekit');

	configureAuthKit({
		clientId: env.WORKOS_CLIENT_ID!,
		apiKey: env.WORKOS_API_KEY!,
		redirectUri: env.WORKOS_REDIRECT_URI || `${PUBLIC_APP_URL || 'http://localhost:5173'}/auth/callback`,
		cookiePassword: env.WORKOS_COOKIE_PASSWORD || 'development-cookie-password-min-32-chars!'
	});

	authHandle = authKitHandle();
}

export const handle: Handle = async ({ event, resolve }) => {
	if (authHandle) {
		return authHandle({ event, resolve });
	}
	// Pass through without auth when WorkOS not configured
	return resolve(event);
};
