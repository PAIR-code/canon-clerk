# Tasks

## 1. Domain Diagnostics and Resolution Engine
- [ ] 1.1 Expose configuration inspection with granular property-level source attribution in `@canon-clerk/configuration`
- [ ] 1.2 Implement POSIX owner-only permission bitmask verification in credential store inspection utilities
- [ ] 1.3 Add unit tests verifying granular attribution and permission auditing across diverse filesystem modes

## 2. Formatting and Presentation
- [ ] 2.1 Implement stylish tree formatter displaying host store health, tier configuration trees, and granular sources
- [ ] 2.2 Implement canonical JSON formatter emitting structured diagnostic payloads with masked secrets and validity status
- [ ] 2.3 Implement secret masking utility enforcing trailing-only redaction across formatted outputs

## 3. CLI Subcommand and Routing
- [ ] 3.1 Implement `check-config` command action handling options (`--tier`, `-f/--format`, `--json`, `-q/--quiet`, `--max-warnings`)
- [ ] 3.2 Wire `check-config` subcommand into root CLI program and categorized help screens in `packages/cli`
- [ ] 3.3 Add comprehensive unit and integration tests covering exit codes, quiet execution, warning thresholds, and formatters
- [ ] 3.4 Verify monorepo checks pass across all lanes (`npm run check`)
