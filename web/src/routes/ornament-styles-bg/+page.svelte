<script lang="ts">
	type OrnamentStyle = 'floral-coral' | 'renaissance-gold' | 'eastern-elegance' | 'mughal-royal' | 'rustic-botanical';

	let activeStyle = $state<OrnamentStyle>('floral-coral');
	let isTransitioning = $state(false);

	const styles: { id: OrnamentStyle; name: string; subtitle: string; file: string; bgColor: string }[] = [
		{ id: 'floral-coral', name: 'Floral Coral', subtitle: 'Warm Botanical Elegance', file: '/references/3529892_64471.svg', bgColor: '#F7F0E2' },
		{ id: 'renaissance-gold', name: 'Renaissance Gold', subtitle: 'Regal Manuscript Art', file: '/references/33812591_7832741.svg', bgColor: '#ECEBE9' },
		{ id: 'eastern-elegance', name: 'Eastern Elegance', subtitle: 'Intricate Earth Tones', file: '/references/9450088_2022.svg', bgColor: '#F5F0E5' },
		{ id: 'mughal-royal', name: 'Mughal Royal', subtitle: 'Imperial Splendor', file: '/references/89911540_Mughul art version 10.svg', bgColor: '#FCF1D2' },
		{ id: 'rustic-botanical', name: 'Rustic Botanical', subtitle: 'Earthy Warmth', file: '/references/16337744_rm298-element-set-katie-tang-04f(1).svg', bgColor: '#1A1A1A' }
	];

	function selectStyle(style: OrnamentStyle) {
		if (style === activeStyle || isTransitioning) return;
		isTransitioning = true;
		setTimeout(() => {
			activeStyle = style;
			setTimeout(() => {
				isTransitioning = false;
			}, 600);
		}, 300);
	}

	function getCurrentStyle() {
		return styles.find(s => s.id === activeStyle)!;
	}
</script>

