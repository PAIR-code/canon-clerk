# Diagnostic Health Check (`probe`)

**Status:** Authoritative Architectural Standard  
**Core Domain Engine:** Caseload Domain Engine  
**Driving Adapter:** CLI (`probe`)

---

## 1. Domain Concept & Role (`core` / `configuration`)

`probe` serves as the **Diagnostic Network Health Check** for configured model endpoints. In the court clerkship taxonomy, it represents testing the teleconferencing equipment and audio-visual communication links before a remote hearing begins.

- **Imperative Verb:** `probe`
- **Court Clerkship Role:** Live connectivity and latency verification of provider endpoints.
- **Metric Pair:** N/A (Live connectivity probe; reports `status` and `latency_ms`).

---

## 2. Dependencies & Prerequisites (`core`)

```mermaid
flowchart LR
    subgraph Inputs["Inputs"]
        Config["configure (or --caseload)<br/><code>.config</code>"]
        Params["Stage Flags<br/><i>(--timeout)</i>"]
    end

    Config --> Probe["probe<br/><b>(Current Node)</b><br/><code>.probe</code>"]
    Params --> Probe

    Probe --> Report["Diagnostic Health Report<br/><i>(Endpoint latency & reachability, Exit 0)</i>"]
    Probe -. "connectivity failure" .-> Failure["Connection Error<br/><i>(Unreachable / bad key, Exit 1)</i>"]

    style Probe fill:#1f6feb,stroke:#58a6ff,stroke-width:2px,color:#fff
    style Failure stroke-dasharray: 5 5
```

- **Direct Prerequisites:** `configure` (requires resolved provider credentials and model specifiers in `caseload.config`).
- **Transitive Prerequisites:** None.
- **Pruned from Cascades:** `probe` is **never executed during review cascades** (`canon-clerk audit`). This ensures that evaluation runs do not incur redundant health-check latency prior to screening.

---

## 3. Caseload Contribution & Data Plane Constraints

### Caseload Delta
`probe` populates the `.probe` namespace on the cumulative `Caseload` envelope with live diagnostic health records for the configured model tiers:

- **Screener Endpoint Health (`screener`):** Connectivity probe result for `.config.screener_model`, capturing connection `status` (`"ok"` or `"error"`), roundtrip `latency_ms`, resolved `model` identifier, and an optional diagnostic `error` message when unreachable or unauthorized.
- **Auditor Endpoint Health (`auditor`):** Connectivity probe result for `.config.auditor_model` with the same diagnostic fields (`status`, `latency_ms`, `model`, and optional `error`).

### Data Plane & Diagnostic Invariants
1. **Cascade Isolation:** `.probe` is an auxiliary diagnostic leaf namespace; because `probe` is pruned from all evaluation cascades (`docket`, `admit`, `audit`, `apprise`), substantive review runs never populate or depend on `.probe`.
2. **Non-Throwing Diagnostic Capture:** Endpoint timeouts, HTTP 4xx/5xx responses, and authentication rejections are captured inside the respective endpoint health record (`status: "error"`, `error: "<reason>"`) on `.probe` before the CLI adapter maps any failure to exit code `2`.

---

## 4. Process & Domain Logic (`configuration`)

1. **Endpoint Resolution:** Reads configured `screener_model` and `auditor_model` from `caseload.config`.
2. **Ping Request Assembly:** Constructs lightweight probe requests (0 reasoning tokens, minimum prompt payload) to verify authentication and reachability.
3. **Concurrent Probe:** Concurrently sends probe requests to both endpoints in parallel:
   - Measures roundtrip response latency in milliseconds (`latency_ms`).
   - Verifies model availability and provider credential validity.
4. **Diagnostic Record:** Attaches endpoint latency and reachability metadata to `Caseload.probe`.

---

## 5. Driving Adapter: CLI

The CLI exposes `probe` as an imperative subcommand:

```bash
# Human-readable connectivity and latency check:
canon-clerk probe

# Emit structured JSON probe record:
canon-clerk probe --json
```

### CLI Flags & Options
- `--timeout <ms>`: Request timeout in milliseconds (default: `5000ms`).
- `--caseload <path|->`: Ingests upstream Caseload.
- `--json`: Emits enriched Caseload JSON.

### CLI Exit Codes
- **0:** All configured endpoints are reachable, authenticated, and responsive.
- **2:** Provider credentials rejected (401/403) or endpoints unreachable (timeout / 5xx).
