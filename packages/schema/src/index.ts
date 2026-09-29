export interface Canon {
  id: string;
  title: string;
  directive?: 'Requirement' | 'Guidance' | 'Supplement' | 'Exception';
  exceptions?: string[];
  triggers?: {
    paths?: string[];
    [key: string]: unknown;
  };
}

export const SCHEMA_VERSION = '1.0.0';
