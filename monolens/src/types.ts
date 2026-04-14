// Shared TypeScript interfaces for MonoLens.
// All types that cross module boundaries live here.

// ─── Domain state ─────────────────────────────────────────────────────────────

/** Visual and functional state of a domain in the workspace. */
export type DomainState = 'ghost' | 'loading' | 'loaded' | 'shared';

// ─── Graph (graph.json schema) ────────────────────────────────────────────────

export type EdgeType = 'static_import' | 'co_change';

export interface GraphNode {
  /** Repo-relative path, e.g. "apis/sales" */
  path: string;
  /** "service" | "library" | "shared" */
  type: string;
}

export interface GraphEdge {
  from: string;
  to: string;
  type: string;
  /** 0–1 weight for co_change edges; 1.0 for static imports */
  weight: number;
  score: number;
  commit_count?: number;
}

/** Root schema of .sam/graph.json */
export interface Graph {
  version: string;
  generated_at: string;
  domains: string[];
  edges: GraphEdge[];
}

/** A dependency with its edge metadata, returned by GraphClient queries. */
export interface Dependency {
  domain: string;
  type: EdgeType;
  score: number;
}

// ─── Workspace state (.sam/workspace.yaml) ─────────────────────────────────────

export interface WorkspaceStatus {
  activeProfile: string;
  hydratedDomains: string[];
  lastUpdated: string;
  monographAnalyzed: boolean;
}

// ─── Profile schema (.sam/profiles.yaml) ──────────────────────────────────────

export interface Profile {
  domains: string[] | '*';
  auto_include?: string[];
  ai_infer?: boolean;
  watch?: string[];
  owners?: string[];
}

export interface ProfilesConfig {
  version: string;
  profiles: Record<string, Profile>;
}

// ─── sam CLI results ─────────────────────────────────────────────────────────

export interface PlanEntry {
  domain: string;
  reason: string;
  files: number;
  status: 'new' | 'already_hydrated';
}

export interface PlanResult {
  domain: string;
  entries: PlanEntry[];
  totalNewFiles: number;
}

export interface ImpactEntry {
  domain: string;
  risk: 'critical' | 'high' | 'medium' | 'low';
  callsDay: number;
  score: number;
}

export interface ImpactResult {
  entries: ImpactEntry[];
}

export interface HydrateResult {
  domain: string;
  filesAdded: number;
  success: boolean;
  errorMessage?: string;
}

// ─── Hydration panel ──────────────────────────────────────────────────────────

export interface HydrationPreview {
  requestedDomain: string;
  entries: HydrationPreviewEntry[];
}

export interface HydrationPreviewEntry {
  domain: string;
  reason: 'requested' | 'auto_include' | 'ai_inferred' | 'already_hydrated';
  fileCount: number;
  score?: number;
}

// ─── Extension config ─────────────────────────────────────────────────────────

export interface MonoLensConfig {
  samBinaryPath: string;
  showImpactGutter: boolean;
  minCochangeScore: number;
}
