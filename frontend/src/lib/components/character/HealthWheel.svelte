<script lang="ts">
	import '$lib/styles/compat.css';

	interface Incapacitation {
		imbalance: number;
		exhaustion: number;
		pain: number;
		bloodLoss: number;
		fear: number;
		fatigue: number;
	}

	interface Props {
		incapacitation: Incapacitation;
	}

	let { incapacitation }: Props = $props();

	const WHEEL_SIZE = 180;
	const CENTER = WHEEL_SIZE / 2;
	const RADIUS = 70;
	const STROKE_WIDTH = 20;

	const incapTypes = [
		{ key: 'imbalance', label: 'Imbalance', color: 'var(--color-incap-imbalance)' },
		{ key: 'exhaustion', label: 'Exhaustion', color: 'var(--color-incap-exhaustion)' },
		{ key: 'pain', label: 'Pain', color: 'var(--color-incap-pain)' },
		{ key: 'bloodLoss', label: 'Blood Loss', color: 'var(--color-incap-blood)' },
		{ key: 'fear', label: 'Fear', color: 'var(--color-incap-fear)' },
		{ key: 'fatigue', label: 'Fatigue', color: 'var(--color-incap-fatigue)' }
	] as const;

	function describeArc(
		cx: number,
		cy: number,
		radius: number,
		startAngle: number,
		endAngle: number
	): string {
		const start = polarToCartesian(cx, cy, radius, endAngle);
		const end = polarToCartesian(cx, cy, radius, startAngle);
		const largeArcFlag = endAngle - startAngle <= 180 ? 0 : 1;
		return `M ${start.x} ${start.y} A ${radius} ${radius} 0 ${largeArcFlag} 0 ${end.x} ${end.y}`;
	}

	function polarToCartesian(
		cx: number,
		cy: number,
		radius: number,
		angleInDegrees: number
	): { x: number; y: number } {
		const angleInRadians = ((angleInDegrees - 90) * Math.PI) / 180;
		return {
			x: cx + radius * Math.cos(angleInRadians),
			y: cy + radius * Math.sin(angleInRadians)
		};
	}

	function getIncapValue(key: string): number {
		return incapacitation[key as keyof Incapacitation] ?? 0;
	}

	const total = $derived(
		incapacitation.imbalance +
			incapacitation.exhaustion +
			incapacitation.pain +
			incapacitation.bloodLoss +
			incapacitation.fear +
			incapacitation.fatigue
	);

	const totalPercent = $derived(Math.min(total * 100, 100));
	const isWarning = $derived(total > 0.5);
	const isDanger = $derived(total > 0.8);
	const isPulsing = $derived(total > 0.9);

	const segments = $derived(() => {
		const result: Array<{
			key: string;
			label: string;
			color: string;
			value: number;
			startAngle: number;
			endAngle: number;
			path: string;
		}> = [];

		let currentAngle = 0;
		const totalCircle = 360;

		for (const type of incapTypes) {
			const value = incapacitation[type.key as keyof Incapacitation];
			if (value > 0) {
				const segmentAngle = value * totalCircle;
				result.push({
					key: type.key,
					label: type.label,
					color: type.color,
					value,
					startAngle: currentAngle,
					endAngle: currentAngle + segmentAngle,
					path: describeArc(CENTER, CENTER, RADIUS, currentAngle, currentAngle + segmentAngle)
				});
				currentAngle += segmentAngle;
			}
		}

		return result;
	});

	let hoveredSegment = $state<string | null>(null);
</script>

