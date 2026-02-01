import { configureAuthKit, authKitHandle } from '@workos/authkit-sveltekit';
import { env } from '$env/dynamic/private';
import { PUBLIC_APP_URL } from '$env/static/public';

// Configure AuthKit with environment variables
if (env.WORKOS_CLIENT_ID && env.WORKOS_API_KEY) {
	configureAuthKit({
		clientId: env.WORKOS_CLIENT_ID,
		apiKey: env.WORKOS_API_KEY,
		redirectUri: env.WORKOS_REDIRECT_URI || `${PUBLIC_APP_URL || 'http://localhost:5173'}/auth/callback`,
		cookiePassword: env.WORKOS_COOKIE_PASSWORD || 'development-cookie-password-min-32-chars!'
	});
}

export const handle = authKitHandle();
