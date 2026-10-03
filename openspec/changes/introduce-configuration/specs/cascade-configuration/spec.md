# Spec Delta

## Purpose

Defines the domain capability for configuring the Three-Phase Evaluation Cascade across screener and auditor tiers, providing pure model configuration contracts and canonical provider specifier parsing without ambient host environment dependencies.

## ADDED Requirements

### Requirement: Pure Evaluation Cascade Configuration Contracts
The system SHALL declare immutable model configuration contracts (`ModelConfig`, `CascadeModelConfig`) and cascade tier enumerations (`screener`, `auditor`) as explicit domain types within the core engine without importing host filesystem or configuration dependencies.

#### Scenario: Declaring tier-specific configuration contracts
- **WHEN** a consumer initializes evaluation cascade options with a typed model configuration
- **THEN** the configuration specifies model, provider, modelName, and optional credentials as immutable properties

#### Scenario: Encapsulating model provider and endpoint parameters
- **WHEN** configuring a model targeting a local or remote model endpoint
- **THEN** the configuration encapsulates optional provider API key and base URL parameters

### Requirement: Canonical Model Specifier Parsing
The system SHALL parse model specifier strings formatted as `provider:model` into structured provider and model name identifiers, defaulting to the canonical system provider when the provider prefix is omitted.

#### Scenario: Parsing explicit provider and model name
- **WHEN** parsing a specifier such as `"google:gemini-3.5-flash-lite"`
- **THEN** returns provider `"google"` and modelName `"gemini-3.5-flash-lite"`

#### Scenario: Defaulting provider when omitting prefix
- **WHEN** parsing a specifier without a colon delimiter such as `"gemini-3.1-pro"`
- **THEN** returns the default provider `"google"` and modelName `"gemini-3.1-pro"`
