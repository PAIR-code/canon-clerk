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

pub mod cli;

pub use cli::{Cli, init_logging, run};

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
