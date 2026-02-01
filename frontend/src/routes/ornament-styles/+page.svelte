<script lang="ts">
	type OrnamentStyle = 'floral-coral' | 'renaissance-gold' | 'eastern-elegance' | 'mughal-royal' | 'rustic-botanical';

	let activeStyle = $state<OrnamentStyle>('floral-coral');
	let isTransitioning = $state(false);

	const styles: { id: OrnamentStyle; name: string; subtitle: string; era: string }[] = [
		{ id: 'floral-coral', name: 'Floral Coral', subtitle: 'Warm Botanical Elegance', era: 'Rococo Gardens' },
		{ id: 'renaissance-gold', name: 'Renaissance Gold', subtitle: 'Regal Manuscript Art', era: 'Italian Courts' },
		{ id: 'eastern-elegance', name: 'Eastern Elegance', subtitle: 'Intricate Earth Tones', era: 'Persian Carpets' },
		{ id: 'mughal-royal', name: 'Mughal Royal', subtitle: 'Imperial Splendor', era: 'Indian Palaces' },
		{ id: 'rustic-botanical', name: 'Rustic Botanical', subtitle: 'Earthy Warmth', era: 'Vintage Naturalism' }
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
</script>

<div class="showcase" data-style={activeStyle} class:transitioning={isTransitioning}>
	<!-- Ambient Background Layer -->
	<div class="ambient-layer"></div>

	<!-- Style Selector Navigation -->
	<nav class="style-nav">
		<div class="nav-ornament left"></div>
		<div class="nav-content">
			{#each styles as style}
				<button
					class="style-btn"
					class:active={activeStyle === style.id}
					onclick={() => selectStyle(style.id)}
				>
					<span class="style-icon"></span>
					<span class="style-name">{style.name}</span>
					<span class="style-era">{style.era}</span>
				</button>
			{/each}
		</div>
		<div class="nav-ornament right"></div>
	</nav>

	<!-- Main Showcase Area -->
	<main class="showcase-main">
		<!-- Corner Ornaments - Using Actual SVGs -->
		<div class="corner-ornaments">
			<div class="corner top-left">
				<img src="/borders/{activeStyle}-corner.svg" alt="" class="corner-svg" />
			</div>
			<div class="corner top-right">
				<img src="/borders/{activeStyle}-corner.svg" alt="" class="corner-svg" />
			</div>
			<div class="corner bottom-left">
				<img src="/borders/{activeStyle}-corner.svg" alt="" class="corner-svg" />
			</div>
			<div class="corner bottom-right">
				<img src="/borders/{activeStyle}-corner.svg" alt="" class="corner-svg" />
			</div>
		</div>

		<!-- Edge Borders - THICK DECORATIVE -->
		<div class="edge-borders">
			<div class="edge top"></div>
			<div class="edge right"></div>
			<div class="edge bottom"></div>
			<div class="edge left"></div>
		</div>

		<!-- Central Content -->
		<div class="content-area">
			<!-- Header with Current Style -->
			<header class="style-header">
				<div class="header-ornament"></div>
				<h1 class="style-title">
					{styles.find(s => s.id === activeStyle)?.name}
				</h1>
				<p class="style-subtitle">
					{styles.find(s => s.id === activeStyle)?.subtitle}
				</p>
				<div class="header-ornament"></div>
			</header>

			<!-- Large Central Medallion - Using Actual SVG -->
			<div class="medallion-showcase">
				<div class="medallion">
					<img src="/borders/{activeStyle}-medallion.svg" alt="" class="medallion-svg" />
				</div>
			</div>

			<!-- Prominent Divider - Using Actual SVG -->
			<div class="divider-showcase">
				<img src="/borders/{activeStyle}-divider.svg" alt="" class="divider-svg" />
			</div>

			<!-- Sample Content Panel -->
			<section class="sample-panel">
				<div class="panel-border">
					<div class="panel-content">
						<h2 class="panel-heading">The Art of Ornamentation</h2>
						<p class="panel-text">
							In the grand tradition of illuminated manuscripts, each stroke carries meaning,
							each color tells a story. The master craftsmen of old spent years perfecting
							the art of decoration, transforming simple pages into treasures of incalculable worth.
						</p>
						<p class="panel-text">
							From the coral gardens of Rococo France to the gold-leafed halls of Renaissance Italy,
							from the intricate carpets of Persia to the magnificent palaces of Mughal India —
							each tradition developed its own visual language, its own vocabulary of beauty.
						</p>
						<div class="panel-footer">
							<span class="footer-ornament"></span>
							<span class="footer-text">Artisans of the Ages</span>
							<span class="footer-ornament"></span>
						</div>
					</div>
				</div>
			</section>

			<!-- Border Thickness Showcase -->
			<section class="border-showcase">
				<h3 class="section-heading">Border Weights</h3>
				<div class="border-samples">
					<div class="border-sample thin">
						<span>4px</span>
					</div>
					<div class="border-sample medium">
						<span>8px</span>
					</div>
					<div class="border-sample thick">
						<span>12px</span>
					</div>
					<div class="border-sample ultra">
						<span>16px</span>
					</div>
				</div>
			</section>
		</div>
	</main>

	<!-- Footer Ornament -->
	<footer class="showcase-footer">
		<div class="footer-border"></div>
		<div class="footer-content">
			<span class="footer-symbol left"></span>
			<span class="footer-label">Ornament Style Showcase</span>
			<span class="footer-symbol right"></span>
		</div>
	</footer>
</div>

<style>
	/* =============================================
	   BASE SHOWCASE STRUCTURE
	   ============================================= */
	.showcase {
		min-height: 100vh;
		display: flex;
		flex-direction: column;
		position: relative;
		overflow-x: hidden;
		transition: all 0.6s cubic-bezier(0.4, 0, 0.2, 1);
	}

	.showcase.transitioning {
		opacity: 0.7;
		transform: scale(0.995);
	}

	.ambient-layer {
		position: fixed;
		inset: 0;
		pointer-events: none;
		z-index: 0;
		transition: all 0.8s ease;
	}

	/* =============================================
	   STYLE NAVIGATION
	   ============================================= */
	.style-nav {
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 2rem;
		position: relative;
		z-index: 100;
	}

	.nav-content {
		display: flex;
		gap: 0.5rem;
		padding: 0.75rem;
		border-radius: 4px;
		transition: all 0.5s ease;
	}

	.nav-ornament {
		width: 60px;
		height: 60px;
		transition: all 0.5s ease;
	}

	.style-btn {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.25rem;
		padding: 1rem 1.5rem;
		border-radius: 4px;
		transition: all 0.3s ease;
		position: relative;
		overflow: hidden;
	}

	.style-btn::before {
		content: '';
		position: absolute;
		inset: 0;
		opacity: 0;
		transition: opacity 0.3s ease;
	}

	.style-btn:hover::before {
		opacity: 0.1;
	}

	.style-btn.active::before {
		opacity: 0.2;
	}

	.style-icon {
		width: 32px;
		height: 32px;
		border-radius: 50%;
		transition: all 0.3s ease;
	}

	.style-name {
		font-family: var(--font-display);
		font-size: 0.9rem;
		font-weight: 600;
		letter-spacing: 0.05em;
		text-transform: uppercase;
		transition: all 0.3s ease;
	}

	.style-era {
		font-family: var(--font-stats);
		font-size: 0.7rem;
		font-style: italic;
		opacity: 0.7;
		transition: all 0.3s ease;
	}

	.style-btn.active .style-icon {
		transform: scale(1.2);
		box-shadow: 0 0 20px currentColor;
	}

	/* =============================================
	   MAIN SHOWCASE AREA
	   ============================================= */
	.showcase-main {
		flex: 1;
		position: relative;
		margin: 0 2rem 2rem;
		min-height: 800px;
	}

	/* =============================================
	   CORNER ORNAMENTS - BOLD & THICK
	   ============================================= */
	.corner-ornaments {
		position: absolute;
		inset: 0;
		pointer-events: none;
		z-index: 10;
	}

	.corner {
		position: absolute;
		width: 150px;
		height: 150px;
		transition: all 0.6s cubic-bezier(0.4, 0, 0.2, 1);
	}

	.corner.top-left { top: 0; left: 0; }
	.corner.top-right { top: 0; right: 0; transform: scaleX(-1); }
	.corner.bottom-left { bottom: 0; left: 0; transform: scaleY(-1); }
	.corner.bottom-right { bottom: 0; right: 0; transform: scale(-1); }

	.corner-svg {
		width: 100%;
		height: 100%;
		object-fit: contain;
		transition: all 0.6s ease;
		filter: drop-shadow(2px 2px 4px rgba(0,0,0,0.2));
	}

	/* =============================================
	   EDGE BORDERS - 12px THICK
	   ============================================= */
	.edge-borders {
		position: absolute;
		inset: 0;
		pointer-events: none;
		z-index: 5;
	}

	.edge {
		position: absolute;
		transition: all 0.6s ease;
	}

	.edge.top, .edge.bottom {
		left: 150px;
		right: 150px;
		height: 12px;
	}

	.edge.top { top: 0; }
	.edge.bottom { bottom: 0; }

	.edge.left, .edge.right {
		top: 150px;
		bottom: 150px;
		width: 12px;
	}

	.edge.left { left: 0; }
	.edge.right { right: 0; }

	/* =============================================
	   CONTENT AREA
	   ============================================= */
	.content-area {
		position: relative;
		z-index: 20;
		padding: 4rem;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 3rem;
	}

	/* =============================================
	   STYLE HEADER
	   ============================================= */
	.style-header {
		text-align: center;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 1rem;
	}

	.header-ornament {
		width: 200px;
		height: 20px;
		transition: all 0.6s ease;
	}

	.style-title {
		font-family: var(--font-display);
		font-size: 3.5rem;
		font-weight: 700;
		letter-spacing: 0.15em;
		text-transform: uppercase;
		margin: 0;
		transition: all 0.5s ease;
	}

	.style-subtitle {
		font-family: var(--font-stats);
		font-size: 1.25rem;
		font-style: italic;
		margin: 0;
		transition: all 0.5s ease;
	}

	/* =============================================
	   MEDALLION - LARGE & PROMINENT
	   ============================================= */
	.medallion-showcase {
		padding: 2rem;
	}

	.medallion {
		position: relative;
		width: 220px;
		height: 220px;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.medallion-svg {
		width: 100%;
		height: 100%;
		object-fit: contain;
		transition: all 0.6s ease;
		filter: drop-shadow(0 4px 20px rgba(0,0,0,0.25));
	}

	.medallion:hover .medallion-svg {
		transform: scale(1.05) rotate(5deg);
	}

	/* =============================================
	   DIVIDER - PROMINENT
	   ============================================= */
	.divider-showcase {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 100%;
		max-width: 700px;
	}

	.divider-svg {
		width: 100%;
		height: 60px;
		object-fit: contain;
		transition: all 0.6s ease;
		filter: drop-shadow(0 2px 8px rgba(0,0,0,0.15));
	}

	/* =============================================
	   SAMPLE PANEL
	   ============================================= */
	.sample-panel {
		width: 100%;
		max-width: 800px;
	}

	.panel-border {
		padding: 12px;
		transition: all 0.6s ease;
	}

	.panel-content {
		padding: 2.5rem;
		transition: all 0.6s ease;
	}

	.panel-heading {
		font-family: var(--font-display);
		font-size: 1.75rem;
		font-weight: 600;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		text-align: center;
		margin-bottom: 1.5rem;
		transition: all 0.5s ease;
	}

	.panel-text {
		font-family: var(--font-body);
		font-size: 1.1rem;
		line-height: 1.8;
		text-align: justify;
		text-indent: 2em;
		margin-bottom: 1rem;
		transition: all 0.5s ease;
	}

	.panel-footer {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 1rem;
		margin-top: 2rem;
		padding-top: 1.5rem;
		border-top: 2px solid;
		transition: all 0.5s ease;
	}

	.footer-ornament {
		width: 24px;
		height: 24px;
		transition: all 0.5s ease;
	}

	.footer-text {
		font-family: var(--font-stats);
		font-size: 1rem;
		font-style: italic;
		transition: all 0.5s ease;
	}

	/* =============================================
	   BORDER SHOWCASE
	   ============================================= */
	.border-showcase {
		width: 100%;
		max-width: 700px;
	}

	.section-heading {
		font-family: var(--font-display);
		font-size: 1.25rem;
		text-align: center;
		letter-spacing: 0.15em;
		text-transform: uppercase;
		margin-bottom: 1.5rem;
		transition: all 0.5s ease;
	}

	.border-samples {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 1.5rem;
	}

	.border-sample {
		aspect-ratio: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		font-family: var(--font-display);
		font-size: 1.25rem;
		font-weight: 600;
		transition: all 0.5s ease;
	}

	.border-sample.thin { border-width: 4px; border-style: solid; }
	.border-sample.medium { border-width: 8px; border-style: solid; }
	.border-sample.thick { border-width: 12px; border-style: solid; }
	.border-sample.ultra { border-width: 16px; border-style: solid; }

	/* =============================================
	   FOOTER
	   ============================================= */
	.showcase-footer {
		padding: 1.5rem 2rem;
		text-align: center;
		transition: all 0.5s ease;
	}

	.footer-border {
		height: 4px;
		margin-bottom: 1rem;
		transition: all 0.5s ease;
	}

	.footer-content {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 1rem;
	}

	.footer-symbol {
		width: 20px;
		height: 20px;
		transition: all 0.5s ease;
	}

	.footer-label {
		font-family: var(--font-display);
		font-size: 0.85rem;
		letter-spacing: 0.2em;
		text-transform: uppercase;
		transition: all 0.5s ease;
	}

	/* =============================================
	   FLORAL CORAL STYLE
	   Warm, organic, flowing curves
	   ============================================= */
	[data-style="floral-coral"] {
		--style-primary: #F79764;
		--style-secondary: #AF4C54;
		--style-tertiary: #A9C1A3;
		--style-accent: #E8B4A0;
		--style-bg: #FDF8F4;
		--style-bg-dark: #F5EDE6;
		--style-text: #5C4033;
		--style-text-light: #8B7355;
	}

	[data-style="floral-coral"] .ambient-layer {
		background:
			radial-gradient(ellipse 80% 50% at 20% 20%, rgba(247, 151, 100, 0.15) 0%, transparent 50%),
			radial-gradient(ellipse 60% 40% at 80% 80%, rgba(169, 193, 163, 0.15) 0%, transparent 50%),
			linear-gradient(135deg, #FDF8F4 0%, #F5EDE6 100%);
	}

	[data-style="floral-coral"] .nav-content {
		background: rgba(247, 151, 100, 0.1);
		border: 2px solid var(--style-primary);
	}

	[data-style="floral-coral"] .nav-ornament {
		background:
			radial-gradient(circle at 30% 30%, var(--style-primary) 8%, transparent 8%),
			radial-gradient(circle at 70% 30%, var(--style-secondary) 6%, transparent 6%),
			radial-gradient(circle at 50% 70%, var(--style-tertiary) 10%, transparent 10%),
			radial-gradient(circle at 50% 50%, var(--style-accent) 15%, transparent 15%);
	}

	[data-style="floral-coral"] .style-btn {
		color: var(--style-text);
		border: 2px solid transparent;
	}

	[data-style="floral-coral"] .style-btn::before {
		background: var(--style-primary);
	}

	[data-style="floral-coral"] .style-btn.active {
		border-color: var(--style-primary);
		background: rgba(247, 151, 100, 0.15);
	}

	[data-style="floral-coral"] .style-icon {
		background: conic-gradient(
			from 0deg,
			var(--style-primary) 0deg,
			var(--style-accent) 90deg,
			var(--style-tertiary) 180deg,
			var(--style-secondary) 270deg,
			var(--style-primary) 360deg
		);
	}

	[data-style="floral-coral"] .showcase-main {
		background: var(--style-bg);
	}

	[data-style="floral-coral"] .edge.top,
	[data-style="floral-coral"] .edge.bottom {
		background: linear-gradient(90deg,
			var(--style-primary) 0%,
			var(--style-accent) 25%,
			var(--style-tertiary) 50%,
			var(--style-accent) 75%,
			var(--style-primary) 100%
		);
	}

	[data-style="floral-coral"] .edge.left,
	[data-style="floral-coral"] .edge.right {
		background: linear-gradient(180deg,
			var(--style-primary) 0%,
			var(--style-secondary) 25%,
			var(--style-tertiary) 50%,
			var(--style-secondary) 75%,
			var(--style-primary) 100%
		);
	}

	[data-style="floral-coral"] .header-ornament {
		background: linear-gradient(90deg,
			transparent 0%,
			var(--style-tertiary) 10%,
			var(--style-primary) 30%,
			var(--style-secondary) 50%,
			var(--style-primary) 70%,
			var(--style-tertiary) 90%,
			transparent 100%
		);
	}

	[data-style="floral-coral"] .style-title {
		color: var(--style-secondary);
		text-shadow: 2px 2px 0 var(--style-accent);
	}

	[data-style="floral-coral"] .style-subtitle {
		color: var(--style-text-light);
	}

	[data-style="floral-coral"] .panel-border {
		background: linear-gradient(135deg,
			var(--style-primary),
			var(--style-secondary),
			var(--style-tertiary),
			var(--style-primary)
		);
		border-radius: 8px;
	}

	[data-style="floral-coral"] .panel-content {
		background: var(--style-bg);
		border-radius: 4px;
	}

	[data-style="floral-coral"] .panel-heading {
		color: var(--style-secondary);
	}

	[data-style="floral-coral"] .panel-text {
		color: var(--style-text);
	}

	[data-style="floral-coral"] .panel-footer {
		border-color: var(--style-tertiary);
	}

	[data-style="floral-coral"] .footer-ornament {
		background: radial-gradient(circle, var(--style-primary), var(--style-secondary));
		border-radius: 50%;
	}

	[data-style="floral-coral"] .footer-text {
		color: var(--style-text-light);
	}

	[data-style="floral-coral"] .section-heading {
		color: var(--style-secondary);
	}

	[data-style="floral-coral"] .border-sample {
		background: var(--style-bg);
		color: var(--style-text);
	}

	[data-style="floral-coral"] .border-sample.thin { border-color: var(--style-tertiary); }
	[data-style="floral-coral"] .border-sample.medium { border-color: var(--style-primary); }
	[data-style="floral-coral"] .border-sample.thick { border-color: var(--style-secondary); }
	[data-style="floral-coral"] .border-sample.ultra {
		border-color: var(--style-secondary);
		border-image: linear-gradient(135deg, var(--style-primary), var(--style-secondary), var(--style-tertiary)) 1;
	}

	[data-style="floral-coral"] .footer-border {
		background: linear-gradient(90deg,
			transparent,
			var(--style-tertiary) 20%,
			var(--style-primary) 35%,
			var(--style-secondary) 50%,
			var(--style-primary) 65%,
			var(--style-tertiary) 80%,
			transparent
		);
	}

	[data-style="floral-coral"] .footer-symbol {
		background: conic-gradient(var(--style-primary), var(--style-tertiary), var(--style-primary));
		border-radius: 50%;
	}

	[data-style="floral-coral"] .footer-label {
		color: var(--style-text-light);
	}

	/* =============================================
	   RENAISSANCE GOLD STYLE
	   Regal, balanced, geometric with flourishes
	   ============================================= */
	[data-style="renaissance-gold"] {
		--style-primary: #EDCE6B;
		--style-secondary: #5569AC;
		--style-tertiary: #538237;
		--style-accent: #57828F;
		--style-bg: #F8F6F0;
		--style-bg-dark: #EAE6DA;
		--style-text: #2C2416;
		--style-text-light: #5A4D3A;
	}

	[data-style="renaissance-gold"] .ambient-layer {
		background:
			radial-gradient(ellipse 100% 60% at 50% 0%, rgba(237, 206, 107, 0.12) 0%, transparent 50%),
			radial-gradient(ellipse 80% 80% at 0% 100%, rgba(85, 105, 172, 0.08) 0%, transparent 40%),
			radial-gradient(ellipse 80% 80% at 100% 100%, rgba(83, 130, 55, 0.08) 0%, transparent 40%),
			linear-gradient(180deg, #F8F6F0 0%, #EAE6DA 100%);
	}

	[data-style="renaissance-gold"] .nav-content {
		background: linear-gradient(180deg, rgba(237, 206, 107, 0.15), rgba(237, 206, 107, 0.05));
		border: 3px solid var(--style-primary);
		box-shadow: 0 0 0 1px var(--style-secondary), 0 4px 20px rgba(237, 206, 107, 0.2);
	}

	[data-style="renaissance-gold"] .nav-ornament {
		background:
			linear-gradient(45deg, var(--style-primary) 25%, transparent 25%),
			linear-gradient(-45deg, var(--style-secondary) 25%, transparent 25%),
			linear-gradient(135deg, var(--style-tertiary) 25%, transparent 25%),
			linear-gradient(-135deg, var(--style-accent) 25%, transparent 25%),
			radial-gradient(circle at 50% 50%, var(--style-primary) 20%, transparent 20%);
	}

	[data-style="renaissance-gold"] .style-btn {
		color: var(--style-text);
		border: 2px solid var(--style-primary);
		background: rgba(255, 255, 255, 0.5);
	}

	[data-style="renaissance-gold"] .style-btn::before {
		background: var(--style-primary);
	}

	[data-style="renaissance-gold"] .style-btn.active {
		background: linear-gradient(180deg, var(--style-primary), #D4B85A);
		color: var(--style-text);
		box-shadow: 0 4px 15px rgba(237, 206, 107, 0.5);
	}

	[data-style="renaissance-gold"] .style-icon {
		background: linear-gradient(135deg, var(--style-primary), #C9A227);
		border: 2px solid var(--style-secondary);
	}

	[data-style="renaissance-gold"] .showcase-main {
		background: var(--style-bg);
		box-shadow: inset 0 0 100px rgba(237, 206, 107, 0.1);
	}

	[data-style="renaissance-gold"] .edge.top,
	[data-style="renaissance-gold"] .edge.bottom {
		background:
			repeating-linear-gradient(90deg,
				var(--style-primary) 0px,
				var(--style-primary) 20px,
				var(--style-secondary) 20px,
				var(--style-secondary) 25px,
				var(--style-tertiary) 25px,
				var(--style-tertiary) 30px,
				var(--style-secondary) 30px,
				var(--style-secondary) 35px
			);
		box-shadow: 0 0 0 2px var(--style-text);
	}

	[data-style="renaissance-gold"] .edge.left,
	[data-style="renaissance-gold"] .edge.right {
		background:
			repeating-linear-gradient(180deg,
				var(--style-primary) 0px,
				var(--style-primary) 20px,
				var(--style-secondary) 20px,
				var(--style-secondary) 25px,
				var(--style-tertiary) 25px,
				var(--style-tertiary) 30px,
				var(--style-secondary) 30px,
				var(--style-secondary) 35px
			);
		box-shadow: 0 0 0 2px var(--style-text);
	}

	[data-style="renaissance-gold"] .header-ornament {
		height: 30px;
		background:
			linear-gradient(90deg,
				transparent 0%,
				var(--style-secondary) 5%,
				var(--style-secondary) 10%,
				transparent 10%,
				transparent 15%,
				var(--style-primary) 15%,
				var(--style-primary) 85%,
				transparent 85%,
				transparent 90%,
				var(--style-secondary) 90%,
				var(--style-secondary) 95%,
				transparent 100%
			);
		box-shadow: 0 2px 0 var(--style-text), 0 -2px 0 var(--style-text);
	}

	[data-style="renaissance-gold"] .style-title {
		color: var(--style-text);
		text-shadow:
			1px 1px 0 var(--style-primary),
			2px 2px 0 var(--style-secondary);
		-webkit-text-stroke: 1px var(--style-text);
	}

	[data-style="renaissance-gold"] .style-subtitle {
		color: var(--style-secondary);
	}

	[data-style="renaissance-gold"] .panel-border {
		background: var(--style-text);
		padding: 4px;
		border-radius: 0;
	}

	[data-style="renaissance-gold"] .panel-border::before {
		content: '';
		position: absolute;
		inset: 4px;
		background: var(--style-primary);
		z-index: -1;
	}

	[data-style="renaissance-gold"] .panel-content {
		background: var(--style-bg);
		border: 4px solid var(--style-primary);
		box-shadow: inset 0 0 0 2px var(--style-secondary);
		position: relative;
	}

	[data-style="renaissance-gold"] .panel-heading {
		color: var(--style-secondary);
		border-bottom: 3px solid var(--style-primary);
		padding-bottom: 1rem;
	}

	[data-style="renaissance-gold"] .panel-text {
		color: var(--style-text);
	}

	[data-style="renaissance-gold"] .panel-footer {
		border-color: var(--style-primary);
		border-width: 3px;
	}

	[data-style="renaissance-gold"] .footer-ornament {
		background: linear-gradient(135deg, var(--style-primary), var(--style-secondary));
		transform: rotate(45deg);
	}

	[data-style="renaissance-gold"] .footer-text {
		color: var(--style-secondary);
		font-weight: 600;
	}

	[data-style="renaissance-gold"] .section-heading {
		color: var(--style-secondary);
		border-bottom: 2px solid var(--style-primary);
		padding-bottom: 0.5rem;
	}

	[data-style="renaissance-gold"] .border-sample {
		background: var(--style-bg);
		color: var(--style-text);
	}

	[data-style="renaissance-gold"] .border-sample.thin { border-color: var(--style-primary); }
	[data-style="renaissance-gold"] .border-sample.medium { border-color: var(--style-secondary); }
	[data-style="renaissance-gold"] .border-sample.thick { border-color: var(--style-tertiary); }
	[data-style="renaissance-gold"] .border-sample.ultra {
		border-color: var(--style-primary);
		box-shadow: 0 0 0 4px var(--style-secondary), 0 0 0 8px var(--style-tertiary);
	}

	[data-style="renaissance-gold"] .footer-border {
		height: 6px;
		background:
			repeating-linear-gradient(90deg,
				var(--style-primary) 0px,
				var(--style-primary) 30px,
				var(--style-secondary) 30px,
				var(--style-secondary) 35px,
				var(--style-tertiary) 35px,
				var(--style-tertiary) 40px,
				var(--style-secondary) 40px,
				var(--style-secondary) 45px
			);
	}

	[data-style="renaissance-gold"] .footer-symbol {
		background: var(--style-primary);
		border: 2px solid var(--style-text);
		transform: rotate(45deg);
	}

	[data-style="renaissance-gold"] .footer-label {
		color: var(--style-text);
	}

	/* =============================================
	   EASTERN ELEGANCE STYLE
	   Earthy, intricate, flowing patterns
	   ============================================= */
	[data-style="eastern-elegance"] {
		--style-primary: #DDBB8E;
		--style-secondary: #8B6914;
		--style-tertiary: #6B4423;
		--style-accent: #C4956A;
		--style-bg: #FBF7F0;
		--style-bg-dark: #F0E8DC;
		--style-text: #3D2B1F;
		--style-text-light: #6B5344;
	}

	[data-style="eastern-elegance"] .ambient-layer {
		background:
			radial-gradient(ellipse 120% 80% at 50% 50%, rgba(221, 187, 142, 0.1) 0%, transparent 50%),
			repeating-linear-gradient(45deg,
				transparent 0px,
				transparent 40px,
				rgba(139, 105, 20, 0.03) 40px,
				rgba(139, 105, 20, 0.03) 41px
			),
			repeating-linear-gradient(-45deg,
				transparent 0px,
				transparent 40px,
				rgba(107, 68, 35, 0.03) 40px,
				rgba(107, 68, 35, 0.03) 41px
			),
			linear-gradient(180deg, #FBF7F0 0%, #F0E8DC 100%);
	}

	[data-style="eastern-elegance"] .nav-content {
		background: linear-gradient(180deg, var(--style-bg), var(--style-bg-dark));
		border: 2px solid var(--style-secondary);
		border-radius: 8px;
		box-shadow:
			0 0 0 4px var(--style-primary),
			0 0 0 6px var(--style-secondary);
	}

	[data-style="eastern-elegance"] .nav-ornament {
		background:
			radial-gradient(circle at 50% 50%, var(--style-secondary) 15%, transparent 15%),
			radial-gradient(circle at 25% 25%, var(--style-accent) 10%, transparent 10%),
			radial-gradient(circle at 75% 25%, var(--style-accent) 10%, transparent 10%),
			radial-gradient(circle at 25% 75%, var(--style-accent) 10%, transparent 10%),
			radial-gradient(circle at 75% 75%, var(--style-accent) 10%, transparent 10%),
			radial-gradient(circle at 50% 10%, var(--style-tertiary) 8%, transparent 8%),
			radial-gradient(circle at 50% 90%, var(--style-tertiary) 8%, transparent 8%),
			radial-gradient(circle at 10% 50%, var(--style-tertiary) 8%, transparent 8%),
			radial-gradient(circle at 90% 50%, var(--style-tertiary) 8%, transparent 8%);
	}

	[data-style="eastern-elegance"] .style-btn {
		color: var(--style-text);
		border: 1px solid var(--style-accent);
		border-radius: 6px;
	}

	[data-style="eastern-elegance"] .style-btn::before {
		background: radial-gradient(circle at center, var(--style-primary), transparent);
	}

	[data-style="eastern-elegance"] .style-btn.active {
		background: linear-gradient(180deg, var(--style-primary), var(--style-accent));
		border-color: var(--style-secondary);
		box-shadow: 0 0 20px rgba(139, 105, 20, 0.3);
	}

	[data-style="eastern-elegance"] .style-icon {
		background:
			radial-gradient(circle at 50% 50%, var(--style-secondary) 30%, transparent 30%),
			radial-gradient(circle at 50% 50%, var(--style-primary) 60%, transparent 60%),
			var(--style-accent);
	}

	[data-style="eastern-elegance"] .showcase-main {
		background: var(--style-bg);
	}

	[data-style="eastern-elegance"] .edge.top,
	[data-style="eastern-elegance"] .edge.bottom {
		background:
			repeating-linear-gradient(90deg,
				var(--style-secondary) 0px,
				var(--style-secondary) 8px,
				var(--style-primary) 8px,
				var(--style-primary) 16px,
				var(--style-accent) 16px,
				var(--style-accent) 24px,
				var(--style-primary) 24px,
				var(--style-primary) 32px
			);
	}

	[data-style="eastern-elegance"] .edge.left,
	[data-style="eastern-elegance"] .edge.right {
		background:
			repeating-linear-gradient(180deg,
				var(--style-secondary) 0px,
				var(--style-secondary) 8px,
				var(--style-primary) 8px,
				var(--style-primary) 16px,
				var(--style-accent) 16px,
				var(--style-accent) 24px,
				var(--style-primary) 24px,
				var(--style-primary) 32px
			);
	}

	[data-style="eastern-elegance"] .header-ornament {
		height: 16px;
		background:
			repeating-linear-gradient(90deg,
				transparent 0px,
				transparent 8px,
				var(--style-secondary) 8px,
				var(--style-secondary) 10px,
				transparent 10px,
				transparent 18px,
				var(--style-accent) 18px,
				var(--style-accent) 20px
			);
	}

	[data-style="eastern-elegance"] .style-title {
		color: var(--style-secondary);
		letter-spacing: 0.2em;
	}

	[data-style="eastern-elegance"] .style-subtitle {
		color: var(--style-text-light);
	}

	[data-style="eastern-elegance"] .panel-border {
		background:
			linear-gradient(var(--style-secondary), var(--style-secondary)) padding-box,
			repeating-linear-gradient(45deg,
				var(--style-secondary) 0px,
				var(--style-secondary) 10px,
				var(--style-primary) 10px,
				var(--style-primary) 20px
			) border-box;
		border: 8px solid transparent;
		border-radius: 12px;
		padding: 4px;
	}

	[data-style="eastern-elegance"] .panel-content {
		background: var(--style-bg);
		border-radius: 8px;
	}

	[data-style="eastern-elegance"] .panel-heading {
		color: var(--style-secondary);
	}

	[data-style="eastern-elegance"] .panel-text {
		color: var(--style-text);
	}

	[data-style="eastern-elegance"] .panel-footer {
		border-color: var(--style-accent);
	}

	[data-style="eastern-elegance"] .footer-ornament {
		background:
			radial-gradient(circle at 50% 50%, var(--style-secondary) 40%, var(--style-primary) 100%);
		border-radius: 50%;
	}

	[data-style="eastern-elegance"] .footer-text {
		color: var(--style-text-light);
	}

	[data-style="eastern-elegance"] .section-heading {
		color: var(--style-secondary);
	}

	[data-style="eastern-elegance"] .border-sample {
		background: var(--style-bg);
		color: var(--style-text);
		border-radius: 6px;
	}

	[data-style="eastern-elegance"] .border-sample.thin { border-color: var(--style-primary); }
	[data-style="eastern-elegance"] .border-sample.medium { border-color: var(--style-accent); }
	[data-style="eastern-elegance"] .border-sample.thick { border-color: var(--style-secondary); }
	[data-style="eastern-elegance"] .border-sample.ultra {
		border-color: var(--style-tertiary);
		background: linear-gradient(135deg, var(--style-bg), var(--style-primary));
	}

	[data-style="eastern-elegance"] .footer-border {
		background:
			repeating-linear-gradient(90deg,
				var(--style-secondary) 0px,
				var(--style-secondary) 20px,
				var(--style-primary) 20px,
				var(--style-primary) 40px,
				var(--style-accent) 40px,
				var(--style-accent) 60px
			);
	}

	[data-style="eastern-elegance"] .footer-symbol {
		background: radial-gradient(circle, var(--style-secondary), var(--style-accent));
		border-radius: 50%;
	}

	[data-style="eastern-elegance"] .footer-label {
		color: var(--style-text);
	}

	/* =============================================
	   MUGHAL ROYAL STYLE
	   Vibrant, imperial, architectural patterns
	   ============================================= */
	[data-style="mughal-royal"] {
		--style-primary: #2D16D3;
		--style-secondary: #538237;
		--style-tertiary: #57828F;
		--style-accent: #E8B923;
		--style-bg: #F5F0E8;
		--style-bg-dark: #E8E0D4;
		--style-text: #1A1A2E;
		--style-text-light: #3D3D5C;
	}

	[data-style="mughal-royal"] .ambient-layer {
		background:
			radial-gradient(ellipse 60% 40% at 20% 80%, rgba(45, 22, 211, 0.08) 0%, transparent 50%),
			radial-gradient(ellipse 60% 40% at 80% 20%, rgba(83, 130, 55, 0.08) 0%, transparent 50%),
			radial-gradient(ellipse 40% 30% at 50% 50%, rgba(232, 185, 35, 0.05) 0%, transparent 50%),
			linear-gradient(180deg, #F5F0E8 0%, #E8E0D4 100%);
	}

	[data-style="mughal-royal"] .nav-content {
		background: linear-gradient(180deg, var(--style-bg), var(--style-bg-dark));
		border: 4px solid var(--style-primary);
		box-shadow:
			0 0 0 2px var(--style-accent),
			0 0 0 6px var(--style-secondary),
			0 8px 30px rgba(45, 22, 211, 0.2);
	}

	[data-style="mughal-royal"] .nav-ornament {
		background:
			conic-gradient(from 0deg at 50% 50%,
				var(--style-primary) 0deg,
				var(--style-accent) 60deg,
				var(--style-secondary) 120deg,
				var(--style-tertiary) 180deg,
				var(--style-secondary) 240deg,
				var(--style-accent) 300deg,
				var(--style-primary) 360deg
			);
		clip-path: polygon(50% 0%, 100% 38%, 82% 100%, 18% 100%, 0% 38%);
	}

	[data-style="mughal-royal"] .style-btn {
		color: var(--style-text);
		border: 2px solid var(--style-tertiary);
		background: rgba(255, 255, 255, 0.8);
	}

	[data-style="mughal-royal"] .style-btn::before {
		background: linear-gradient(135deg, var(--style-primary), var(--style-tertiary));
	}

	[data-style="mughal-royal"] .style-btn.active {
		background: linear-gradient(180deg, var(--style-primary), #1E0F8C);
		color: var(--style-bg);
		border-color: var(--style-accent);
		box-shadow:
			0 0 20px rgba(45, 22, 211, 0.5),
			0 0 40px rgba(232, 185, 35, 0.3);
	}

	[data-style="mughal-royal"] .style-icon {
		background:
			conic-gradient(from 0deg,
				var(--style-primary) 0deg,
				var(--style-accent) 90deg,
				var(--style-secondary) 180deg,
				var(--style-tertiary) 270deg,
				var(--style-primary) 360deg
			);
		border: 2px solid var(--style-accent);
	}

	[data-style="mughal-royal"] .showcase-main {
		background: var(--style-bg);
		box-shadow: inset 0 0 150px rgba(45, 22, 211, 0.05);
	}

	[data-style="mughal-royal"] .edge.top,
	[data-style="mughal-royal"] .edge.bottom {
		height: 14px;
		background:
			linear-gradient(180deg,
				var(--style-accent) 0%,
				var(--style-accent) 20%,
				var(--style-primary) 20%,
				var(--style-primary) 50%,
				var(--style-secondary) 50%,
				var(--style-secondary) 80%,
				var(--style-accent) 80%,
				var(--style-accent) 100%
			);
	}

	[data-style="mughal-royal"] .edge.left,
	[data-style="mughal-royal"] .edge.right {
		width: 14px;
		background:
			linear-gradient(90deg,
				var(--style-accent) 0%,
				var(--style-accent) 20%,
				var(--style-primary) 20%,
				var(--style-primary) 50%,
				var(--style-secondary) 50%,
				var(--style-secondary) 80%,
				var(--style-accent) 80%,
				var(--style-accent) 100%
			);
	}

	[data-style="mughal-royal"] .header-ornament {
		height: 24px;
		background:
			linear-gradient(90deg,
				transparent 0%,
				var(--style-accent) 5%,
				var(--style-primary) 15%,
				var(--style-secondary) 30%,
				var(--style-tertiary) 45%,
				var(--style-accent) 50%,
				var(--style-tertiary) 55%,
				var(--style-secondary) 70%,
				var(--style-primary) 85%,
				var(--style-accent) 95%,
				transparent 100%
			);
		clip-path: polygon(5% 50%, 0 0, 100% 0, 95% 50%, 100% 100%, 0 100%);
	}

	[data-style="mughal-royal"] .style-title {
		color: var(--style-primary);
		text-shadow:
			2px 2px 0 var(--style-accent),
			4px 4px 0 rgba(45, 22, 211, 0.2);
	}

	[data-style="mughal-royal"] .style-subtitle {
		color: var(--style-tertiary);
	}

	[data-style="mughal-royal"] .panel-border {
		background:
			linear-gradient(135deg,
				var(--style-primary),
				var(--style-accent),
				var(--style-secondary),
				var(--style-tertiary),
				var(--style-primary)
			);
		padding: 6px;
	}

	[data-style="mughal-royal"] .panel-content {
		background: var(--style-bg);
		border: 3px solid var(--style-accent);
	}

	[data-style="mughal-royal"] .panel-heading {
		color: var(--style-primary);
	}

	[data-style="mughal-royal"] .panel-text {
		color: var(--style-text);
	}

	[data-style="mughal-royal"] .panel-footer {
		border-color: var(--style-tertiary);
		border-width: 2px;
	}

	[data-style="mughal-royal"] .footer-ornament {
		background:
			conic-gradient(from 0deg,
				var(--style-primary),
				var(--style-accent),
				var(--style-secondary),
				var(--style-primary)
			);
		clip-path: polygon(50% 0%, 100% 38%, 82% 100%, 18% 100%, 0% 38%);
	}

	[data-style="mughal-royal"] .footer-text {
		color: var(--style-primary);
		font-weight: 600;
	}

	[data-style="mughal-royal"] .section-heading {
		color: var(--style-primary);
	}

	[data-style="mughal-royal"] .border-sample {
		background: var(--style-bg);
		color: var(--style-text);
	}

	[data-style="mughal-royal"] .border-sample.thin { border-color: var(--style-tertiary); }
	[data-style="mughal-royal"] .border-sample.medium { border-color: var(--style-secondary); }
	[data-style="mughal-royal"] .border-sample.thick { border-color: var(--style-primary); }
	[data-style="mughal-royal"] .border-sample.ultra {
		border-color: var(--style-accent);
		box-shadow:
			inset 0 0 0 4px var(--style-primary),
			inset 0 0 0 8px var(--style-secondary);
	}

	[data-style="mughal-royal"] .footer-border {
		height: 8px;
		background:
			linear-gradient(90deg,
				var(--style-accent) 0%,
				var(--style-primary) 25%,
				var(--style-secondary) 50%,
				var(--style-primary) 75%,
				var(--style-accent) 100%
			);
	}

	[data-style="mughal-royal"] .footer-symbol {
		background: conic-gradient(var(--style-primary), var(--style-accent), var(--style-primary));
		clip-path: polygon(50% 0%, 100% 38%, 82% 100%, 18% 100%, 0% 38%);
	}

	[data-style="mughal-royal"] .footer-label {
		color: var(--style-text);
	}

	/* =============================================
	   RUSTIC BOTANICAL STYLE
	   Warm, earthy, vintage naturalism
	   ============================================= */
	[data-style="rustic-botanical"] {
		--style-primary: #855956;
		--style-secondary: #75433D;
		--style-tertiary: #A48366;
		--style-accent: #E9D899;
		--style-bg: #F8F4EE;
		--style-bg-dark: #EDE6DC;
		--style-text: #3D2B2B;
		--style-text-light: #6B5344;
	}

	[data-style="rustic-botanical"] .ambient-layer {
		background:
			radial-gradient(ellipse 80% 50% at 20% 20%, rgba(133, 89, 86, 0.12) 0%, transparent 50%),
			radial-gradient(ellipse 60% 40% at 80% 80%, rgba(164, 131, 102, 0.12) 0%, transparent 50%),
			linear-gradient(135deg, #F8F4EE 0%, #EDE6DC 100%);
	}

	[data-style="rustic-botanical"] .nav-content {
		background: rgba(133, 89, 86, 0.08);
		border: 2px solid var(--style-primary);
		border-radius: 6px;
	}

	[data-style="rustic-botanical"] .nav-ornament {
		background:
			radial-gradient(circle at 30% 30%, var(--style-primary) 8%, transparent 8%),
			radial-gradient(circle at 70% 30%, var(--style-secondary) 6%, transparent 6%),
			radial-gradient(circle at 50% 70%, var(--style-tertiary) 10%, transparent 10%),
			radial-gradient(circle at 50% 50%, var(--style-accent) 15%, transparent 15%);
	}

	[data-style="rustic-botanical"] .style-btn {
		color: var(--style-text);
		border: 2px solid transparent;
		border-radius: 4px;
	}

	[data-style="rustic-botanical"] .style-btn::before {
		background: var(--style-primary);
	}

	[data-style="rustic-botanical"] .style-btn.active {
		border-color: var(--style-primary);
		background: rgba(133, 89, 86, 0.15);
	}

	[data-style="rustic-botanical"] .style-icon {
		background: conic-gradient(
			from 0deg,
			var(--style-primary) 0deg,
			var(--style-tertiary) 90deg,
			var(--style-accent) 180deg,
			var(--style-secondary) 270deg,
			var(--style-primary) 360deg
		);
	}

	[data-style="rustic-botanical"] .showcase-main {
		background: var(--style-bg);
	}

	[data-style="rustic-botanical"] .edge.top,
	[data-style="rustic-botanical"] .edge.bottom {
		background: linear-gradient(90deg,
			var(--style-primary) 0%,
			var(--style-tertiary) 25%,
			var(--style-accent) 50%,
			var(--style-tertiary) 75%,
			var(--style-primary) 100%
		);
	}

	[data-style="rustic-botanical"] .edge.left,
	[data-style="rustic-botanical"] .edge.right {
		background: linear-gradient(180deg,
			var(--style-primary) 0%,
			var(--style-secondary) 25%,
			var(--style-accent) 50%,
			var(--style-secondary) 75%,
			var(--style-primary) 100%
		);
	}

	[data-style="rustic-botanical"] .header-ornament {
		background: linear-gradient(90deg,
			transparent 0%,
			var(--style-tertiary) 10%,
			var(--style-primary) 30%,
			var(--style-accent) 50%,
			var(--style-primary) 70%,
			var(--style-tertiary) 90%,
			transparent 100%
		);
	}

	[data-style="rustic-botanical"] .style-title {
		color: var(--style-secondary);
		text-shadow: 2px 2px 0 var(--style-accent);
	}

	[data-style="rustic-botanical"] .style-subtitle {
		color: var(--style-text-light);
	}

	[data-style="rustic-botanical"] .panel-border {
		background: linear-gradient(135deg,
			var(--style-primary),
			var(--style-secondary),
			var(--style-tertiary),
			var(--style-primary)
		);
		border-radius: 8px;
	}

	[data-style="rustic-botanical"] .panel-content {
		background: var(--style-bg);
		border-radius: 4px;
	}

	[data-style="rustic-botanical"] .panel-heading {
		color: var(--style-secondary);
	}

	[data-style="rustic-botanical"] .panel-text {
		color: var(--style-text);
	}

	[data-style="rustic-botanical"] .panel-footer {
		border-color: var(--style-tertiary);
	}

	[data-style="rustic-botanical"] .footer-ornament {
		background: radial-gradient(circle, var(--style-primary), var(--style-secondary));
		border-radius: 50%;
	}

	[data-style="rustic-botanical"] .footer-text {
		color: var(--style-text-light);
	}

	[data-style="rustic-botanical"] .section-heading {
		color: var(--style-secondary);
	}

	[data-style="rustic-botanical"] .border-sample {
		background: var(--style-bg);
		color: var(--style-text);
	}

	[data-style="rustic-botanical"] .border-sample.thin { border-color: var(--style-tertiary); }
	[data-style="rustic-botanical"] .border-sample.medium { border-color: var(--style-primary); }
	[data-style="rustic-botanical"] .border-sample.thick { border-color: var(--style-secondary); }
	[data-style="rustic-botanical"] .border-sample.ultra {
		border-color: var(--style-secondary);
		border-image: linear-gradient(135deg, var(--style-primary), var(--style-secondary), var(--style-tertiary)) 1;
	}

	[data-style="rustic-botanical"] .footer-border {
		background: linear-gradient(90deg,
			transparent,
			var(--style-tertiary) 20%,
			var(--style-primary) 35%,
			var(--style-accent) 50%,
			var(--style-primary) 65%,
			var(--style-tertiary) 80%,
			transparent
		);
	}

	[data-style="rustic-botanical"] .footer-symbol {
		background: conic-gradient(var(--style-primary), var(--style-tertiary), var(--style-primary));
		border-radius: 50%;
	}

	[data-style="rustic-botanical"] .footer-label {
		color: var(--style-text-light);
	}

	/* =============================================
	   RESPONSIVE ADJUSTMENTS
	   ============================================= */
	@media (max-width: 900px) {
		.nav-content {
			flex-wrap: wrap;
			justify-content: center;
		}

		.nav-ornament {
			display: none;
		}

		.corner {
			width: 100px;
			height: 100px;
		}

		.edge.top, .edge.bottom {
			left: 100px;
			right: 100px;
		}

		.edge.left, .edge.right {
			top: 100px;
			bottom: 100px;
		}

		.content-area {
			padding: 2rem;
		}

		.style-title {
			font-size: 2.5rem;
		}

		.medallion {
			width: 150px;
			height: 150px;
		}

		.border-samples {
			grid-template-columns: repeat(2, 1fr);
		}
	}

	@media (max-width: 600px) {
		.style-nav {
			padding: 1rem;
		}

		.style-btn {
			padding: 0.75rem 1rem;
		}

		.style-era {
			display: none;
		}

		.corner {
			width: 60px;
			height: 60px;
		}

		.edge.top, .edge.bottom {
			left: 60px;
			right: 60px;
			height: 8px;
		}

		.edge.left, .edge.right {
			top: 60px;
			bottom: 60px;
			width: 8px;
		}

		.showcase-main {
			margin: 0 1rem 1rem;
		}

		.content-area {
			padding: 1.5rem;
			gap: 2rem;
		}

		.style-title {
			font-size: 1.75rem;
			letter-spacing: 0.1em;
		}

		.medallion {
			width: 120px;
			height: 120px;
		}

		.sample-panel {
			max-width: 100%;
		}

		.panel-content {
			padding: 1.5rem;
		}

		.panel-text {
			font-size: 1rem;
			text-indent: 1em;
		}
	}
</style>
