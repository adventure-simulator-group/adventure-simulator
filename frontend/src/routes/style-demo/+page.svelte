<script lang="ts">
	let currentTheme = $state('illuminated');

	function toggleTheme() {
		// This would need actual theme switching logic
		currentTheme = currentTheme === 'illuminated' ? 'original' : 'illuminated';
	}
</script>

<div class="demo-page">
	<header class="demo-header">
		<h1 class="heading-illuminated">Illuminated Manuscript Theme</h1>
		<p class="subtitle">1500s Bible-inspired ornamentation</p>
	</header>

	<!-- Color Palette -->
	<section class="demo-section">
		<h2 class="heading-illuminated">Color Palette</h2>

		<h3>Parchment (Lighter Paper)</h3>
		<div class="color-row">
			<div class="color-swatch" style="background: var(--parchment-lightest)"><span>lightest</span></div>
			<div class="color-swatch" style="background: var(--parchment-light)"><span>light</span></div>
			<div class="color-swatch" style="background: var(--parchment-base)"><span>base</span></div>
			<div class="color-swatch" style="background: var(--parchment-medium)"><span>medium</span></div>
			<div class="color-swatch" style="background: var(--parchment-dark)"><span>dark</span></div>
			<div class="color-swatch" style="background: var(--parchment-aged)"><span>aged</span></div>
		</div>

		<h3>Illuminated Ornament Colors</h3>
		<div class="color-row">
			<div class="color-swatch" style="background: var(--ornament-red); color: white"><span>Red (Vermillion)</span></div>
			<div class="color-swatch" style="background: var(--ornament-blue); color: white"><span>Blue (Ultramarine)</span></div>
			<div class="color-swatch" style="background: var(--ornament-green); color: white"><span>Green (Verdigris)</span></div>
			<div class="color-swatch" style="background: var(--ornament-gold); color: black"><span>Gold Leaf</span></div>
		</div>

		<h3>Floral & Ribbon Accents</h3>
		<div class="color-row">
			<div class="color-swatch" style="background: var(--accent-rose); color: white"><span>Rose</span></div>
			<div class="color-swatch" style="background: var(--accent-burgundy); color: white"><span>Burgundy</span></div>
			<div class="color-swatch" style="background: var(--accent-teal); color: white"><span>Teal</span></div>
			<div class="color-swatch" style="background: var(--accent-sage); color: white"><span>Sage</span></div>
			<div class="color-swatch" style="background: var(--accent-plum); color: white"><span>Plum</span></div>
		</div>
	</section>

	<!-- Dividers -->
	<section class="demo-section">
		<h2 class="heading-illuminated">Dividers</h2>

		<p>Original gold divider:</p>
		<div class="divider">
			<div class="divider-ornament"></div>
		</div>

		<p>Illuminated divider (new):</p>
		<div class="divider-illuminated">
			<div class="divider-illuminated-ornament"></div>
		</div>
	</section>

	<!-- Borders -->
	<section class="demo-section">
		<h2 class="heading-illuminated">Borders</h2>

		<div class="border-grid">
			<div class="border-demo border-illuminated">
				<p><strong>.border-illuminated</strong></p>
				<p>Tricolor gradient border</p>
			</div>

			<div class="border-demo border-illuminated-double">
				<p><strong>.border-illuminated-double</strong></p>
				<p>Triple-line border with red, blue, green</p>
			</div>

			<div class="border-demo border-vine">
				<p><strong>.border-vine</strong></p>
				<p>Organic vine pattern</p>
			</div>
		</div>
	</section>

	<!-- Ribbons -->
	<section class="demo-section">
		<h2 class="heading-illuminated">Ribbon Banners</h2>

		<div class="ribbon-row">
			<span class="ribbon">Original Gold</span>
		</div>

		<div class="ribbon-row">
			<span class="ribbon-illuminated">Blue Ribbon</span>
		</div>

		<div class="ribbon-row">
			<span class="ribbon-illuminated ribbon-red">Red Ribbon</span>
		</div>

		<div class="ribbon-row">
			<span class="ribbon-illuminated ribbon-green">Green Ribbon</span>
		</div>
	</section>

	<!-- Cards -->
	<section class="demo-section">
		<h2 class="heading-illuminated">Cards</h2>

		<div class="card-grid">
			<div class="card-demo card-border">
				<h4>Original Card</h4>
				<p>.card-border</p>
			</div>

			<div class="card-demo card-border-gold">
				<h4>Gold Border</h4>
				<p>.card-border-gold</p>
			</div>

			<div class="card-demo card-illuminated">
				<h4>Illuminated Card</h4>
				<p>.card-illuminated</p>
				<p>Triple border with blue, gold, and red</p>
			</div>
		</div>
	</section>

	<!-- Medallions -->
	<section class="demo-section">
		<h2 class="heading-illuminated">Medallion Frames</h2>

		<div class="medallion-row">
			<div class="medallion-demo">
				<p>Original</p>
				<div class="medallion-frame">
					<div class="medallion-frame-inner medallion-inner-content">
						<span>M</span>
					</div>
				</div>
			</div>

			<div class="medallion-demo">
				<p>Illuminated</p>
				<div class="medallion-illuminated">
					<div class="medallion-illuminated-inner medallion-inner-content">
						<span>M</span>
					</div>
				</div>
			</div>
		</div>
	</section>

	<!-- Headings -->
	<section class="demo-section">
		<h2 class="heading-illuminated">Heading Styles</h2>

		<h3 class="heading-decorated">Original Decorated Heading</h3>
		<p>Uses gold and dark brown underline pattern</p>

		<h3 class="heading-illuminated">Illuminated Heading</h3>
		<p>Uses red, blue, gold, and green segments</p>

		<h3 class="heading-underline">Simple Underline</h3>
		<p>Gold accent underline</p>
	</section>

	<!-- Sample Content Panel -->
	<section class="demo-section">
		<h2 class="heading-illuminated">Sample Content Panel</h2>

		<div class="sample-panel card-illuminated">
			<h3 class="heading-illuminated">The Book of Adventures</h3>

			<div class="divider-illuminated">
				<div class="divider-illuminated-ornament"></div>
			</div>

			<p class="sample-text">
				In the manner of illuminated manuscripts from the great scriptoriums of the
				fifteenth century, this theme brings forth the rich colors of vermillion,
				ultramarine, and verdigris upon lighter vellum parchment.
			</p>

			<p class="sample-text">
				The decorative elements draw inspiration from the ornate borders and
				flourishes found in Renaissance bibles, featuring intertwining ribbons,
				floral motifs, and the characteristic tricolor palette beloved by the
				master illuminators of old.
			</p>

			<div class="divider-illuminated">
				<div class="divider-illuminated-ornament"></div>
			</div>

			<div class="ribbon-row" style="text-align: center;">
				<span class="ribbon-illuminated">Finis</span>
			</div>
		</div>
	</section>
