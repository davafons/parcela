# Parcela brand assets

Two designs are retained from the logo studies.

| File | Purpose |
| --- | --- |
| [`../../logo.svg`](../../logo.svg) | Smooth mascot A, traced from the selected reference into editable vector paths. White fill, dark outline and eyes, and cobalt gesture marks. The canvas is transparent. |
| [`mascot-pixel.png`](mascot-pixel.png) | Pixel 01, the steady-wave mascot. A transparent 32×32 PNG with a one-pixel dark outline. |
| [`mascot-pixel.json`](mascot-pixel.json) | Editable pixel grid and exact RGBA palette. The builder adds the outline without changing the original foreground pixels. |
| [`mascot-reference.png`](mascot-reference.png) | Unmodified native image-generation output A, retained as the master reference. This raster has an opaque background and is not the reusable logo. |

The outline and eyes use `#111318`. The body is white and the accent is `#315de8`. The outline keeps the white forms visible on light backgrounds and blends into the graphite theme on dark backgrounds.

To rebuild the pixel PNG, install the docs dependencies and run:

```sh
npm ci --prefix docs/website
node scripts/build-brand-pixel.mjs
```

Display the pixel mark at its native size or an integer multiple. Use nearest-neighbor scaling, or `image-rendering: pixelated` in CSS. Do not use smoothing or fractional scaling when preserving the pixel grid matters.

The SVG is a vector cleanup of the selected generated concept, not an SVG returned by the image-generation model. The pixel design is grid-authored. The generated master reference has SHA-256 `62373f4ec80f39a9814bad3c8ab6789c7fd7a5c0f5501353c1b4f5758c8cfd74`.