<div class="health-wheel" class:pulsing={isPulsing}>
	<svg
		viewBox="0 0 {WHEEL_SIZE} {WHEEL_SIZE}"
		class="wheel-svg"
		style="--pulse-color: {isDanger
			? 'var(--ornament-red)'
			: isWarning
				? 'var(--ornament-gold)'
				: 'var(--ornament-green)'}"
	>
		<!-- Background circle -->
		<circle
			cx={CENTER}
			cy={CENTER}
			r={RADIUS}
			fill="none"
			stroke="var(--parchment-dark)"
			stroke-width={STROKE_WIDTH}
		/>

		<!-- Incapacitation segments -->
		{#each segments() as segment}
			<path
				d={segment.path}
				fill="none"
				stroke={segment.color}
				stroke-width={STROKE_WIDTH}
				stroke-linecap="butt"
				class="segment"
				class:hovered={hoveredSegment === segment.key}
				onmouseenter={() => (hoveredSegment = segment.key)}
				onmouseleave={() => (hoveredSegment = null)}
			/>
		{/each}

		<!-- Center display -->
		<circle cx={CENTER} cy={CENTER} r={RADIUS - STROKE_WIDTH / 2 - 5} fill="var(--parchment-light)" />

		<!-- Center text -->
		<text
			x={CENTER}
			y={CENTER - 10}
			text-anchor="middle"
			class="center-percent"
			class:warning={isWarning}
			class:danger={isDanger}
		>
			{Math.round(totalPercent)}%
		</text>
		<text x={CENTER} y={CENTER + 15} text-anchor="middle" class="center-label">Incapacitated</text>
	</svg>

	<!-- Legend -->
	<div class="legend">
		{#each incapTypes as type}
			{@const value = getIncapValue(type.key)}
			<div
				class="legend-item"
				class:active={value > 0}
				class:hovered={hoveredSegment === type.key}
				onmouseenter={() => (hoveredSegment = type.key)}
				onmouseleave={() => (hoveredSegment = null)}
			>
				<div class="legend-color" style="background: {type.color}"></div>
				<span class="legend-label">{type.label}</span>
				<span class="legend-value">{Math.round(value * 100)}%</span>
			</div>
		{/each}
	</div>

	<!-- Status indicator -->
	<div class="status" class:warning={isWarning} class:danger={isDanger}>
		{#if isDanger}
			Near Knockout!
		{:else if isWarning}
			Weakened
		{:else}
			Combat Ready
		{/if}
	</div>
</div>

<style>
	.health-wheel {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--space-4);
	}

	.wheel-svg {
		width: 180px;
		height: 180px;
		filter: drop-shadow(0 4px 8px rgba(0, 0, 0, 0.3));
	}

	.segment {
		transition: all var(--duration-fast);
	}

	.segment.hovered {
		stroke-width: 24;
		filter: brightness(1.2);
	}

	.center-percent {
		font-size: 28px;
		font-weight: var(--font-bold);
		fill: var(--ornament-green);
		font-family: var(--font-display);
	}

	.center-percent.warning {
		fill: var(--ornament-gold);
	}

	.center-percent.danger {
		fill: var(--ornament-red);
	}

	.center-label {
		font-size: 10px;
		fill: var(--ink-faded);
		text-transform: uppercase;
		letter-spacing: 0.5px;
		font-family: var(--font-stats);
	}

	.legend {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: var(--space-2);
		width: 100%;
		max-width: 220px;
	}

	.legend-item {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-1) var(--space-2);
		border-radius: var(--radius-sm);
		opacity: 0.5;
		transition: all var(--duration-fast);
		cursor: default;
	}

	.legend-item.active {
		opacity: 1;
	}

	.legend-item.hovered {
		background: var(--parchment-medium);
	}

	.legend-color {
		width: 12px;
		height: 12px;
		border-radius: var(--radius-sm);
		flex-shrink: 0;
	}

	.legend-label {
		font-size: var(--text-xs);
		color: var(--ink-brown);
		flex: 1;
		font-family: var(--font-stats);
	}

	.legend-value {
		font-size: var(--text-xs);
		font-weight: var(--font-semibold);
		font-family: var(--font-stats);
	}

	.status {
		padding: var(--space-2) var(--space-4);
		border-radius: var(--radius-md);
		font-size: var(--text-sm);
		font-weight: var(--font-semibold);
		background: var(--ornament-green);
		color: var(--parchment-lightest);
		text-transform: uppercase;
		letter-spacing: 0.5px;
		font-family: var(--font-display);
	}

	.status.warning {
		background: var(--ornament-gold);
		color: var(--ink-black);
	}

	.status.danger {
		background: var(--ornament-red);
		animation: pulse-danger 1s infinite;
	}

	.health-wheel.pulsing .wheel-svg {
		animation: pulse-glow 1s infinite;
	}

	@keyframes pulse-danger {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.7;
		}
	}

	@keyframes pulse-glow {
		0%,
		100% {
			filter: drop-shadow(0 4px 8px rgba(0, 0, 0, 0.3));
		}
		50% {
			filter: drop-shadow(0 0 20px var(--ornament-red));
		}
	}
</style>