</div>

<style>
	.demo-page {
		max-width: 900px;
		margin: 0 auto;
		padding: var(--space-8) var(--space-4);
	}

	.demo-header {
		text-align: center;
		margin-bottom: var(--space-12);
	}

	.demo-header h1 {
		font-family: var(--font-display);
		font-size: var(--text-4xl);
		color: var(--ink-black);
		margin-bottom: var(--space-2);
	}

	.subtitle {
		font-family: var(--font-stats);
		font-style: italic;
		color: var(--ink-medium);
	}

	.demo-section {
		margin-bottom: var(--space-12);
	}

	.demo-section h2 {
		font-family: var(--font-display);
		font-size: var(--text-2xl);
		color: var(--ink-dark);
		margin-bottom: var(--space-6);
	}

	.demo-section h3 {
		font-family: var(--font-display);
		font-size: var(--text-lg);
		color: var(--ink-brown);
		margin: var(--space-4) 0 var(--space-2);
	}

	.demo-section p {
		color: var(--ink-medium);
		margin-bottom: var(--space-3);
	}

	/* Color swatches */
	.color-row {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		margin-bottom: var(--space-4);
	}

	.color-swatch {
		width: 100px;
		height: 60px;
		border-radius: var(--radius-sm);
		display: flex;
		align-items: flex-end;
		justify-content: center;
		padding: var(--space-1);
		box-shadow: var(--shadow-sm);
	}

	.color-swatch span {
		font-size: var(--text-xs);
		text-align: center;
	}

	/* Border demos */
	.border-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
		gap: var(--space-4);
	}

	.border-demo {
		padding: var(--space-6);
		background: var(--parchment-light);
	}

	.border-demo p {
		margin: 0;
	}

	/* Ribbon row */
	.ribbon-row {
		margin: var(--space-6) 0;
		padding-left: var(--space-6);
	}

	/* Card grid */
	.card-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
		gap: var(--space-4);
	}

	.card-demo {
		padding: var(--space-4);
	}

	.card-demo h4 {
		font-family: var(--font-display);
		margin-bottom: var(--space-2);
	}

	.card-demo p {
		font-size: var(--text-sm);
		margin: 0;
	}

	/* Medallion row */
	.medallion-row {
		display: flex;
		gap: var(--space-8);
		align-items: center;
	}

	.medallion-demo {
		text-align: center;
	}

	.medallion-demo p {
		margin-bottom: var(--space-2);
	}

	.medallion-inner-content {
		width: 64px;
		height: 64px;
		display: flex;
		align-items: center;
		justify-content: center;
		font-family: var(--font-display);
		font-size: var(--text-2xl);
		font-weight: var(--font-bold);
		color: var(--ink-brown);
	}

	/* Sample panel */
	.sample-panel {
		padding: var(--space-8);
		margin-top: var(--space-4);
	}

	.sample-panel h3 {
		text-align: center;
		font-size: var(--text-xl);
	}

	.sample-text {
		font-family: var(--font-body);
		line-height: var(--leading-relaxed);
		text-align: justify;
		text-indent: 2em;
		margin-bottom: var(--space-4);
	}
</style>
