import type {
  CanonicalEvent,
  CaseDetailResponse,
  CaseSummaryResponse,
  CreateCaseRequest,
  CreateCaseResponse,
  CreateWorkspaceRequest,
  CreateWorkspaceResponse,
  GetCaseStatusResponse,
  ImportViewingKeyRequest,
  ImportViewingKeyResponse,
  PaginatedResponse,
  PaginationParams,
  ScanCaseResponse,
  WorkspaceSummaryResponse,
} from "./types";

export type {
  CanonicalEvent,
  CaseDetailResponse,
  CaseSummaryResponse,
  ChainId,
  Counterparty,
  CounterpartyVisibility,
  CreateCaseRequest,
  CreateCaseResponse,
  CreateWorkspaceRequest,
  CreateWorkspaceResponse,
  EventMemo,
  EventProvenance,
  EventType,
  GetCaseStatusResponse,
  ImportViewingKeyRequest,
  ImportViewingKeyResponse,
  Network,
  PaginatedResponse,
  PaginationParams,
  ScanCaseResponse,
  ScanJobStatus,
  WorkspaceSummaryResponse,
} from "./types";

export interface HeraWasmBindings {
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
}

export interface HeraWasmModule {
  default(input?: unknown): Promise<unknown>;
  HeraClient: new (baseUrl: string, apiKey: string) => HeraWasmBindings;
}

export interface HeraClientConfig {
  baseUrl: string;
  apiKey: string;
}

export class HeraClient {
  private readonly inner: HeraWasmBindings;

  constructor(wasm: HeraWasmModule, config: HeraClientConfig) {
    this.inner = new wasm.HeraClient(config.baseUrl, config.apiKey);
  }

  async listWorkspaces(
    pagination: PaginationParams = {},
  ): Promise<PaginatedResponse<WorkspaceSummaryResponse>> {
    return parseJson(
      await this.inner.listWorkspaces(encodeOptionalJson(pagination)),
    );
  }

  async createWorkspace(
    request: CreateWorkspaceRequest,
  ): Promise<CreateWorkspaceResponse> {
    return parseJson(await this.inner.createWorkspace(JSON.stringify(request)));
  }

  async createCase(request: CreateCaseRequest): Promise<CreateCaseResponse> {
    return parseJson(await this.inner.createCase(JSON.stringify(request)));
  }

  async getCase(caseId: string): Promise<CaseDetailResponse> {
    return parseJson(await this.inner.getCase(caseId));
  }

  async listCasesForWorkspace(
    workspaceId: string,
    pagination: PaginationParams = {},
  ): Promise<PaginatedResponse<CaseSummaryResponse>> {
    return parseJson(
      await this.inner.listCasesForWorkspace(
        workspaceId,
        encodeOptionalJson(pagination),
      ),
    );
  }

  async importZcashViewKey(
    caseId: string,
    request: ImportViewingKeyRequest,
  ): Promise<ImportViewingKeyResponse> {
    return parseJson(
      await this.inner.importZcashViewKey(caseId, JSON.stringify(request)),
    );
  }

  async importNamadaViewKey(
    caseId: string,
    request: ImportViewingKeyRequest,
  ): Promise<ImportViewingKeyResponse> {
    return parseJson(
      await this.inner.importNamadaViewKey(caseId, JSON.stringify(request)),
    );
  }

  async scanCase(caseId: string): Promise<ScanCaseResponse> {
    return parseJson(await this.inner.scanCase(caseId));
  }

  async getCaseStatus(caseId: string): Promise<GetCaseStatusResponse> {
    return parseJson(await this.inner.getCaseStatus(caseId));
  }

  async getCaseEvents(caseId: string): Promise<CanonicalEvent[]> {
    return parseJson(await this.inner.getCaseEvents(caseId));
  }
}

export async function loadEmbeddedWasmModule(): Promise<HeraWasmModule> {
  const wasm = (await import("./wasm/hera_sdk_wasm.js")) as HeraWasmModule;
  await wasm.default();
  return wasm;
}

export async function createHeraClient(
  config: HeraClientConfig,
  wasmModule?: HeraWasmModule,
): Promise<HeraClient> {
  return new HeraClient(wasmModule ?? (await loadEmbeddedWasmModule()), config);
}

function parseJson<T>(value: string): T {
  return JSON.parse(value) as T;
}

function encodeOptionalJson(value: PaginationParams): string | undefined {
  if (value.limit === undefined && value.offset === undefined) {
    return undefined;
  }

  return JSON.stringify(value);
}
