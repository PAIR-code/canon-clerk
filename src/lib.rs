// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Canon Clerk: Automated review gate enforcing declarative engineering rule packs.

pub mod admit;
pub mod apprise;
pub mod audit;
pub mod bench;
pub mod builtin;
pub mod canon;
pub mod caseload;
pub mod cli;
mod concurrency;
pub mod configure;
pub mod discover;
pub mod docket;
pub mod github;
pub mod intake;
pub mod policy;
pub mod probe;
pub mod provider;
pub(crate) mod spans;
pub mod telemetry;
pub mod validate;
pub mod vet;

pub use admit::{
    AdmitError, AdmitOptions, DEFAULT_ADMIT_MAX_CONCURRENT_CALLS, DEFAULT_ADMIT_THRESHOLD,
    ENV_ADMIT_CONCURRENCY, ENV_ADMIT_THRESHOLD, execute_admit,
};
pub use apprise::{
    AppriseError, AppriseOptions, DEFAULT_APPRISE_MAX_CONCURRENT_CALLS, DEFAULT_APPRISE_THRESHOLD,
    ENV_APPRISE_BATCH_SIZE, ENV_APPRISE_CONCURRENCY, ENV_APPRISE_THRESHOLD, execute_apprise,
};
pub use audit::{
    AuditError, AuditOptions, DEFAULT_AUDIT_MAX_CONCURRENT_CALLS, DEFAULT_AUDIT_THRESHOLD,
    DEFAULT_AUDIT_TIMEOUT_MS, ENV_AUDIT_CONCURRENCY, ENV_AUDIT_THRESHOLD, execute_audit,
    resolve_canon_reference_paths,
};
pub use bench::{
    BenchIoStreams, BenchmarkStage, ConfigConsistencyReport, ConsistencyBenchCli,
    ConsistencyBenchError, ConsistencyBenchFormat, ConsistencyBenchOptions, ConsistencyBenchReport,
    ConsistencyConfigSpec, DEFAULT_BENCHMARK_TRIALS, ENV_BENCH_BATCH_SIZE, ENV_BENCH_CONCURRENCY,
    ENV_BENCH_FORMAT, ENV_BENCH_INCLUDE_STABLE, ENV_BENCH_MODEL, ENV_BENCH_OUTPUT_JSON,
    ENV_BENCH_STAGE, ENV_BENCH_THRESHOLD, ENV_BENCH_TRIALS, HermeticBenchDeps, SubjectConsistency,
    SubjectObservation, TrialSummary, execute_consistency_bench,
    execute_consistency_bench_observed, render_consistency_markdown, run_consistency_bench_cli,
    run_consistency_bench_hermetic,
};
pub use builtin::{BUILTIN_CANONS, BuiltinCanon, builtin_canons, find_builtin_canon};
pub use canon::{
    Canon, CanonBody, CanonMetadata, CanonParseError, CanonScope, CanonSourceInfo, CanonSourceKind,
    CanonToken, CanonTokenKind, DEFAULT_FALLBACK_ID, DEFAULT_GLOBAL_TRIGGER_PATTERN,
    DEFAULT_INSPECT_PLANES, DirectiveName, DirectiveOccurrence, ExtractedFrontmatter,
    FrontmatterKeySpan, InspectPlane, InspectToken, ParseCanonOptions, RawFrontmatter, SourceSpan,
    assemble_canon_from_tokens, derive_diagnostic_path, derive_exists, derive_id, derive_inspect,
    derive_metadata, derive_references, derive_scope, derive_tags, derive_title, derive_triggers,
    extract_first_heading, extract_first_heading_from_tokens, extract_frontmatter,
    extract_frontmatter_from_tokens, has_directory_traversal, id_to_title_case,
    normalize_canon_content, parse_body, parse_body_from_tokens, parse_canon, resolve_source_info,
    scope_pattern, to_kebab_case, tokenize_canon,
};
pub use caseload::{
    ActiveExhibits, AdmissibilityStatus, AnnotationLevel, ApprisalStatus, CASELOAD_SCHEMA_VERSION,
    CanonAdjudication, CanonApprisalAssessment, CanonDocketAssessment, CanonVettingAssessment,
    Caseload, CaseloadApprisal, CaseloadConfig, CaseloadDiscovery, CaseloadDocket,
    CaseloadEvidence, CaseloadIntake, CaseloadProbe, CaseloadValidation, CaseloadVerdict,
    CaseloadVetting, CodeAnnotation, ColorabilityStatus, ContentOmissionReason, ContestedSpan,
    DEFAULT_AUDIT_THINKING_LEVEL, DEFAULT_GOOGLE_DISCRIMINATOR_MODEL,
    DEFAULT_GOOGLE_GENERATOR_MODEL, DEFAULT_LOCAL_DISCRIMINATOR_MODEL,
    DEFAULT_PROBE_THINKING_LEVEL, DEFAULT_PROVIDER, DEFAULT_TYPESAFE_DISCRIMINATOR_MODEL,
    DiscoveryMode, ExhibitAssessment, FileArtifact, FileChangeStatus, HeuristicNode, IntakeScope,
    LOCAL_PROVIDER, LinkedIssueContext, PatchOmissionReason, ProbeStatus, ProbeTargetResult,
    ResolvedNodeConfig, TYPESAFE_PROVIDER, ThinkingLevel, ValidatedCanonMetadata,
    ValidationOutcome, VerdictStatus, VettingStatus, WorkspaceState, canonicalize_contested_spans,
    normalize_admissibility_score, normalize_colorability_score, normalize_compliance_score,
    normalize_unit_score, normalize_vetting_score, sort_annotations,
};
pub use cli::{
    AdmitArgs, AdmitFormat, AdmitNodeModelArgs, AllNodeModelArgs, AppriseArgs, AppriseFormat,
    AppriseNodeModelArgs, AuditArgs, AuditFormat, AuditNodeModelArgs, CanonSelectionArgs,
    CaseloadIoArgs, Cli, CliError, Commands, ConfigureArgs, ConfigureFormat, DiffContextArgs,
    DiscoverArgs, DiscoverFormat, DocketArgs, DocketFormat, DocketNodeModelArgs, ENV_LOG_FILE,
    ENV_VERBOSE, FilingSourceArgs, GithubCheckArgs, IntakeArgs, OnMissingCredentials, ProbeArgs,
    ProbeFormat, ProviderConfigArgs, ProviderTransportArgs, RejectedUnqualifiedTuningArgs,
    ResolvedFilingSource, StageDisposition, StderrProgressSink, StderrTelemetryOptions,
    ValidateArgs, ValidateFormat, VetArgs, VetFormat, VetNodeModelArgs, WorkspaceStateArg,
    build_cli_telemetry_sink, init_logging, open_jsonl_log_sink, resolve_log_file_path,
    resolve_verbose_flag, run, run_admit, run_admit_live_with_telemetry,
    run_admit_with_snapshot_and_provider, run_admit_with_telemetry, run_apprise,
    run_apprise_live_with_telemetry, run_apprise_with_snapshot_and_provider,
    run_apprise_with_telemetry, run_audit, run_audit_live_with_telemetry,
    run_audit_with_snapshot_and_provider, run_audit_with_telemetry, run_configure,
    run_configure_with_env, run_configure_with_telemetry, run_discover, run_discover_in_workspace,
    run_discover_with_telemetry, run_docket, run_docket_live_with_telemetry,
    run_docket_with_snapshot_and_provider, run_docket_with_telemetry, run_intake,
    run_intake_with_telemetry, run_probe, run_probe_live_with_telemetry,
    run_probe_with_env_and_prober, run_probe_with_telemetry, run_validate,
    run_validate_in_workspace, run_validate_with_telemetry, run_vet, run_vet_live_with_telemetry,
    run_vet_with_snapshot_and_provider, run_vet_with_telemetry,
};
pub use configure::{
    AuthPolicy, ConfigureError, ConfigureOptions, ConfigureScope, DEFAULT_LOCAL_BASE_URL,
    ENV_ADMIT_MODEL, ENV_APPRISE_MODEL, ENV_AUDIT_MODEL, ENV_DOCKET_MODEL, ENV_GEMINI_API_KEY,
    ENV_GEMINI_BASE_URL, ENV_LOCAL_API_KEY, ENV_LOCAL_BASE_URL, ENV_PROVIDER, ENV_REASONING_BUDGET,
    ENV_THINKING_LEVEL, ENV_TYPESAFE_API_KEY, ENV_TYPESAFE_BASE_URL, ENV_VET_MODEL,
    ModelCapability, PROVIDER_CAPABILITIES, ProtocolDriver, ProviderAuth, ProviderCapability,
    ProviderConnection, ProviderProfile, ProviderRegistry, StoredConfig, StoredDefaultsConfig,
    StoredNodeConfig, StoredProviderConfig, execute_configure, is_discriminative_model,
    load_host_config_file, model_capability, normalize_model_specifier, parse_thinking_level,
    provider_capability, resolve_default_config_path, resolve_gemini_api_key,
    resolve_provider_connections,
};
pub use discover::{
    DEFAULT_CANON_GLOB, DEFAULT_IGNORES, DiscoverError, DiscoverOptions, GlobPattern,
    GlobSyntaxError, WorkspaceSnapshot, execute_discover, execute_discover_on_disk,
    is_ignored_path, is_wildcard_pattern, normalize_workspace_path, parse_canon_or_fallback,
    scope_relative_path,
};
pub use docket::{
    DEFAULT_DOCKET_MAX_CONCURRENT_CALLS, DEFAULT_DOCKET_THRESHOLD, DocketError, DocketOptions,
    ENV_DOCKET_BATCH_SIZE, ENV_DOCKET_CONCURRENCY, ENV_DOCKET_THRESHOLD, execute_docket,
};
pub use github::{
    CheckRunConclusion, CheckRunPublisher, CheckRunReport, DEFAULT_GITHUB_API_TIMEOUT_MS,
    DEFAULT_GITHUB_API_URL, DEFAULT_GITHUB_CHECK_NAME, ENV_GH_TOKEN, ENV_GITHUB_API_URL,
    ENV_GITHUB_EVENT_NAME, ENV_GITHUB_EVENT_PATH, ENV_GITHUB_REPOSITORY, ENV_GITHUB_SHA,
    ENV_GITHUB_TOKEN, GITHUB_CHECKS_MAX_ANNOTATIONS_PER_REQUEST, GITHUB_CHECKS_MAX_OUTPUT_CHARS,
    GithubCheckTarget, GithubChecksClient, GithubChecksError, GithubPullRequestEventContext,
    GithubPullRequestFetcher, GithubPullRequestMetadata, GithubPullRequestTarget,
    PublishedCheckRun, ResolvedGithubPullRequest, is_pull_request_target_event,
    parse_pr_context_from_event_path, parse_pr_event_json, resolve_github_check_target,
    resolve_github_check_target_with_fallback_sha, resolve_github_pr_filing,
};
pub use intake::{IntakeError, IntakeOptions, execute_intake, parse_unified_diff};
pub use policy::{
    DiscoveredPolicyFiles, POLICY_CHAINS_FILENAME, PolicyAction, PolicyError, PolicyFilePathKind,
    WorkspacePolicySet, classify_policy_file_path, detect_nested_policy_files,
    discover_policy_files_on_disk, is_direct_policy_file_path,
};
pub use probe::{
    DEFAULT_PROBE_TIMEOUT_MS, ENV_PROBE_TIMEOUT_MS, EndpointProber, GeminiDecisionsEndpointProber,
    GeminiEndpointProber, MultiProviderEndpointProber, ProbeError, ProbeOptions,
    TypesafeEndpointProber, execute_probe,
};
pub use provider::{
    DEFAULT_BASE_BACKOFF_MS, DEFAULT_COOLDOWN_SLOT_MS, DEFAULT_GEMINI_BASE_URL,
    DEFAULT_GENERATION_TIMEOUT_MS, DEFAULT_LOCAL_GENERATION_TIMEOUT_MS,
    DEFAULT_LOCAL_MAX_CONCURRENT_CALLS, DEFAULT_MAX_BACKOFF_MS, DEFAULT_MAX_RETRIES,
    DEFAULT_TYPESAFE_BASE_URL, DecisionBudgetProfile, DecisionModelProvider, DecisionRequest,
    DecisionResponse, ENV_MAX_BACKOFF_MS, ENV_MAX_RETRIES, ENV_TIMEOUT_MS, GeminiDecisionProvider,
    GeminiModelProvider, GenerationRequest, GenerationResponse, MultiModelProvider, ProviderError,
    RetryNotification, RetryObserver, RetryPolicy, SharedHttpRequester, StructuredModelProvider,
    TokenUsage, TypesafeDecisionProvider, default_concurrency_for_model,
    default_timeout_ms_for_model,
};
pub use spans::{DEFAULT_DIFF_HEAD_LINES, ENV_DIFF_HEAD_LINES, MAX_HEAD_CONTEXT_CHARS};
pub use telemetry::{
    CompositeTelemetrySink, InstrumentedModelProvider, JsonlWriterSink, NoopTelemetryClock,
    NoopTelemetrySink, PipelineStage, RecordingTelemetrySink, StageScope, SteppingTelemetryClock,
    SystemTelemetryClock, TelemetryClock, TelemetryEvent, TelemetrySession, TelemetrySink,
    format_unix_millis_iso8601,
};
pub use validate::{
    ValidateError, ValidateOptions, execute_validate, execute_validate_on_disk, lint_canon_source,
};
pub use vet::{
    DEFAULT_VET_EXCEPTION_THRESHOLD, DEFAULT_VET_MAX_CONCURRENT_CALLS, DEFAULT_VET_THRESHOLD,
    ENV_VET_CONCURRENCY, ENV_VET_THRESHOLD, VetError, VetOptions, execute_vet,
};

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct BaselineManifest {
        name: String,
        version: String,
    }

    #[test]
    fn test_foundational_serde_serialization() {
        let manifest = BaselineManifest {
            name: "canon-clerk".to_string(),
            version: "0.1.0".to_string(),
        };

        let json = serde_json::to_string(&manifest).expect("serialization must succeed");
        let decoded: BaselineManifest =
            serde_json::from_str(&json).expect("deserialization must succeed");

        assert_eq!(manifest, decoded);
    }
}
