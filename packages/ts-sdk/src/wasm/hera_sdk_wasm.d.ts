export default function init(input?: unknown): Promise<unknown>;

export class HeraClient {
  constructor(baseUrl: string, apiKey: string);
  listWorkspaces(paginationJson?: string): Promise<string>;
  createWorkspace(requestJson: string): Promise<string>;
  createCase(requestJson: string): Promise<string>;
  getCase(caseId: string): Promise<string>;
  listCasesForWorkspace(workspaceId: string, paginationJson?: string): Promise<string>;
  importZcashViewKey(caseId: string, requestJson: string): Promise<string>;
  importNamadaViewKey(caseId: string, requestJson: string): Promise<string>;
  scanCase(caseId: string): Promise<string>;
  getCaseStatus(caseId: string): Promise<string>;
  getCaseEvents(caseId: string): Promise<string>;
  downloadCaseReportJson(caseId: string): Promise<Uint8Array>;
  downloadCaseReportPdf(caseId: string): Promise<Uint8Array>;
}