<div class="showcase" data-style={activeStyle} class:transitioning={isTransitioning}>
	<!-- Style Selector Navigation -->
	<nav class="style-nav">
		<h1 class="page-title">Full Background Preview</h1>
		<div class="nav-content">
			{#each styles as style}
				<button
					class="style-btn"
					class:active={activeStyle === style.id}
					onclick={() => selectStyle(style.id)}
				>
					<span class="style-name">{style.name}</span>
					<span class="style-subtitle">{style.subtitle}</span>
				</button>
			{/each}
		</div>
	</nav>

	<!-- Full Background Preview -->
	<main class="preview-main">
		<div class="background-container">
			<!-- The actual SVG scaled up 4x -->
			<div class="svg-background" style="--bg-color: {getCurrentStyle().bgColor}">
				<img
					src={getCurrentStyle().file}
					alt="{getCurrentStyle().name} ornament pattern"
					class="ornament-image"
				/>
			</div>

			<!-- Sample content overlay to see how it looks with text -->
			<div class="content-overlay">
				<div class="content-card">
					<h2 class="card-title">{getCurrentStyle().name}</h2>
					<p class="card-subtitle">{getCurrentStyle().subtitle}</p>
					<div class="card-divider"></div>
					<p class="card-text">
						This preview shows the full ornamental SVG scaled up approximately 4x
						to demonstrate how it would appear as a page background. The intricate
						details and color palette create a rich, decorative atmosphere suitable
						for storytelling interfaces and immersive applications.
					</p>
					<div class="card-footer">
						<span>Scale: ~4x Original</span>
					</div>
				</div>
			</div>
		</div>
	</main>

	<!-- Info Footer -->
	<footer class="showcase-footer">
		<span class="footer-info">Original SVG: {getCurrentStyle().file}</span>
	</footer>
</div>

<style>
	/* =============================================
	   BASE STRUCTURE
	   ============================================= */
	.showcase {
		min-height: 100vh;
		display: flex;
		flex-direction: column;
		position: relative;
		overflow: hidden;
		transition: all 0.6s cubic-bezier(0.4, 0, 0.2, 1);
	}

	.showcase.transitioning {
		opacity: 0.7;
	}

	/* =============================================
	   NAVIGATION
	   ============================================= */
	.style-nav {
		position: relative;
		z-index: 100;
		padding: 1.5rem 2rem;
		background: rgba(255, 255, 255, 0.95);
		backdrop-filter: blur(10px);
		border-bottom: 3px solid currentColor;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 1rem;
	}

	.page-title {
		font-family: var(--font-display, 'Georgia', serif);
		font-size: 1.5rem;
		font-weight: 700;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		margin: 0;
	}

	.nav-content {
		display: flex;
		gap: 0.5rem;
		flex-wrap: wrap;
		justify-content: center;
	}

	.style-btn {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.25rem;
		padding: 0.75rem 1.25rem;
		border: 2px solid #ccc;
		border-radius: 6px;
		background: white;
		cursor: pointer;
		transition: all 0.3s ease;
	}

	.style-btn:hover {
		border-color: #888;
		transform: translateY(-2px);
	}

	.style-btn.active {
		border-color: #333;
		background: #333;
		color: white;
	}

	.style-name {
		font-family: var(--font-display, 'Georgia', serif);
		font-size: 0.85rem;
		font-weight: 600;
		letter-spacing: 0.05em;
		text-transform: uppercase;
	}

	.style-subtitle {
		font-family: var(--font-body, sans-serif);
		font-size: 0.7rem;
		opacity: 0.7;
	}

	/* =============================================
	   PREVIEW MAIN
	   ============================================= */
	.preview-main {
		flex: 1;
		position: relative;
		overflow: hidden;
	}

	.background-container {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	/* =============================================
	   SVG BACKGROUND - SCALED 4X
	   ============================================= */
	.svg-background {
		position: absolute;
		inset: -50%;
		width: 200%;
		height: 200%;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--bg-color, #f5f5f5);
	}

	.ornament-image {
		width: 400%;
		height: 400%;
		object-fit: contain;
		opacity: 0.9;
		transition: all 0.6s ease;
		filter: drop-shadow(0 0 100px rgba(0,0,0,0.1));
	}

	/* =============================================
	   CONTENT OVERLAY
	   ============================================= */
	.content-overlay {
		position: relative;
		z-index: 10;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 2rem;
		width: 100%;
		height: 100%;
	}

	.content-card {
		max-width: 500px;
		padding: 2.5rem;
		background: rgba(255, 255, 255, 0.92);
		backdrop-filter: blur(20px);
		border-radius: 8px;
		box-shadow:
			0 20px 60px rgba(0, 0, 0, 0.2),
			0 0 0 1px rgba(0, 0, 0, 0.1);
		text-align: center;
	}

	.card-title {
		font-family: var(--font-display, 'Georgia', serif);
		font-size: 2rem;
		font-weight: 700;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		margin: 0 0 0.5rem;
		color: #222;
	}

	.card-subtitle {
		font-family: var(--font-body, sans-serif);
		font-size: 1.1rem;
		font-style: italic;
		color: #666;
		margin: 0 0 1.5rem;
	}

	.card-divider {
		width: 80px;
		height: 3px;
		background: linear-gradient(90deg, transparent, #888, transparent);
		margin: 0 auto 1.5rem;
	}

	.card-text {
		font-family: var(--font-body, sans-serif);
		font-size: 1rem;
		line-height: 1.7;
		color: #444;
		margin: 0 0 1.5rem;
	}

	.card-footer {
		font-family: var(--font-display, 'Georgia', serif);
		font-size: 0.8rem;
		color: #888;
		letter-spacing: 0.1em;
		text-transform: uppercase;
	}

	/* =============================================
	   FOOTER
	   ============================================= */
	.showcase-footer {
		position: relative;
		z-index: 100;
		padding: 1rem 2rem;
		background: rgba(255, 255, 255, 0.95);
		backdrop-filter: blur(10px);
		border-top: 2px solid #ddd;
		text-align: center;
	}

	.footer-info {
		font-family: var(--font-body, monospace);
		font-size: 0.75rem;
		color: #888;
	}

	/* =============================================
	   STYLE-SPECIFIC THEMING
	   ============================================= */
	[data-style="floral-coral"] .style-nav {
		border-color: #F79764;
		color: #AF4C54;
	}

	[data-style="floral-coral"] .card-title {
		color: #AF4C54;
	}

	[data-style="floral-coral"] .card-divider {
		background: linear-gradient(90deg, transparent, #F79764, #AF4C54, #F79764, transparent);
	}

	[data-style="renaissance-gold"] .style-nav {
		border-color: #EDCE6B;
		color: #5569AC;
	}

	[data-style="renaissance-gold"] .card-title {
		color: #5569AC;
	}

	[data-style="renaissance-gold"] .card-divider {
		background: linear-gradient(90deg, transparent, #EDCE6B, #5569AC, #EDCE6B, transparent);
	}

	[data-style="eastern-elegance"] .style-nav {
		border-color: #DDBB8E;
		color: #6B4423;
	}

	[data-style="eastern-elegance"] .card-title {
		color: #6B4423;
	}

	[data-style="eastern-elegance"] .card-divider {
		background: linear-gradient(90deg, transparent, #DDBB8E, #8B6914, #DDBB8E, transparent);
	}

	[data-style="mughal-royal"] .style-nav {
		border-color: #2D16D3;
		color: #2D16D3;
	}

	[data-style="mughal-royal"] .card-title {
		color: #2D16D3;
	}

	[data-style="mughal-royal"] .card-divider {
		background: linear-gradient(90deg, transparent, #E8B923, #2D16D3, #E8B923, transparent);
	}

	[data-style="rustic-botanical"] .style-nav {
		border-color: #855956;
		color: #75433D;
	}

	[data-style="rustic-botanical"] .card-title {
		color: #75433D;
	}

	[data-style="rustic-botanical"] .card-divider {
		background: linear-gradient(90deg, transparent, #A48366, #855956, #E9D899, #855956, #A48366, transparent);
	}

	[data-style="rustic-botanical"] .content-card {
		background: rgba(248, 244, 238, 0.95);
	}

	/* =============================================
	   RESPONSIVE
	   ============================================= */
	@media (max-width: 768px) {
		.page-title {
			font-size: 1.2rem;
		}

		.style-btn {
			padding: 0.5rem 1rem;
		}

		.style-subtitle {
			display: none;
		}

		.content-card {
			padding: 1.5rem;
			margin: 1rem;
		}

		.card-title {
			font-size: 1.5rem;
		}

		.card-text {
			font-size: 0.9rem;
		}
	}
</style>
