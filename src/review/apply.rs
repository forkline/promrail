use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::error::{AppResult, PromrailError};
use crate::review::analyze::PromotionAnalysis;
use crate::review::models::{ReviewArtifact, ReviewArtifactStatus, ReviewDecision};

pub fn artifact_ready_for_apply(
    artifact: &ReviewArtifact,
    analysis: &PromotionAnalysis,
) -> AppResult<bool> {
    if artifact.status != ReviewArtifactStatus::Classified {
        return Ok(false);
    }

    if artifact.fingerprint != analysis.fingerprint {
        return Ok(false);
    }

    let expected_ids: HashSet<_> = analysis
        .review_items
        .iter()
        .map(|item| item.id.clone())
        .collect();
    let actual_ids: HashSet<_> = artifact.items.iter().map(|item| item.id.clone()).collect();
    Ok(expected_ids == actual_ids)
}

pub fn apply_review_decisions(
    artifact: &ReviewArtifact,
    analysis: &PromotionAnalysis,
) -> AppResult<HashMap<PathBuf, (String, PathBuf)>> {
    let mut approved = analysis.auto_files.clone();

    for item in &artifact.items {
        match item.decision {
            Some(ReviewDecision::Promote) => {
                let selected_source = match item.selected_source.as_ref() {
                    Some(source) => source.clone(),
                    None if item.candidate_sources.len() == 1 => item.candidate_sources[0].clone(),
                    None => {
                        return Err(PromrailError::ReviewArtifactInvalid(format!(
                            "review item {} is promoted but selected_source is missing",
                            item.id
                        )));
                    }
                };

                if !item.candidate_sources.contains(&selected_source) {
                    return Err(PromrailError::ReviewArtifactInvalid(format!(
                        "review item {} selected invalid source {}",
                        item.id, selected_source
                    )));
                }

                for file in &item.files {
                    let relative = PathBuf::from(file);
                    let source_path = analysis
                        .source_lookup
                        .get(&(relative.clone(), selected_source.clone()))
                        .ok_or_else(|| {
                            PromrailError::ReviewArtifactInvalid(format!(
                                "review item {} references missing source file {} from {}",
                                item.id, file, selected_source
                            ))
                        })?
                        .clone();
                    approved.insert(relative, (selected_source.clone(), source_path));
                }
            }
            Some(ReviewDecision::Skip) => {}
            None => {
                return Err(PromrailError::ReviewArtifactInvalid(format!(
                    "review item {} is missing a decision",
                    item.id
                )));
            }
        }
    }

    Ok(approved)
}
