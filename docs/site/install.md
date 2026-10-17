# Install Parcela

## Install the CLI

Use macOS or Linux.

Install the binary into `~/.local/bin`:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/davafons/parcela/releases/latest/download/parcela-installer.sh | sh
```

Release builds target macOS on Apple silicon and Intel, and Linux on x86_64 and arm64 with glibc 2.35 or newer. Each archive includes `LICENSE`, `COPYRIGHT` and `THIRD_PARTY_NOTICES.txt`. The release also attaches the matching source archive.

From source, with Rust 1.89 or newer and a C toolchain for bundled SQLite:

```sh
cargo install --git https://github.com/davafons/parcela --locked
```

Then check the binary:

```sh
parcela --version &&
  parcela --help
```

Expect `parcela 0.1.0` followed by command help.

An installed binary does not restart a running proxy. Keep the same binary and registry for later environment commands.

To print instructions your agent can read:

```sh
parcela --skill
```

The command prints Markdown without reading or creating project or registry state. Printing it does not install a skill.

- [Install the operating skill for Claude Code](agents.md#install-the-skill-for-claude-code) with a copyable project-local command.
- [Ask your existing agent to configure this checkout](agents.md#tldr-ask-your-agent-to-configure-this-checkout) without a separate AI setup service.

> **Coordinate upgrades.**
>
> A new build can migrate the registry schema. Stop running environments and the proxy with the old binary (`parcela down --all-projects`, then `parcela proxy stop`) before upgrading.

## Check the backends

Install Caddy 2.x and Process Compose 1.x once:

```sh
# macOS
brew install caddy f1bonacc1/tap/process-compose

# Linux: Caddy from your distribution or caddyserver.com, then Process Compose
sh -c "$(curl -fsSL https://raw.githubusercontent.com/F1bonacc1/process-compose/main/scripts/get-pc.sh)" -- -b ~/.local/bin
```
 Parcela also needs `lsof` and `ps` for Unix listener and process inspection. Python 3 is needed for the documentation demo, not for every project.

Check that this machine is ready:

```sh
parcela doctor
```

Doctor probes `process-compose` and `caddy` on `PATH`, `lsof`, `ps`, the nearest `parcela.yaml`, the private state directory and the HTTPS proxy. Each line is a ✓ or ✗ with a fix, and the last line names the next step. With `--json`, or when stdout is not a terminal, it prints `{ready, checks: [{name, ok, detail, fix}], notes}` and exits 1 until every check passes. Doctor installs nothing and changes no host setting. A passing version probe is not proof that the front door or the application is healthy.

The native acceptance fixtures use Caddy 2.11.7 and Process Compose 1.122.0. Other versions require their own verification. A successful version probe does not prove graph compatibility or application health.

## The front door

Parcela's proxy is a dedicated Caddy with its own config, storage and admin socket, separate from any other Caddy installation. It listens on loopback only. `parcela up` starts it in the background when it is not running, so there is no separate proxy terminal.

By default the proxy serves HTTPS on port 1443 and every URL carries that port, for example `https://notes.localhost:1443`. This needs no sudo and no port forwarding. To choose another port, start the proxy once with it; the port is saved for later starts:

```sh
parcela proxy start --port 4443
```

If the port is busy, the command fails and names the process holding it. Parcela never stops an unknown listener or falls back to another port. While nothing has been saved, `PARCELA_HTTPS_PORT` replaces the default 1443.

Check the proxy from any shell that uses the same binary and state directory:

```sh
parcela proxy status
```

It prints the mode, port, process IDs, log path and the Parcela version that started the proxy. After upgrading Parcela, `parcela proxy restart` replaces a proxy started by the old version; Parcela warns about the mismatch but never restarts it by itself. `parcela proxy stop` stops it. Running services keep running, but their URLs stop answering until the proxy starts again.

## Trust the local certificate authority

Caddy issues certificates from a private certificate authority in Parcela's state directory. Browsers warn until that authority is trusted. Parcela changes system trust only when you run:

```sh
parcela trust
```

It says which certificate it adds and how to undo it, then runs `caddy trust` against Parcela's own storage and admin socket. Caddy may ask for your password. To use the certificate without trusting it, for example with `curl --cacert`, print its path:

```sh
parcela trust --print
```

Do not use `curl -k` or disable TLS checks to hide a trust failure.

## Standard ports

URLs without a port need ports 443 and 80 forwarded to the proxy's unprivileged internal listeners. That is a separate, explicitly authorized host change. The source tree has an operator-only macOS helper at `scripts/setup_macos.py` that configures guarded PF forwarding and a forwarding boot job. Review the helper before running it. Ordinary commands never invoke it. Custom PF state, loopback skip settings and partial setup need operator diagnosis, not a blind global reload.

With forwarding in place, the proxy runs in standard mode from the saved front-door profile described in the [configuration reference](../configuration.md#front-door). Its status check also confirms that the forwarded port 80 reaches the owned proxy. If it fails, diagnose [forwarding and ownership](troubleshooting.md#front-door-or-certificate-failure) before starting applications.

## Use an isolated test setup when needed

Disposable acceptance tests use a private state directory and an explicit free port:

```sh
parcela --state-dir "$TEST_STATE" proxy start --port "$TEST_HTTPS_PORT"
```

Validate TLS with the certificate from `parcela --state-dir "$TEST_STATE" trust --print` without changing system trust, and stop the proxy with `proxy stop` when done.

Once the intended front door passes its checks, [run the first environment](first-environment.md).
