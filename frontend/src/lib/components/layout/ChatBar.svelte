<script lang="ts">
	import {
		chat,
		activeChannelMessages,
		activeChannel,
		totalUnread,
		isChatOpen,
		channelUnread
	} from '$lib/stores/chat';
	import { currentCharacter } from '$lib/stores/character';
	import { sendChatMessage } from '$lib/spacetimedb/reducers';
	import type { ChatChannel, ChatMessage } from '$lib/spacetimedb/types';

	let messageInput = $state('');
	let messagesContainer: HTMLElement;

	const channels: { id: ChatChannel; label: string }[] = [
		{ id: 'global', label: 'Global' },
		{ id: 'party', label: 'Party' },
		{ id: 'local', label: 'Local' }
	];

	async function handleSend() {
		if (!messageInput.trim() || !$currentCharacter) return;

		const content = messageInput.trim();
		messageInput = '';

		// Add message optimistically
		const senderId = String($currentCharacter.id);
		const message: ChatMessage = {
			id: crypto.randomUUID(),
			senderId,
			senderName: $currentCharacter.name,
			content,
			timestamp: Date.now(),
			channel: $activeChannel
		};
		chat.addMessage(message);

		// Send to server
		await sendChatMessage(senderId, content, $activeChannel);

		// Scroll to bottom
		requestAnimationFrame(() => {
			if (messagesContainer) {
				messagesContainer.scrollTop = messagesContainer.scrollHeight;
			}
		});
	}

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Enter' && !event.shiftKey) {
			event.preventDefault();
			handleSend();
		}
	}

	function formatTime(timestamp: number): string {
		const date = new Date(timestamp);
		return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
	}

	function selectChannel(channel: ChatChannel) {
		chat.setActiveChannel(channel);
	}
</script>

