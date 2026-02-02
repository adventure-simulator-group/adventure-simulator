import { env } from '$env/dynamic/private';
import { json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';

// Handle OAuth callback from WorkOS
// This exchanges the authorization code for a session
// We dynamically import authKit to avoid initialization issues during SSR build
export const GET: RequestHandler = async (event) => {
	// Check if WorkOS is configured
	if (!env.WORKOS_CLIENT_ID || !env.WORKOS_API_KEY) {
		return json({ error: 'WorkOS not configured' }, { status: 503 });
	}

	// Dynamically import to ensure hooks.server.ts has already configured authKit
	const { authKit } = await import('@workos/authkit-sveltekit');
	const handler = authKit.handleCallback();
	return handler(event);
};
