//! UniFFI bindings generator.
//!
//! Built only with `--features cli`; see `apple/build-core.sh`.

fn main() {
    uniffi::uniffi_bindgen_main();
}
