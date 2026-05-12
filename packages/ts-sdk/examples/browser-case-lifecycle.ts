import { createHeraClient } from "../dist/index.js";

async function bootstrap(): Promise<void> {
  const client = await createHeraClient({
    baseUrl: "http://127.0.0.1:3000",
    apiKey: "dev-api-key",
  });

  const workspaces = await client.listWorkspaces({ limit: 5 });
  console.log("hera workspaces", workspaces.items);

  const signedCaseId = new URLSearchParams(window.location.search).get("caseId");
  if (!signedCaseId) {
    return;
  }

  const pdfBytes = await client.downloadCaseReportPdf(signedCaseId);
  const pdfBlob = new Blob([new Uint8Array(pdfBytes)], { type: "application/pdf" });
  const pdfUrl = URL.createObjectURL(pdfBlob);

  const link = document.createElement("a");
  link.href = pdfUrl;
  link.download = `hera-report-${signedCaseId}.pdf`;
  link.textContent = "Download Hera PDF report";
  document.body.append(link);
}

void bootstrap();
