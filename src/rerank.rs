//! Stage 3: final ordering.
//!
//! Why listwise on top of the Noul scores: the two signals are complementary —
//! the per-file Nouls calibrate keep/drop against the lane threshold but
//! under-resolve order near the top, while one listwise Choice ranks the whole
//! kept set in a single ~0.3 s call. The Noul decides what is kept; the Choice
//! distribution orders it. The API allows ≤255 options per Choice; kept sets
//! are ≤30, far inside the limit.

use crate::jev::{client::JevClient, questions, schemas::*};
use serde_json::json;

/// One Choice over all kept files; returns (path, rank probability) sorted
/// descending. `kept` pairs each path with a ~25-line head snippet for state.
/// `any_doc` selects the content-neutral rerank phrasing when documents are
/// present; all-code requests keep the original phrasing byte-for-byte, so
/// cache keys and calibration are unchanged.
pub async fn rank_kept(
    client: &JevClient,
    model: &str,
    concept: &str,
    kept: &[(String, String)],
    any_doc: bool,
) -> anyhow::Result<Vec<(String, f64)>> {
    let request = JevRequest {
        state: json!({
            "concept": concept,
            "files": kept.iter().enumerate().map(|(i, (path, head))| json!({
                "id": format!("f{i}"), "path": path, "head": head,
            })).collect::<Vec<_>>(),
        }),
        model: model.into(),
        questions: [(
            "rank".to_string(),
            questions::listwise_rank((0..kept.len()).map(|i| format!("f{i}")), any_doc),
        )]
        .into(),
    };
    let resp = client.system_one(&request).await?;
    let Some(Answer::Choice { probabilities, .. }) = resp.answers.get("rank") else {
        anyhow::bail!("expected choice answer for rank");
    };
    let mut ranked: Vec<_> = kept
        .iter()
        .enumerate()
        .map(|(i, (path, _))| {
            let p = probabilities.get(&format!("f{i}")).copied().unwrap_or(0.0);
            (path.clone(), p)
        })
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    Ok(ranked)
}
