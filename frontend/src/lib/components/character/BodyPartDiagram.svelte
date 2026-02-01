<script lang="ts">
	import '$lib/styles/compat.css';
	import type { CharacterBodyPart } from '$lib/spacetimedb/types';

	interface Props {
		bodyParts: CharacterBodyPart[];
	}

	let { bodyParts }: Props = $props();

	let hoveredPart = $state<string | null>(null);
	let selectedPart = $state<string | null>(null);

	function getHealthColor(health: number, maxHealth: number): string {
		const ratio = maxHealth > 0 ? health / maxHealth : 0;
		if (ratio > 0.7) return 'var(--color-health-full)';
		if (ratio > 0.3) return 'var(--color-health-medium)';
		if (ratio > 0) return 'var(--color-health-critical)';
		return 'var(--color-health-destroyed)';
	}

	function getPartHealth(type: string): { current: number; max: number; ratio: number } {
		const part = bodyParts.find((p) => p.partType === type);
		if (!part) return { current: 100, max: 100, ratio: 1 };
		const ratio = part.maxHealth > 0 ? part.currentHealth / part.maxHealth : 0;
		return { current: part.currentHealth, max: part.maxHealth, ratio };
	}

	function formatPartName(type: string): string {
		return type.replace(/([A-Z])/g, ' $1').trim();
	}

	function handlePartClick(type: string) {
		selectedPart = selectedPart === type ? null : type;
	}
</script>

