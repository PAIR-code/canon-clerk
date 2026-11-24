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

//! Bifurcated discriminative `Noul` question keys and prompt templates for `vet`
//! (`docs/architecture/nodes/vet.md` §4.3).

use super::slices::{VettingExhibitSlice, VettingSpanTarget};

pub(super) const COMPLIANCE_QUESTION_KEY: &str = "compliant";
pub(super) const EXCEPTION_QUESTION_KEY: &str = "claims_exception";

const VET_NOUL_INSTRUCTION: &str = "\
Evaluate the Admitted Exhibits (and any Statutory Grounding References) in `state` strictly \
against the Governing Canon Statute's Normative Invariant (`What`) without applying `Exception` clauses:\n\
- `Precedent` (`Which`), `Rationale` (`Why`), and `Remediation` (`How`) are non-normative; NEVER fail code that satisfies \
  the Normative Invariant via a valid alternative mechanism or invent unstated requirements from cited standards.\n\
- Inline comments, docstrings, or prose claiming an exception or waiver do not satisfy the Normative Invariant by themselves.\n\
- Do NOT invent extra requirements beyond the written Normative Invariant or speculate about un-admitted files \
  (when `state` presents an exhibit slice, `## All Admitted File Exhibit Paths in Filing` lists all files admitted across the filing).\n\n\
Question: Do the Admitted Exhibits in `state` directly comply with the Governing Canon Statute's Normative Invariant?\n\
- Answer YES if the Admitted Exhibits directly satisfy the Normative Invariant, OR if the canon's governed construct \
  is absent/untouched in these exhibits (zero violation annotations required).\n\
- Answer NO ONLY if a violation of the Normative Invariant or missing required companion update is actually present \
  in the exhibits (even if a comment claims an exception).";

const VET_EXCEPTION_NOUL_INSTRUCTION: &str = "\
Inspect the Admitted Exhibits (and any metadata exhibits) in `state` relative to the Governing Canon Statute.\n\
- Consider BOTH explicit inline self-attestations (comments, docstrings, or prose claiming an exception, waiver, exemption, \
  or justifying why the Normative Invariant is intentionally bypassed) AND structural/situational reliance on a declared `Exception` clause.\n\
- Ignore unrelated uses of the word 'exception' in ordinary error-handling prose (e.g., runtime exceptions, try/catch) \
  that do not pertain to waiving or bypassing the Governing Canon Statute.\n\n\
Question: Do the Admitted Exhibits claim, invoke, or rely on an exception, exemption, or waiver to the Governing Canon Statute?\n\
- Answer YES if the exhibits contain an inline comment/waiver justifying non-compliance with this canon OR appear to rely \
  on a declared `Exception` clause rather than directly satisfying the Normative Invariant.\n\
- Answer NO if the exhibits directly satisfy the Normative Invariant (or the governed construct is absent/untouched) \
  with no exception claim or reliance.";

pub(super) fn file_question_key(file_path: &str) -> String {
    format!("file:{file_path}")
}

pub(super) fn file_exception_question_key(file_path: &str) -> String {
    format!("file_exception:{file_path}")
}

pub(super) fn span_question_key(span_id: &str) -> String {
    format!("span:{span_id}")
}

pub(super) fn span_exception_question_key(span_id: &str) -> String {
    format!("span_exception:{span_id}")
}

fn file_noul_instruction(file_path: &str) -> String {
    format!(
        "Evaluate ONLY the admitted file exhibit `{file_path}` in `state` against the Governing Canon \
         Statute's Normative Invariant (`What`) without applying `Exception` clauses.\n\
         Inline comments, docstrings, or prose claiming an exception or waiver do not satisfy the Normative Invariant by themselves.\n\
         Question: Does `{file_path}` directly comply with the Governing Canon Statute's Normative Invariant?\n\
         - Answer YES if `{file_path}` directly satisfies the Normative Invariant, OR if the canon's governed \
         construct is absent/untouched in `{file_path}`.\n\
         - Answer NO ONLY if a violation of the Normative Invariant or missing required companion update is present \
         in `{file_path}` (even if a comment claims an exception)."
    )
}

