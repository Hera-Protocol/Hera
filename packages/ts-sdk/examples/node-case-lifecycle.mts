import { createHeraClient } from "../dist/index.js";
import { writeFile } from "node:fs/promises";

async function main(): Promise<void> {
  const client = await createHeraClient({
    baseUrl: process.env.HERA_API_URL ?? "http://127.0.0.1:3000",
    apiKey: process.env.HERA_API_KEY ?? "dev-api-key",
  });

  const workspaces = await client.listWorkspaces({ limit: 10 });
  console.log("workspace count", workspaces.items.length);

  const caseId = process.env.HERA_CASE_ID;
  if (!caseId) {
    return;
  }

  const pdfBytes = await client.downloadCaseReportPdf(caseId);
  await writeFile(`hera-report-${caseId}.pdf`, pdfBytes);
  console.log("saved report", `hera-report-${caseId}.pdf`);
}

void main();
