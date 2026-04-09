# Namada MASP Decoder

This package is the isolated execution boundary for Namada owned-note decoding.

Why it exists:
- Hera's main workspace is Apache-2.0.
- The official Namada SDK path is a separate dependency surface and should stay isolated from Stage 1 runtime crates.
- The worker already knows how to call an external decoder over `stdin` / `stdout`.

Current state:
- This binary implements the decoder JSON contract.
- It now parses public MASP transaction batches with the official Namada MASP
  primitives and trial-decrypts owned notes with the provided viewing key.
- It fetches block timestamps from `NAMADA_RPC_URL` so emitted notes carry real
  chain time instead of fabricated local timestamps.
- It preserves the canonical MASP asset type as `asset_id`, and now enriches
  emitted notes with chain-decoded asset precision plus a friendly symbol when
  that symbol can be determined reliably. Native NAM resolves to `NAM`; other
  assets fall back to stable token identity strings when richer metadata is not
  available from the official RPC path.

Contract:
- Input: `hera_namada_adapter::ExternalDecodeRequest`
- Output: `hera_namada_adapter::ExternalDecodeResponse`

Worker configuration example:

```env
NAMADA_DECODER_COMMAND=cargo
NAMADA_DECODER_ARGS=run --manifest-path tools/namada-masp-decoder/Cargo.toml --quiet --
NAMADA_RPC_URL=https://rpc.namada.net
```
