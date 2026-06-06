// Shrink the C shadow stack from the wasm-ld default (1 MiB) to 256 KiB. The
// plugin's deepest call chain (serde_json descent + vodozemac crypto) needs only
// a few KiB; the 1 MiB default otherwise pushes __heap_base up by ~1 MiB, so the
// PSRAM-backed linear memory has to grow into ever-larger contiguous blocks until
// memory.grow can no longer relocate the live set and the guest traps. A smaller
// stack lowers __heap_base, shrinking the footprint and keeping the heap well
// below the relocation ceiling. This is not --initial-memory (which only leaves
// an unused gap above __heap_base): lowering the stack frees real heap headroom.
//
// Scoped to the cdylib (the wasm plugin); native `cargo test` builds a test
// harness whose host linker does not understand `-z stack-size`.
fn main() {
    println!("cargo:rustc-link-arg-cdylib=-z");
    println!("cargo:rustc-link-arg-cdylib=stack-size=262144");
}
