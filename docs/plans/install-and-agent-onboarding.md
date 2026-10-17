# Installation and agent onboarding

## Current decisions

- Use Rails Getting Started as the writing and tutorial reference.
- Provide copyable commands and a setup request that runs in the user's existing agent session.
- License Parcela under GNU AGPL version 3 only. Copyright holder is David Afonso, beginning in 2026.
- Prepare public binary downloads. Keep the repository private for now; a later visibility change requires its own authorization.
- Pair any public AGPL binary release with matching corresponding source. A private-only GitHub link is not the planned source-download path.

AGPL permits commercial use and competing forks subject to its terms. It replaces the earlier anti-competition proposal; do not add custom restrictions. Third-party components retain their licenses. Source and notice completeness need release review, not an assumption that a successful build proves compliance.

## Keep installation phases separate

1. Install reviewed, versioned native artifacts into user-owned paths.
2. Check the required maintained backends and system inspection tools.
3. Prepare HTTPS, forwarding and trust only through an explicit host procedure.
4. Configure the selected project and its data policy inside the user's harness.
5. Adopt and start an environment only after those operations are authorized.

A shell installer must not run application preparation, migrate an installed registry, replace an unknown binary, stop existing owners, trust a CA or alter forwarding. Downloading a newer binary is not a coordinated upgrade of an old proxy and controllers.

## Release artifact contract

A release manifest identifies the version, source revision, supported native targets, archive names and SHA-256 values. Binary and source archives must describe the same reviewed source. Immutable version URLs are the reproducibility boundary; a latest-version lookup selects one of those versions.

Native archives contain the executable, project copyright and license, and required third-party notices. Corresponding source includes the build inputs, build instructions and dependency source needed for the covered work. Vendoring locked Rust dependencies is one candidate for an offline-verifiable source package. Do not copy a home directory, registry, credentials, linked VCS metadata, build caches or internal machine records.

Build and test each advertised target. The local release-mode CLI build is verified on macOS ARM64; native archives have not been assembled or installed. Linux and other architectures must have their own build and execution evidence before being advertised. Native compilation on one host does not establish an ABI or installation contract on another.

Caddy and Process Compose remain maintained external backends. The intended one-install experience needs pinned, verified backend artifacts in a Parcela-owned versioned directory, with upstream licenses and notices. Do not replace a user's general-purpose `caddy` or `process-compose`. The current CLI build is not that integrated package.

## Shell installer contract

Prefer maintained native-release tooling over a custom platform/download framework. A small launcher can expose the chosen release channel once the actual endpoint and artifacts exist.

The installer must verify the selected OS and architecture, retain TLS verification, check artifact hashes, stage files privately, and refuse conflicting destinations. It must report the exact installed version and executable location. Keep shell-profile edits, personal skill installation and host changes separate and opt-in. Existing installations require a guarded upgrade path, not a force flag.

Offer both a download-and-inspect workflow and a clearly documented pipe-to-shell shortcut. Do not put a plausible but nonexistent installer URL into a copyable quickstart. The release installer is not implemented or published yet.

## Harness-native setup

Use the existing `parcela --skill` export as the authoritative operating instructions for a matching binary. The setup request in `docs/site/agents.md` asks the user's agent to inspect project entry points, propose data bindings, update configuration and validate the organizer schema. It does not invoke a Parcela model service or require another API key.

The user's harness supplies the model, normal billing, tools and permissions. Do not add automatic framework setup or an AI gateway. Configuration authorship does not authorize application migrations, seeds, resets, trust, listener takeover or other environments' lifecycle.

For Claude Code, provide project-local installation into `.claude/skills/parcela/SKILL.md`. Personal installation under `~/.claude/skills` is a separate explicit choice. Preserve existing skills, publish only a complete export, and do not modify Claude permissions. Direct download is a fallback, not automatic plugin registration. Model-driven selection and setup must be tested separately from file installation.

## Shipping gates

- Current-scope independent review and CI for the changed source.
- Native package and source reconstruction checks for every advertised target.
- Dependency/license/notice and source-archive privacy review.
- Explicit authorization for artifact publication, docs deployment and any repository visibility change.
- Fresh ownership verification of the existing download hostname and service.

Until these gates pass, local archives are private experiments. The current docs must continue to say that release archives and a public shell installer are unavailable.
