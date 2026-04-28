# `@hera/ts-sdk`

Thin TypeScript wrapper around the Hera Wasm bridge.

## Local build flow

1. Build the Wasm package into this package's local vendor directory.

```sh
cd packages/ts-sdk
npm run build:wasm
```

2. Typecheck or build the TypeScript wrapper.

```sh
tsc --noEmit -p tsconfig.json
tsc -p tsconfig.json
```

The current bridge uses JSON strings across the Wasm boundary on purpose. That keeps the
TypeScript layer dependency-free and avoids introducing extra serialization crates into the Rust
workspace.

## Usage shape

The generated Wasm bundle exports `HeraClient`, which this package wraps into a typed
`HeraClient` with normal TypeScript objects.

See:

- `examples/node-case-lifecycle.mts`
- `examples/browser-case-lifecycle.ts`
