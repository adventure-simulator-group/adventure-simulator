/**
 * SpacetimeDB Client Integration
 *
 * Handles connection to SpacetimeDB maincloud and provides callbacks
 * for real-time data updates from table subscriptions.
 *
 * Note: After running `spacetime generate`, uncomment the actual SDK imports
 * and connection code. For now, this provides mock functionality.
 */

import { browser } from '$app/environment';
import { PUBLIC_SPACETIMEDB_URI, PUBLIC_SPACETIMEDB_MODULE } from '$env/static/public';
import type { ConnectionState, ConnectionStatus } from './types';

// Configuration
const STDB_URI = PUBLIC_SPACETIMEDB_URI || 'ws://localhost:3000';
const STDB_MODULE = PUBLIC_SPACETIMEDB_MODULE || 'adventure-simulator';
const TOKEN_KEY = 'stdb_token';

// Connection state
let connectionState: ConnectionState = {
	status: 'disconnected',
	identity: null,
	error: null
};

// Placeholder for actual connection - will be populated after generate
// import { DbConnection } from '../spacetimedb-generated';
let connection: unknown = null;

// Event callbacks
type ConnectionCallback = (state: ConnectionState) => void;
const connectionCallbacks: Set<ConnectionCallback> = new Set();

/**
 * Subscribe to connection state changes
 */
export function onConnectionChange(callback: ConnectionCallback): () => void {
	connectionCallbacks.add(callback);
	callback(connectionState);
	return () => connectionCallbacks.delete(callback);
}

/**
 * Update connection state and notify subscribers
 */
function setConnectionState(newState: Partial<ConnectionState>) {
	connectionState = { ...connectionState, ...newState };
	connectionCallbacks.forEach((cb) => cb(connectionState));
}

/**
 * Initialize the SpacetimeDB client connection
 */
export async function initConnection(): Promise<void> {
	if (!browser) return;

	if (connectionState.status === 'connected') {
		return;
	}

	setConnectionState({ status: 'connecting', error: null });

	try {
		const token = localStorage.getItem(TOKEN_KEY) || undefined;

		// TODO: After running `spacetime generate`, uncomment this:
		// connection = DbConnection.builder()
		//     .withUri(STDB_URI)
		//     .withModuleName(STDB_MODULE)
		//     .withToken(token)
		//     .onConnect((conn, identity, newToken) => {
		//         localStorage.setItem(TOKEN_KEY, newToken);
		//         setConnectionState({
		//             status: 'connected',
		//             identity: identity.toHexString(),
		//             error: null
		//         });
		//         subscribeToTables(conn, identity);
		//     })
		//     .onDisconnect(() => {
		//         setConnectionState({ status: 'disconnected' });
		//     })
		//     .onConnectError((error) => {
		//         setConnectionState({
		//             status: 'error',
		//             error: error.message
		//         });
		//     })
		//     .build();

		// For now, simulate connection with mock
		await new Promise((resolve) => setTimeout(resolve, 500));

		const mockIdentity = token || generateMockIdentity();
		if (!token) {
			localStorage.setItem(TOKEN_KEY, mockIdentity);
		}

		setConnectionState({
			status: 'connected',
			identity: mockIdentity,
			error: null
		});

		console.log(`[SpacetimeDB] Connected to ${STDB_URI}/${STDB_MODULE}`);
		console.log(`[SpacetimeDB] Identity: ${mockIdentity.slice(0, 16)}...`);
	} catch (error) {
		const errorMessage = error instanceof Error ? error.message : 'Unknown connection error';
		setConnectionState({
			status: 'error',
			error: errorMessage
		});
		console.error('[SpacetimeDB] Connection failed:', errorMessage);
		throw error;
	}
}

/**
 * Disconnect from SpacetimeDB
 */
export function disconnect(): void {
	if (connection) {
		// (connection as DbConnection).disconnect();
		connection = null;
	}
	setConnectionState({
		status: 'disconnected',
		identity: null,
		error: null
	});
	console.log('[SpacetimeDB] Disconnected');
}

/**
 * Get current connection state
 */
export function getConnectionState(): ConnectionState {
	return { ...connectionState };
}

/**
 * Check if connected
 */
export function isConnected(): boolean {
	return connectionState.status === 'connected';
}

/**
 * Get the underlying connection for advanced operations
 */
export function getConnection(): unknown {
	return connection;
}

/**
 * Generate a mock identity for development
 */
function generateMockIdentity(): string {
	const chars = 'abcdef0123456789';
	let id = '';
	for (let i = 0; i < 64; i++) {
		id += chars[Math.floor(Math.random() * chars.length)];
	}
	return id;
}

// ============================================================================
// Table subscription helpers
// ============================================================================

// Type for table event callbacks
type RowCallback<T> = (row: T) => void;
type UpdateCallback<T> = (oldRow: T, newRow: T) => void;

interface TableSubscription<T> {
	onInsert: (callback: RowCallback<T>) => () => void;
	onUpdate: (callback: UpdateCallback<T>) => () => void;
	onDelete: (callback: RowCallback<T>) => () => void;
	getAll: () => T[];
	getById: (id: bigint | string) => T | undefined;
}

// Mock data storage for development
const mockTables: Map<string, Map<string, unknown>> = new Map();

/**
 * Create a table subscription for a specific table
 * In production, this will use SpacetimeDB's subscription system
 */
export function createTableSubscription<T extends { id: bigint | string }>(
	tableName: string
): TableSubscription<T> {
	const insertCallbacks: Set<RowCallback<T>> = new Set();
	const updateCallbacks: Set<UpdateCallback<T>> = new Set();
	const deleteCallbacks: Set<RowCallback<T>> = new Set();

	// Initialize table storage
	if (!mockTables.has(tableName)) {
		mockTables.set(tableName, new Map());
	}
	const tableData = mockTables.get(tableName)!;

	return {
		onInsert(callback: RowCallback<T>) {
			insertCallbacks.add(callback);
			return () => insertCallbacks.delete(callback);
		},

		onUpdate(callback: UpdateCallback<T>) {
			updateCallbacks.add(callback);
			return () => updateCallbacks.delete(callback);
		},

		onDelete(callback: RowCallback<T>) {
			deleteCallbacks.add(callback);
			return () => deleteCallbacks.delete(callback);
		},

		getAll(): T[] {
			return Array.from(tableData.values()) as T[];
		},

		getById(id: bigint | string): T | undefined {
			return tableData.get(String(id)) as T | undefined;
		}
	};
}

// ============================================================================
// Reducer calling
// ============================================================================

type ReducerResult = { success: boolean; error?: string };

/**
 * Call a reducer on the SpacetimeDB server
 * In production, this sends BSATN-encoded requests
 */
export async function callReducer(name: string, args: unknown[]): Promise<ReducerResult> {
	if (!isConnected()) {
		return { success: false, error: 'Not connected to server' };
	}

	console.log(`[SpacetimeDB] Calling reducer: ${name}`, args);

	// TODO: In production, this will use:
	// await (connection as DbConnection).reducers[name](...args);

	// Simulate network delay
	await new Promise((resolve) => setTimeout(resolve, 100));

	return { success: true };
}

// Export configuration for debugging
export const config = {
	uri: STDB_URI,
	module: STDB_MODULE
};
