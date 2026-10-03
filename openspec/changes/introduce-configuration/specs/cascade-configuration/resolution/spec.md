# Spec Delta

## Purpose

Defines the multi-source configuration resolution engine evaluating inputs along a deterministic hierarchy of specificity, providing lone-key provider auto-inference and multi-key ambiguity detection with advisory warnings.

## ADDED Requirements

### Requirement: Deterministic Specificity Resolution Cascade
The system SHALL evaluate model configuration inputs along a descending specificity hierarchy: explicit programmatic overrides, tier-specific environment variables, general environment variables, vendor environment variables, host OS stored credentials, and static defaults.

#### Scenario: Programmatic overrides taking highest precedence
- **WHEN** programmatic overrides are supplied alongside conflicting environment variables and stored credentials
- **THEN** the resolved configuration reflects the explicit override values

#### Scenario: Tier-specific environment taking precedence over general environment
- **WHEN** tier-specific and general environment variables are both present
- **THEN** the resolved configuration adopts the tier-specific environment value

#### Scenario: Stored OS credentials serving as fallback before defaults
- **WHEN** credentials are absent from overrides and environment variables but present in the OS store
- **THEN** the resolved configuration extracts credentials from the OS store

### Requirement: Zero-Friction Lone-Key Provider Inference
The system SHALL automatically infer the model provider and apply corresponding default models when an API key is supplied without explicit model or provider configuration.

#### Scenario: Inferring provider and defaults from lone API key
- **WHEN** only a provider-specific credential key (such as `geminiApiKey`) is available without explicit model selection
- **THEN** infers the matching provider and assigns the standard default model for the requested tier

### Requirement: Multi-Key Ambiguity Detection and Advisory Warnings
The system SHALL detect competing provider credentials in the absence of explicit provider selection, falling back to the canonical default provider while generating structured advisory warnings.

#### Scenario: Resolving default provider with advisory warning on competing credentials
- **WHEN** credentials for multiple providers exist in the store without an explicit model or provider configuration
- **THEN** falls back to the default provider and includes an advisory warning documenting the ambiguity

#### Scenario: Suppressing advisory warnings when explicit model or provider is specified
- **WHEN** multiple provider credentials exist but an explicit model or provider is specified
- **THEN** resolves the specified model without emitting ambiguity warnings

### Requirement: Multi-Tier Cascade Model Configuration Resolution
The system SHALL resolve independent model configurations for both screener and auditor tiers within a unified cascade model configuration.

#### Scenario: Resolving distinct configurations across cascade tiers
- **WHEN** resolving a cascade model configuration with tier-differentiated inputs
- **THEN** produces a cascade structure containing distinct screener and auditor model configurations
