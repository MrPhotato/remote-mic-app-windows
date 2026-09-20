# Release signature verifier

Verifies the installer bytes against a Tauri `.sig` and public-key file. Both
files must contain base64-encoded minisign text. This tool never reads a private
key. It prints only `passed` (exit 0) or `failed` (exit 1), including usage and I/O
failures.

```powershell
cargo run --locked --release --manifest-path tools/release-signature-verify/Cargo.toml --target-dir tools/release-signature-verify/target -- <installer> <signature.sig> <public-key-file>
cargo test --locked --manifest-path tools/release-signature-verify/Cargo.toml --target-dir tools/release-signature-verify/target
```

This is an independent workspace with its own lockfile and output directory;
it does not build the application. Cargo itself prints build output; invoke the
built `release-signature-verify` binary directly when only the result is needed.

Verification uses the public `PublicKey::decode`, `Signature::decode`, and
`PublicKey::verify(..., true)` APIs from
[minisign-verify 0.2.5](https://crates.io/crates/minisign-verify/0.2.5), matching
the legacy/prehashed acceptance of `tauri-plugin-updater` 2.11.0's
`verify_signature`. The public test vectors are from that minisign-verify
version's `src/lib.rs` (Frank Denis, MIT); no signing keys are included.
