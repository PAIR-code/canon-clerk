import { describe, expect, it } from 'vitest';
import {
  classifyProbeError,
  getMissingCredentialsHint,
} from './probe-classifier.js';

describe('classifyProbeError', () => {
  it('classifies TimeoutError and timeout messages as timeout', () => {
    const timeoutErr = new Error('The operation was aborted due to timeout');
    timeoutErr.name = 'TimeoutError';
    const result = classifyProbeError(timeoutErr, 'screener', 'google');
    expect(result.category).toBe('timeout');
    expect(result.hint).toContain('--probe-timeout');

    const abortTimeout = new Error('Request timed out after 15000ms');
    expect(classifyProbeError(abortTimeout, 'auditor', 'google').category).toBe('timeout');

    const causeTimeout = { cause: { code: 'ETIMEDOUT' }, message: 'Connection failed' };
    expect(classifyProbeError(causeTimeout, 'screener', 'google').category).toBe('timeout');
  });

  it('classifies DNS and network transport failures as network_error', () => {
    const dnsErr = new Error('fetch failed');
    (dnsErr as { cause?: unknown }).cause = { code: 'ENOTFOUND', message: 'getaddrinfo ENOTFOUND generativelanguage.googleapis.com' };
    const result = classifyProbeError(dnsErr, 'screener', 'google');
    expect(result.category).toBe('network_error');
    expect(result.hint).toContain('internet connection');

    const connRefused = { cause: { code: 'ECONNREFUSED' }, message: 'connect ECONNREFUSED 127.0.0.1:11434' };
    expect(classifyProbeError(connRefused, 'screener', 'ollama').category).toBe('network_error');
  });

  it('classifies HTTP 401 as authentication', () => {
    const err = { statusCode: 401, message: 'Unauthorized' };
    const result = classifyProbeError(err, 'screener', 'google');
    expect(result.category).toBe('authentication');
    expect(result.hint).toContain('CANON_CLERK_SCREENER_API_KEY');
    expect(result.hint).toContain('https://aistudio.google.com/apikey');
  });

  it('classifies HTTP 400 with API key invalid message as authentication', () => {
    const err = {
      statusCode: 400,
      message: 'API key not valid. Please pass a valid API key.',
      responseBody: '{"error": {"status": "INVALID_ARGUMENT", "message": "API_KEY_INVALID"}}',
    };
    const result = classifyProbeError(err, 'auditor', 'google');
    expect(result.category).toBe('authentication');
    expect(result.hint).toContain('CANON_CLERK_AUDITOR_API_KEY');
  });

  it('classifies HTTP 400 without key signatures as bad_request', () => {
    const err = {
      statusCode: 400,
      message: 'Invalid JSON payload received. Unknown field: foo',
    };
    const result = classifyProbeError(err, 'screener', 'google');
    expect(result.category).toBe('bad_request');
    expect(result.hint).toContain('rejected the generation request payload');
  });

  it('classifies HTTP 403 as authorization', () => {
    const err = {
      statusCode: 403,
      message: 'Generative Language API has not been used in project 12345 before or it is disabled.',
    };
    const result = classifyProbeError(err, 'screener', 'google');
    expect(result.category).toBe('authorization');
    expect(result.hint).toContain('Generative Language API is enabled');
  });

  it('classifies HTTP 404 and model not found messages as model_not_found', () => {
    const err = {
      statusCode: 404,
      message: 'models/gemini-3.1-pro is not found for API version v1beta',
    };
    const result = classifyProbeError(err, 'auditor', 'google');
    expect(result.category).toBe('model_not_found');
    expect(result.hint).toContain('CANON_CLERK_AUDITOR_MODEL');
  });

  it('classifies HTTP 429 as rate_limited', () => {
    const err = {
      statusCode: 429,
      message: 'Resource exhausted: quota exceeded for quota metric generate_content_requests',
    };
    const result = classifyProbeError(err, 'screener', 'google');
    expect(result.category).toBe('rate_limited');
    expect(result.hint).toContain('quota');
  });

  it('classifies unstatused error matching substring signatures', () => {
    const err = new Error('PERMISSION_DENIED: User does not have access');
    expect(classifyProbeError(err, 'screener', 'google').category).toBe('authorization');

    const quotaErr = new Error('RESOURCE_EXHAUSTED: Rate limit hit');
    expect(classifyProbeError(quotaErr, 'screener', 'google').category).toBe('rate_limited');
  });

  it('falls back to unknown for unclassified error', () => {
    const err = new Error('Something completely unexpected occurred');
    const result = classifyProbeError(err, 'screener', 'custom-provider');
    expect(result.category).toBe('unknown');
    expect(result.hint).toContain('--format json');
  });
});

describe('getMissingCredentialsHint', () => {
  it('returns Google-specific guidance', () => {
    const hint = getMissingCredentialsHint('screener', 'google');
    expect(hint).toContain('CANON_CLERK_SCREENER_API_KEY or GEMINI_API_KEY');
    expect(hint).toContain('https://aistudio.google.com/apikey');
  });

  it('returns Anthropic-specific guidance', () => {
    const hint = getMissingCredentialsHint('auditor', 'anthropic');
    expect(hint).toContain('CANON_CLERK_AUDITOR_API_KEY or ANTHROPIC_API_KEY');
    expect(hint).toContain('https://console.anthropic.com/');
  });

  it('returns generic provider guidance', () => {
    const hint = getMissingCredentialsHint('screener', 'custom');
    expect(hint).toContain("Set CANON_CLERK_SCREENER_API_KEY for provider 'custom'");
  });
});
