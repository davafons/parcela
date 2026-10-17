# Build Parcela and prepare HTTPS

## Build the CLI

Use macOS or Linux. You need Rust 1.89 or newer and a C toolchain for bundled SQLite. Release archives, a public shell installer and automatic backend installation are not available yet.

The source repository is currently private. You need repository access or an existing source checkout to build the CLI. Public documentation and example downloads do not grant that access.

Build the [`main` source branch](https://github.com/davafons/parcela/tree/main) for the commands in these guides. Select the intended revision through your project's VCS workflow before building.

From that Parcela source checkout, copy this block into your shell:

```sh
cargo build --release --locked &&
  export PATH="$PWD/target/release:$PATH" &&
  parcela --version &&
  parcela --help
```

Expect `parcela 0.1.0` followed by command help. If the build fails, the remaining commands do not run.

The PATH change affects this shell only. It does not replace an installed binary or restart its proxy. Keep the same binary and registry for later environment commands.

To print instructions your agent can read:

```sh
parcela --skill
```

The command prints Markdown without reading or creating project or registry state. Printing it does not install a skill.

- [Install the operating skill for Claude Code](agents.md#install-the-skill-for-claude-code) with a copyable project-local command.
- [Ask your existing agent to configure this checkout](agents.md#tldr-ask-your-agent-to-configure-this-checkout) without a separate AI setup service.

> **Coordinate upgrades.**
>
> Schema 3 supports service-set hostname and port reservations. Do not let a new build open installed state for writing while an old binary still owns its proxy or application controllers. Stop or explicitly reconcile those owners as part of an authorized upgrade.

## Check the backends

Install Caddy 2.x and Process Compose 1.x through your chosen package or release workflow. Parcela also needs `lsof` and `ps` for Unix listener and process inspection. Python 3 is needed for the documentation demo, not for every project.

Check that this machine is ready:

```sh
parcela doctor
```

Doctor probes `process-compose` and `caddy` on `PATH`, `lsof`, `ps`, the nearest `parcela.yaml`, the private state directory and the HTTPS proxy. Each line is a ✓ or ✗ with a fix, and the last line names the next step. With `--json`, or when stdout is not a terminal, it prints `{ready, checks: [{name, ok, detail, fix}], notes}` and exits 1 until every check passes. Doctor installs nothing and changes no host setting. A passing version probe is not proof that the front door or the application is healthy.

The native acceptance fixtures use Caddy 2.11.7 and Process Compose 1.122.0. Other versions require their own verification. A successful version probe does not prove graph compatibility or application health.

## Prepare the front door explicitly

Normal browser URLs use HTTPS 443 and HTTP 80. The listeners stay on loopback. Caddy uses dedicated config, storage and an admin socket, separate from another Caddy installation.

Before starting a proxy, check existing listeners and decide how your host will provide those ports. Do not stop an unknown listener or silently choose a different port.

On a host where the selected unprivileged process can bind the standard loopback ports, the foreground command is:

```sh
parcela proxy run --caddy caddy
```

On a host with explicitly configured external forwarding, run the owned unprivileged listeners instead:

```sh
parcela proxy run --caddy caddy --forwarded-standard-ports \
  --http-port 8080 --https-port 8443
```

That flag does not create forwarding. It describes a mapping you have already configured. Do not use it as a fallback after a port conflict.

The source tree has an operator-only macOS helper at `scripts/setup_macos.py`. Its full setup configures guarded PF forwarding, a forwarding boot job and System-keychain CA trust. Those are separate privileged host changes. Review the helper and obtain explicit authorization before running it. Ordinary `up` never invokes it. Custom PF state, loopback skip settings and partial setup need operator diagnosis, not a blind global reload.

Caddy does not install its CA trust automatically in Parcela's profile. Trust the intended CA only through an explicit host procedure. Do not use `curl -k` or disable TLS checks to hide a trust failure.

## Verify the front door

From a separate shell using the same binary and registry:

```sh
parcela proxy status
```

The forwarded profile also checks that the standard HTTP front door matches the owned router. If it fails, diagnose [forwarding and ownership](troubleshooting.md#front-door-or-certificate-failure) before starting applications.

## Use an isolated test profile when needed

Disposable acceptance tests can explicitly use available unprivileged ports and a private registry:

```sh
parcela --state-dir "$TEST_STATE" proxy run --test-profile \
  --http-port "$TEST_HTTP_PORT" --https-port "$TEST_HTTPS_PORT"
```

Choose a new private test state directory and independently verified free ports. Test URLs include the HTTPS port. Validate TLS with that test profile's CA without changing system trust. This is a separate test setup, not proof that standard ports or browser trust work.

Once the intended front door passes its checks, [run the first environment](first-environment.md).
