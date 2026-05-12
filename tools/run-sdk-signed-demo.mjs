import { mkdir, writeFile } from "node:fs/promises";
import { createHeraClient } from "../packages/ts-sdk/dist/index.js";

const baseUrl = process.env.HERA_API_URL ?? "http://127.0.0.1:3101";
const apiKey = process.env.HERA_API_KEY ?? "hera-demo-1778618450";
const outputDir = process.env.HERA_OUTPUT_DIR ?? "./tmp/sdk-signed-demo";
const namadaViewingKey =
  process.env.HERA_NAMADA_VIEW_KEY ??
  "zvknam1qurswpc8qurswpc8qurswpc8qurswpc8qurswpc8qurswpc8qurszeg67w";
const birthdayHeight = Number.parseInt(process.env.HERA_BIRTHDAY_HEIGHT ?? "6509877", 10);

function normalizeStatus(status) {
  if (typeof status === "string") {
    return status;
  }

  if (status && typeof status === "object" && "FAILED" in status) {
    return `FAILED: ${status.FAILED}`;
  }

  return JSON.stringify(status);
}

async function sleep(ms) {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

async function main() {
  const client = await createHeraClient({ baseUrl, apiKey });

  const workspace = await client.createWorkspace({
    name: `SDK Signed Report Demo ${new Date().toISOString()}`,
  });
  const newCase = await client.createCase({
    workspace_id: workspace.id,
    chain: "NAMADA",
    network: "MAINNET",
  });
  const importResult = await client.importNamadaViewKey(newCase.id, {
    raw_key: namadaViewingKey,
    birthday_height: birthdayHeight,
  });
  const scan = await client.scanCase(newCase.id);

  let finalStatus = null;
  for (let attempt = 0; attempt < 90; attempt += 1) {
    const status = await client.getCaseStatus(newCase.id);
    const label = normalizeStatus(status.status);
    if (label === "SIGNED" || label.startsWith("FAILED")) {
      finalStatus = status;
      break;
    }
    await sleep(1000);
  }

  if (!finalStatus) {
    throw new Error("timed out waiting for scan status");
  }

  const finalLabel = normalizeStatus(finalStatus.status);
  if (finalLabel !== "SIGNED") {
    throw new Error(`scan did not sign report: ${finalLabel}`);
  }

  const reportJson = await client.downloadCaseReportJson(newCase.id);
  const reportPdf = await client.downloadCaseReportPdf(newCase.id);

  await mkdir(outputDir, { recursive: true });
  const jsonPath = `${outputDir}/report-${newCase.id}.json`;
  const pdfPath = `${outputDir}/report-${newCase.id}.pdf`;
  const summaryPath = `${outputDir}/summary-${newCase.id}.json`;

  await writeFile(jsonPath, reportJson, "utf8");
  await writeFile(pdfPath, reportPdf);

  const summary = {
    baseUrl,
    workspace,
    case: newCase,
    importResult,
    scan,
    finalStatus: finalLabel,
    reportJsonPath: jsonPath,
    reportPdfPath: pdfPath,
    reportPdfLength: reportPdf.length,
  };

  await writeFile(summaryPath, `${JSON.stringify(summary, null, 2)}\n`, "utf8");
  console.log(JSON.stringify(summary, null, 2));
}

await main();
