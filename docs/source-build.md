# Build the release source

Parcela is a Rust CLI. Caddy and Process Compose are separate maintained runtime backends. Building this source does not install them, configure HTTPS, trust a CA, adopt a checkout or start an application.

## Check the build inputs

Use Rust 1.95 and a C compiler for the bundled SQLite library. The native release targets are verified separately. A successful build on one platform does not verify another platform.

Keep the hidden `.agents/skills/parcela/SKILL.md` file. The CLI embeds it at compile time.

The complete release-source bundle contains the frozen tracked source tree, locked dependency sources in `vendor/`, and the matching `.cargo/config.toml`. It also contains full Rust license texts and a `release-source.json` inventory. A raw Cargo package is not the complete release-source bundle.

## Build without network access

From an extracted complete release-source bundle, run:

```sh
cargo build --profile dist --frozen --offline
./target/dist/parcela --version
./target/dist/parcela --help
./target/dist/parcela --skill
```

If a dependency is missing, stop and obtain the matching complete source bundle. Do not remove the lockfile or silently update dependencies.

These commands do not verify live applications or host HTTPS setup. Follow the runtime setup instructions separately before starting an environment.

## Retain license notices

Keep `LICENSE`, `COPYRIGHT`, `THIRD_PARTY_NOTICES.txt`, and `licenses/` with the source. Vendored dependencies retain their own licenses and notices. Caddy and Process Compose have separate upstream notices. Do not replace their notices with Parcela's license.

## Create a source bundle

Use Python 3.11 or newer and `cargo-about` 0.9.2. Populate the locked Cargo dependency cache with an explicit `cargo fetch --locked` before building the archive.

Set `RELEASE_REVISION` to the full 40-character commit ID selected for review. Run the builder from that same revision in a jj workspace or a Git checkout without `.jj/`:

```sh
python3 scripts/build_release_source.py \
	--revision "$RELEASE_REVISION" \
	--output-dir target/release-source \
	--cargo-about cargo-about \
	--verify
```

The output directory must not exist. The builder exports immutable source, vendors the locked dependencies offline, generates notices, and writes a source archive with its SHA-256 checksum. It does not publish a tag or release.

The pinned supplemental license for `rsqlite-vfs` 0.1.1 comes from the upstream commit recorded in that published crate. The builder checks its SHA-256 and crate revision before replacing the incomplete generic MIT notice. It refuses unresolved MIT copyright placeholders. Source provenance records the upstream URL and checksum.

Build and installation checks, independent review, and platform-specific acceptance remain separate release requirements.
