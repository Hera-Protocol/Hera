import { createHeraClient } from "../dist/index.js";

async function bootstrap(): Promise<void> {
  const client = await createHeraClient({
    baseUrl: "http://127.0.0.1:3000",
    apiKey: "dev-api-key",
  });

  const workspaces = await client.listWorkspaces({ limit: 5 });
  console.log("hera workspaces", workspaces.items);
}

void bootstrap();
