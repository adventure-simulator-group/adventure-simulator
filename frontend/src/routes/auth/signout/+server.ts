import { authKit } from '@workos/authkit-sveltekit';
import type { RequestHandler } from './$types';

// Handle sign out - clears session and redirects to home
export const GET: RequestHandler = async (event) => {
	return authKit.signOut(event);
};