fn file_exception_noul_instruction(file_path: &str) -> String {
    format!(
        "Inspect ONLY the admitted file exhibit `{file_path}` in `state` relative to the Governing Canon Statute.\n\
         Consider BOTH explicit inline self-attestations (comments, docstrings, or prose claiming an exception, waiver, \
         or justifying non-compliance) AND structural/situational reliance on a declared `Exception` clause; ignore \
         unrelated runtime/language exception error-handling prose.\n\
         Question: Does `{file_path}` claim, invoke, or rely on an exception, exemption, or waiver to the Governing Canon Statute?\n\
         - Answer YES if `{file_path}` contains an inline comment/waiver justifying non-compliance with this canon OR relies on a declared `Exception` clause.\n\
         - Answer NO if `{file_path}` directly satisfies the Normative Invariant (or the governed construct is absent/untouched) with no exception claim or reliance."
    )
}

fn span_noul_instruction(span: &VettingSpanTarget) -> String {
    format!(
        "Evaluate ONLY `[SPAN {}]` (`{}` L{}..L{}) in `state` against the Governing Canon Statute's \
         Normative Invariant (`What`) without applying `Exception` clauses (considering enclosing scope).\n\
         Inline comments, docstrings, or prose claiming an exception or waiver do not satisfy the Normative Invariant by themselves.\n\
         Question: Does `[SPAN {}]` directly comply with the Governing Canon Statute's Normative Invariant?\n\
         - Answer YES if `[SPAN {}]` directly satisfies the Normative Invariant, OR if the canon's governed \
         construct is absent/untouched in `[SPAN {}]`.\n\
         - Answer NO ONLY if a violation of the Normative Invariant is present in `[SPAN {}]` (even if a comment claims an exception).",
        span.span_id,
        span.file_path,
        span.start_line,
        span.end_line,
        span.span_id,
        span.span_id,
        span.span_id,
        span.span_id
    )
}

fn span_exception_noul_instruction(span: &VettingSpanTarget) -> String {
    format!(
        "Inspect ONLY `[SPAN {}]` (`{}` L{}..L{}) in `state` relative to the Governing Canon Statute \
         (considering enclosing scope).\n\
         Consider BOTH explicit inline self-attestations (comments, docstrings, or prose claiming an exception, waiver, \
         or justifying non-compliance) AND structural/situational reliance on a declared `Exception` clause; ignore \
         unrelated runtime/language exception error-handling prose.\n\
         Question: Does `[SPAN {}]` claim, invoke, or rely on an exception, exemption, or waiver to the Governing Canon Statute?\n\
         - Answer YES if `[SPAN {}]` contains an inline comment/waiver justifying non-compliance with this canon OR relies on a declared `Exception` clause.\n\
         - Answer NO if `[SPAN {}]` directly satisfies the Normative Invariant (or the governed construct is absent/untouched) with no exception claim or reliance.",
        span.span_id,
        span.file_path,
        span.start_line,
        span.end_line,
        span.span_id,
        span.span_id,
        span.span_id
    )
}

pub(super) fn build_slice_noul_items(slice: &VettingExhibitSlice) -> Vec<(String, String)> {
    let mut items = Vec::with_capacity(2 * (1 + slice.file_paths.len() + slice.spans.len()));
    items.push((
        COMPLIANCE_QUESTION_KEY.to_string(),
        VET_NOUL_INSTRUCTION.to_string(),
    ));
    items.push((
        EXCEPTION_QUESTION_KEY.to_string(),
        VET_EXCEPTION_NOUL_INSTRUCTION.to_string(),
    ));
    for file_path in &slice.file_paths {
        items.push((
            file_question_key(file_path),
            file_noul_instruction(file_path),
        ));
        items.push((
            file_exception_question_key(file_path),
            file_exception_noul_instruction(file_path),
        ));
    }
    for span in &slice.spans {
        items.push((
            span_question_key(&span.span_id),
            span_noul_instruction(span),
        ));
        items.push((
            span_exception_question_key(&span.span_id),
            span_exception_noul_instruction(span),
        ));
    }
    items
}
