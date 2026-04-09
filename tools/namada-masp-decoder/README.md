# Namada MASP Decoder

This package is the isolated execution boundary for Namada owned-note decoding.

Why it exists:
- Hera's main workspace is Apache-2.0.
- The official Namada SDK path is a separate dependency surface and should stay isolated from Stage 1 runtime crates.
- The worker already knows how to call an external decoder over `stdin` / `stdout`.

Current state:
- This binary implements the decoder JSON contract.
- By default it fails loudly on non-empty public MASP input.
- If `HERA_NAMADA_DECODER_ALLOW_STUB=1` is set, it emits deterministic stub notes for local plumbing tests only.

Contract:
- Input: `hera_namada_adapter::ExternalDecodeRequest`
- Output: `hera_namada_adapter::ExternalDecodeResponse`

Worker configuration example:

```env
NAMADA_DECODER_COMMAND=cargo
NAMADA_DECODER_ARGS=run --manifest-path tools/namada-masp-decoder/Cargo.toml --quiet --
```

Production expectation:
- Replace `decode_request` in `src/main.rs` with an implementation backed by the official Namada SDK / MASP stack.
- Keep the binary outside the core workspace so the dependency and licensing boundary remains explicit.