<div class="body-diagram">
	<svg viewBox="0 0 200 400" class="body-svg">
		<!-- Head -->
		<ellipse
			cx="100"
			cy="40"
			rx="30"
			ry="35"
			class="body-part"
			class:hovered={hoveredPart === 'Head'}
			class:selected={selectedPart === 'Head'}
			style="fill: {getHealthColor(getPartHealth('Head').current, getPartHealth('Head').max)}"
			onmouseenter={() => (hoveredPart = 'Head')}
			onmouseleave={() => (hoveredPart = null)}
			onclick={() => handlePartClick('Head')}
			role="button"
			tabindex="0"
			onkeydown={(e) => e.key === 'Enter' && handlePartClick('Head')}
		/>

		<!-- Neck connector -->
		<rect x="90" y="75" width="20" height="15" fill="var(--color-border-strong)" />

		<!-- Chest -->
		<path
			d="M 60 90 Q 60 85 70 85 L 130 85 Q 140 85 140 90 L 145 160 Q 145 170 135 170 L 65 170 Q 55 170 55 160 Z"
			class="body-part"
			class:hovered={hoveredPart === 'Chest'}
			class:selected={selectedPart === 'Chest'}
			style="fill: {getHealthColor(getPartHealth('Chest').current, getPartHealth('Chest').max)}"
			onmouseenter={() => (hoveredPart = 'Chest')}
			onmouseleave={() => (hoveredPart = null)}
			onclick={() => handlePartClick('Chest')}
			role="button"
			tabindex="0"
			onkeydown={(e) => e.key === 'Enter' && handlePartClick('Chest')}
		/>

		<!-- Stomach -->
		<path
			d="M 65 170 L 135 170 L 130 240 Q 125 250 100 250 Q 75 250 70 240 Z"
			class="body-part"
			class:hovered={hoveredPart === 'Stomach'}
			class:selected={selectedPart === 'Stomach'}
			style="fill: {getHealthColor(getPartHealth('Stomach').current, getPartHealth('Stomach').max)}"
			onmouseenter={() => (hoveredPart = 'Stomach')}
			onmouseleave={() => (hoveredPart = null)}
			onclick={() => handlePartClick('Stomach')}
			role="button"
			tabindex="0"
			onkeydown={(e) => e.key === 'Enter' && handlePartClick('Stomach')}
		/>

		<!-- Left Arm -->
		<path
			d="M 55 90 L 40 95 Q 25 100 20 130 L 15 200 Q 12 215 20 220 L 30 215 Q 35 210 35 195 L 40 140 Q 42 125 50 120 L 55 115 Z"
			class="body-part"
			class:hovered={hoveredPart === 'LeftArm'}
			class:selected={selectedPart === 'LeftArm'}
			style="fill: {getHealthColor(getPartHealth('LeftArm').current, getPartHealth('LeftArm').max)}"
			onmouseenter={() => (hoveredPart = 'LeftArm')}
			onmouseleave={() => (hoveredPart = null)}
			onclick={() => handlePartClick('LeftArm')}
			role="button"
			tabindex="0"
			onkeydown={(e) => e.key === 'Enter' && handlePartClick('LeftArm')}
		/>

		<!-- Right Arm -->
		<path
			d="M 145 90 L 160 95 Q 175 100 180 130 L 185 200 Q 188 215 180 220 L 170 215 Q 165 210 165 195 L 160 140 Q 158 125 150 120 L 145 115 Z"
			class="body-part"
			class:hovered={hoveredPart === 'RightArm'}
			class:selected={selectedPart === 'RightArm'}
			style="fill: {getHealthColor(getPartHealth('RightArm').current, getPartHealth('RightArm').max)}"
			onmouseenter={() => (hoveredPart = 'RightArm')}
			onmouseleave={() => (hoveredPart = null)}
			onclick={() => handlePartClick('RightArm')}
			role="button"
			tabindex="0"
			onkeydown={(e) => e.key === 'Enter' && handlePartClick('RightArm')}
		/>

		<!-- Left Leg -->
		<path
			d="M 75 250 Q 70 255 68 270 L 60 350 Q 58 370 50 380 L 45 395 L 65 395 L 70 380 Q 75 370 80 350 L 95 270 Q 97 255 95 250 Z"
			class="body-part"
			class:hovered={hoveredPart === 'LeftLeg'}
			class:selected={selectedPart === 'LeftLeg'}
			style="fill: {getHealthColor(getPartHealth('LeftLeg').current, getPartHealth('LeftLeg').max)}"
			onmouseenter={() => (hoveredPart = 'LeftLeg')}
			onmouseleave={() => (hoveredPart = null)}
			onclick={() => handlePartClick('LeftLeg')}
			role="button"
			tabindex="0"
			onkeydown={(e) => e.key === 'Enter' && handlePartClick('LeftLeg')}
		/>

		<!-- Right Leg -->
		<path
			d="M 125 250 Q 130 255 132 270 L 140 350 Q 142 370 150 380 L 155 395 L 135 395 L 130 380 Q 125 370 120 350 L 105 270 Q 103 255 105 250 Z"
			class="body-part"
			class:hovered={hoveredPart === 'RightLeg'}
			class:selected={selectedPart === 'RightLeg'}
			style="fill: {getHealthColor(getPartHealth('RightLeg').current, getPartHealth('RightLeg').max)}"
			onmouseenter={() => (hoveredPart = 'RightLeg')}
			onmouseleave={() => (hoveredPart = null)}
			onclick={() => handlePartClick('RightLeg')}
			role="button"
			tabindex="0"
			onkeydown={(e) => e.key === 'Enter' && handlePartClick('RightLeg')}
		/>
	</svg>

	<!-- Tooltip -->
	{#if hoveredPart}
		<div class="tooltip">
			<div class="tooltip-header">{formatPartName(hoveredPart)}</div>
			<div class="tooltip-health">
				Health: {Math.round(getPartHealth(hoveredPart).ratio * 100)}%
			</div>
		</div>
	{/if}

	<!-- Selected Part Details -->
	{#if selectedPart}
		{@const health = getPartHealth(selectedPart)}
		<div class="part-details">
			<h4>{formatPartName(selectedPart)}</h4>
			<div class="health-display">
				<span class="label">Health:</span>
				<div class="health-bar">
					<div
						class="health-fill"
						style="width: {health.ratio * 100}%; background: {getHealthColor(health.current, health.max)}"
					></div>
				</div>
				<span class="value">{health.current}/{health.max}</span>
			</div>
			<button class="close-btn" onclick={() => (selectedPart = null)}>Close</button>
		</div>
	{/if}
</div>

<style>
	.body-diagram {
		position: relative;
		display: flex;
		flex-direction: column;
		align-items: center;
		min-width: 180px;
	}

	.body-svg {
		width: 180px;
		height: 360px;
	}

	.body-part {
		cursor: pointer;
		stroke: var(--color-border-strong);
		stroke-width: 2;
		transition: all var(--duration-fast);
		filter: drop-shadow(0 2px 4px rgba(0, 0, 0, 0.3));
	}

	.body-part:hover,
	.body-part.hovered {
		stroke: var(--color-accent-primary);
		stroke-width: 3;
		filter: drop-shadow(0 0 8px var(--color-accent-primary));
	}

	.body-part.selected {
		stroke: var(--color-accent-secondary);
		stroke-width: 4;
		filter: drop-shadow(0 0 12px var(--color-accent-secondary));
	}

	.tooltip {
		position: absolute;
		top: 10px;
		left: 50%;
		transform: translateX(-50%);
		background: var(--parchment-dark);
		border: 1px solid var(--parchment-shadow);
		border-radius: var(--radius-md);
		padding: var(--space-2) var(--space-3);
		pointer-events: none;
		z-index: 10;
		text-align: center;
		box-shadow: var(--shadow-md);
	}

	.tooltip-header {
		font-weight: var(--font-semibold);
		color: var(--ornament-gold);
		font-size: var(--text-sm);
		font-family: var(--font-display);
	}

	.tooltip-health {
		font-size: var(--text-xs);
		color: var(--ink-brown);
		font-family: var(--font-stats);
	}

	.part-details {
		margin-top: var(--space-4);
		padding: var(--space-4);
		background: var(--parchment-light);
		border: 1px solid var(--parchment-shadow);
		border-radius: var(--radius-md);
		width: 100%;
		max-width: 200px;
	}

	.part-details h4 {
		margin: 0 0 var(--space-3) 0;
		color: var(--ornament-gold);
		font-size: var(--text-base);
		font-family: var(--font-display);
	}

	.health-display {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-bottom: var(--space-3);
	}

	.health-display .label {
		font-size: var(--text-sm);
		color: var(--ink-brown);
		font-family: var(--font-stats);
	}

	.health-bar {
		flex: 1;
		height: 8px;
		background: var(--parchment-dark);
		border-radius: var(--radius-full);
		overflow: hidden;
	}

	.health-fill {
		height: 100%;
		border-radius: var(--radius-full);
		transition: width var(--duration-normal);
	}

	.health-display .value {
		font-size: var(--text-sm);
		font-weight: var(--font-semibold);
		min-width: 50px;
		text-align: right;
		font-family: var(--font-stats);
	}

	.close-btn {
		width: 100%;
		padding: var(--space-2);
		background: var(--parchment-dark);
		border: 1px solid var(--parchment-shadow);
		border-radius: var(--radius-sm);
		color: var(--ink-brown);
		cursor: pointer;
		font-size: var(--text-sm);
		font-family: var(--font-display);
		transition: all var(--duration-fast);
	}

	.close-btn:hover {
		background: var(--parchment-medium);
		color: var(--ink-dark);
	}
</style>
