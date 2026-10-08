## Server

```
ghcr.io/oghma/sparagne-server:{{VERSION}}
```

For linux/amd64 and linux/arm64, digest `{{DIGEST}}`. To check that it was
built by this repository's release workflow from this tag:

```
gh attestation verify oci://ghcr.io/oghma/sparagne-server:{{VERSION}} --repo Oghma/Sparagne
```

**Upgrade the server before the apps**: back it up, set
`SPARAGNE_VERSION={{VERSION}}` in `server/deploy/.env`, then
`docker compose pull sparagne && docker compose up -d sparagne`
(`docs/DEPLOY.md` §6).

## App (macOS 27, Apple silicon)

The app is not attached: build it from this tag's sources on the Mac that
runs it.

```
git checkout v{{VERSION}}
bash scripts/build-app.sh --install
```

It needs Xcode 27, Rust (the version in `rust-toolchain.toml`) and xcodegen;
see the README.
