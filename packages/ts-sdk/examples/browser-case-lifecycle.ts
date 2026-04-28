import init, * as wasm from "../vendor/hera-sdk-wasm/hera_sdk_wasm.js";
import { HeraClient } from "../dist/index.js";

async function bootstrap(): Promise<void> {
  await init();

  const client = new HeraClient(wasm, {
    baseUrl: "http://127.0.0.1:3000",
    apiKey: "dev-api-key",
  });

  const workspaces = await client.listWorkspaces({ limit: 5 });
  console.log("hera workspaces", workspaces.items);
}

void bootstrap();
