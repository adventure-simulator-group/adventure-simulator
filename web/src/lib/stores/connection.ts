// Connection State Store
import { writable, derived } from 'svelte/store';
import type { ConnectionState, ConnectionStatus } from '$lib/spacetimedb/types';
import { onConnectionChange, initConnection, disconnect } from '$lib/spacetimedb/client';

// Create the writable store
function createConnectionStore() {
	const { subscribe, set, update } = writable<ConnectionState>({
		status: 'disconnected',
		identity: null,
		error: null
	});

	// Subscribe to client connection changes
	onConnectionChange((state) => {
		set(state);
	});

	return {
		subscribe,
		connect: initConnection,
		disconnect,
		setError: (error: string) => update((s) => ({ ...s, error, status: 'error' })),
		clearError: () => update((s) => ({ ...s, error: null }))
	};
}

export const connection = createConnectionStore();

// Derived stores for convenience
export const isConnected = derived(connection, ($conn) => $conn.status === 'connected');

export const isConnecting = derived(connection, ($conn) => $conn.status === 'connecting');

export const connectionError = derived(connection, ($conn) => $conn.error);

export const identity = derived(connection, ($conn) => $conn.identity);
