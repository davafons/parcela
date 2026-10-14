# Build Parcela and prepare HTTPS

## Build the CLI

Use macOS or Linux. You need Rust 1.89 or newer and a C toolchain for bundled SQLite. Release packaging and automatic backend installation are not implemented.

Build the [`main` source branch](https://github.com/davafons/parcela/tree/main) for the commands in these guides. Select the intended revision through your project's VCS workflow before building.

From that Parcela source checkout:

```sh
cargo build --release --locked
./target/release/parcela --version
./target/release/parcela --help
```

Add that build to this shell's path:

```sh
export PATH="$PWD/target/release:$PATH"
```

This changes the current shell only. It does not replace an installed binary or restart its proxy. Keep the same binary and registry for later environment commands.

> **Coordinate upgrades.**
>
> Schema 3 supports service-set hostname and port reservations. Do not let a new build open installed state for writing while an old binary still owns its proxy or application controllers. Stop or explicitly reconcile those owners as part of an authorized upgrade.

## Check the backends

Install Caddy 2.x and Process Compose 1.x through your chosen package or release workflow. Parcela also needs `lsof` and `ps` for Unix listener and process inspection. Python 3 is needed for the documentation demo, not for every project.

Inspect the exact tools you intend to use:

```sh
parcela doctor --caddy caddy --process-compose process-compose
```

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