<div class="chat-bar" class:open={$isChatOpen}>
	<!-- Collapsed bar (always visible) -->
	<button class="chat-toggle" onclick={() => chat.toggleChat()} aria-expanded={$isChatOpen}>
		<span class="chat-icon">💬</span>
		<span class="chat-label">Chat</span>
		{#if $totalUnread > 0}
			<span class="unread-badge">{$totalUnread > 99 ? '99+' : $totalUnread}</span>
		{/if}
	</button>

	<!-- Expanded chat panel -->
	{#if $isChatOpen}
		<div class="chat-panel">
			<!-- Channel tabs -->
			<div class="channel-tabs">
				{#each channels as channel}
					<button
						class="channel-tab"
						class:active={$activeChannel === channel.id}
						onclick={() => selectChannel(channel.id)}
					>
						{channel.label}
						{#if $channelUnread[channel.id] > 0}
							<span class="channel-unread">{$channelUnread[channel.id]}</span>
						{/if}
					</button>
				{/each}
			</div>

			<!-- Messages -->
			<div class="messages-container" bind:this={messagesContainer}>
				{#if $activeChannelMessages.length === 0}
					<div class="no-messages">
						<span class="no-messages-text">No messages yet</span>
					</div>
				{:else}
					{#each $activeChannelMessages as message}
						<div
							class="message"
							class:own={message.senderId === String($currentCharacter?.id)}
						>
							<span class="message-sender">{message.senderName}</span>
							<span class="message-content">{message.content}</span>
							<span class="message-time">{formatTime(message.timestamp)}</span>
						</div>
					{/each}
				{/if}
			</div>

			<!-- Input area -->
			<div class="input-area">
				<input
					type="text"
					class="message-input"
					placeholder="Type a message..."
					bind:value={messageInput}
					onkeydown={handleKeydown}
				/>
				<button
					class="send-button"
					onclick={handleSend}
					disabled={!messageInput.trim()}
					aria-label="Send message"
				>
					<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
						<path d="M2 21l21-9L2 3v7l15 2-15 2v7z" />
					</svg>
				</button>
			</div>
		</div>
	{/if}
</div>

<style>
	.chat-bar {
		position: fixed;
		bottom: 0;
		left: 0;
		right: 0;
		z-index: var(--z-sticky);
		display: flex;
		flex-direction: column;
		pointer-events: none;
	}

	.chat-bar > * {
		pointer-events: auto;
	}

	/* Toggle button */
	.chat-toggle {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		align-self: flex-start;
		margin: 0 var(--space-4) var(--space-2);
		padding: var(--space-2) var(--space-4);
		background-color: var(--parchment-dark);
		border: 2px solid var(--ornament-dark);
		border-radius: var(--radius-full);
		color: var(--ink-dark);
		font-family: var(--font-stats);
		font-size: var(--text-sm);
		box-shadow: var(--shadow-md);
		transition: all var(--duration-fast) var(--ease-out);
	}

	.chat-toggle:hover {
		background-color: var(--parchment-base);
		border-color: var(--ornament-gold);
	}

	.chat-icon {
		font-size: var(--text-lg);
	}

	.unread-badge {
		display: flex;
		align-items: center;
		justify-content: center;
		min-width: 20px;
		height: 20px;
		padding: 0 var(--space-1);
		background-color: var(--ink-red);
		color: var(--parchment-lightest);
		font-size: var(--text-xs);
		font-weight: var(--font-bold);
		border-radius: var(--radius-full);
	}

	/* Chat panel */
	.chat-panel {
		display: flex;
		flex-direction: column;
		max-height: 400px;
		background-color: var(--parchment-light);
		border: 2px solid var(--ornament-dark);
		border-bottom: none;
		border-radius: var(--radius-lg) var(--radius-lg) 0 0;
		box-shadow: var(--shadow-lg);
		overflow: hidden;
	}

	/* Channel tabs */
	.channel-tabs {
		display: flex;
		background-color: var(--parchment-dark);
		border-bottom: 1px solid var(--parchment-shadow);
	}

	.channel-tab {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-3);
		font-family: var(--font-display);
		font-size: var(--text-xs);
		font-weight: var(--font-semibold);
		text-transform: uppercase;
		letter-spacing: var(--tracking-wider);
		color: var(--ink-medium);
		border-bottom: 2px solid transparent;
		transition: all var(--duration-fast) var(--ease-out);
	}

	.channel-tab:hover {
		color: var(--ink-dark);
		background-color: var(--parchment-base);
	}

	.channel-tab.active {
		color: var(--ink-black);
		border-bottom-color: var(--ornament-gold);
		background-color: var(--parchment-light);
	}

	.channel-unread {
		display: flex;
		align-items: center;
		justify-content: center;
		min-width: 16px;
		height: 16px;
		padding: 0 4px;
		background-color: var(--ink-red);
		color: var(--parchment-lightest);
		font-size: 10px;
		border-radius: var(--radius-full);
	}

	/* Messages container */
	.messages-container {
		flex: 1;
		overflow-y: auto;
		padding: var(--space-3);
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-height: 200px;
		max-height: 280px;
	}

	.no-messages {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.no-messages-text {
		font-family: var(--font-stats);
		font-style: italic;
		color: var(--ink-faded);
	}

	.message {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1);
		padding: var(--space-2);
		background-color: var(--parchment-base);
		border-radius: var(--radius-sm);
		font-size: var(--text-sm);
	}

	.message.own {
		background-color: var(--parchment-medium);
	}

	.message-sender {
		font-family: var(--font-stats);
		font-weight: var(--font-semibold);
		color: var(--ink-dark);
	}

	.message-content {
		flex: 1;
		min-width: 0;
		color: var(--ink-brown);
		word-break: break-word;
	}

	.message-time {
		font-family: var(--font-stats);
		font-size: var(--text-xs);
		color: var(--ink-faded);
		margin-left: auto;
	}

	/* Input area */
	.input-area {
		display: flex;
		gap: var(--space-2);
		padding: var(--space-3);
		background-color: var(--parchment-dark);
		border-top: 1px solid var(--parchment-shadow);
	}

	.message-input {
		flex: 1;
		padding: var(--space-2) var(--space-3);
		font-family: var(--font-body);
		font-size: var(--text-sm);
	}

	.send-button {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 40px;
		height: 40px;
		background-color: var(--ornament-gold);
		border-radius: var(--radius-sm);
		color: var(--ink-black);
		transition: all var(--duration-fast) var(--ease-out);
	}

	.send-button:hover:not(:disabled) {
		background-color: var(--ornament-gold-light);
		transform: scale(1.05);
	}

	.send-button:disabled {
		background-color: var(--parchment-shadow);
		color: var(--ink-faded);
		cursor: not-allowed;
	}

	/* Mobile adjustments */
	@media (max-width: 768px) {
		.chat-bar {
			bottom: calc(60px + env(safe-area-inset-bottom));
		}

		.chat-panel {
			max-height: 50vh;
		}

		.messages-container {
			min-height: 150px;
			max-height: 200px;
		}
	}

	/* When open, hide toggle on desktop */
	@media (min-width: 769px) {
		.chat-bar.open .chat-toggle {
			display: none;
		}

		.chat-panel {
			margin: 0 var(--space-4);
			max-width: 400px;
		}
	}
</style>
