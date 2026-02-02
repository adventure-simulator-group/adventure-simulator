// Chat State Store
import { writable, derived } from 'svelte/store';
import type { ChatMessage, ChatChannel } from '$lib/spacetimedb/types';

interface ChatState {
	messages: ChatMessage[];
	activeChannel: ChatChannel;
	unreadCount: Record<ChatChannel, number>;
	isOpen: boolean;
}

const MAX_MESSAGES = 100; // Keep last 100 messages per channel

function createChatStore() {
	const { subscribe, set, update } = writable<ChatState>({
		messages: [],
		activeChannel: 'global',
		unreadCount: {
			global: 0,
			party: 0,
			local: 0,
			whisper: 0
		},
		isOpen: false
	});

	return {
		subscribe,

		// Add a new message
		addMessage: (message: ChatMessage) =>
			update((s) => {
				const messages = [...s.messages, message].slice(-MAX_MESSAGES);
				const unreadCount = { ...s.unreadCount };

				// Increment unread if not in active channel or chat is closed
				if (!s.isOpen || message.channel !== s.activeChannel) {
					unreadCount[message.channel]++;
				}

				return { ...s, messages, unreadCount };
			}),

		// Set active channel
		setActiveChannel: (channel: ChatChannel) =>
			update((s) => ({
				...s,
				activeChannel: channel,
				unreadCount: {
					...s.unreadCount,
					[channel]: 0 // Clear unread when switching to channel
				}
			})),

		// Toggle chat open/closed
		toggleChat: () =>
			update((s) => {
				const isOpen = !s.isOpen;
				return {
					...s,
					isOpen,
					// Clear unread for active channel when opening
					unreadCount: isOpen
						? { ...s.unreadCount, [s.activeChannel]: 0 }
						: s.unreadCount
				};
			}),

		// Open chat
		openChat: () =>
			update((s) => ({
				...s,
				isOpen: true,
				unreadCount: { ...s.unreadCount, [s.activeChannel]: 0 }
			})),

		// Close chat
		closeChat: () =>
			update((s) => ({ ...s, isOpen: false })),

		// Clear messages for a channel
		clearChannel: (channel: ChatChannel) =>
			update((s) => ({
				...s,
				messages: s.messages.filter((m) => m.channel !== channel),
				unreadCount: { ...s.unreadCount, [channel]: 0 }
			})),

		// Clear all messages
		clearAll: () =>
			update((s) => ({
				...s,
				messages: [],
				unreadCount: { global: 0, party: 0, local: 0, whisper: 0 }
			})),

		// Reset store
		reset: () =>
			set({
				messages: [],
				activeChannel: 'global',
				unreadCount: { global: 0, party: 0, local: 0, whisper: 0 },
				isOpen: false
			})
	};
}

export const chat = createChatStore();

// Derived stores
export const chatMessages = derived(chat, ($chat) => $chat.messages);

export const activeChannelMessages = derived(chat, ($chat) =>
	$chat.messages.filter((m) => m.channel === $chat.activeChannel)
);

export const activeChannel = derived(chat, ($chat) => $chat.activeChannel);

export const totalUnread = derived(chat, ($chat) =>
	Object.values($chat.unreadCount).reduce((sum, count) => sum + count, 0)
);

export const isChatOpen = derived(chat, ($chat) => $chat.isOpen);

export const channelUnread = derived(chat, ($chat) => $chat.unreadCount);
