use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Serialize, Deserialize)]
pub struct PromotionLog {
    pub promotions: Vec<PromotionEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PromotionEntry {
    pub timestamp: String,
    pub source: String,
    pub destination: String,
    pub git_ref: String,
    pub promoted: Vec<FileRecord>,
    pub skipped: Vec<SkipRecord>,
    pub protected: Vec<ProtectedRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileRecord {
    pub path: String,
    pub action: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SkipRecord {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProtectedRecord {
    pub path: String,
    pub reason: String,
}

impl PromotionEntry {
    pub fn new(source: String, destination: String, git_ref: String) -> Self {
        Self {
            timestamp: OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
            source,
            destination,
            git_ref,
            promoted: Vec::new(),
            skipped: Vec::new(),
            protected: Vec::new(),
        }
    }

    pub fn add_promoted(&mut self, path: PathBuf, action: &str) {
        self.promoted.push(FileRecord {
            path: path.display().to_string(),
            action: action.to_string(),
        });
    }

    pub fn add_skipped(&mut self, path: PathBuf, reason: &str) {
        self.skipped.push(SkipRecord {
            path: path.display().to_string(),
            reason: reason.to_string(),
        });
    }

    pub fn add_protected(&mut self, path: PathBuf, reason: &str) {
        self.protected.push(ProtectedRecord {
            path: path.display().to_string(),
            reason: reason.to_string(),
        });
    }
}

impl PromotionLog {
    pub fn new() -> Self {
        Self {
            promotions: Vec::new(),
        }
    }

    pub fn load(path: &std::path::Path) -> Result<Self, std::io::Error> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            serde_yaml::from_str(&content)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        } else {
            Ok(Self::new())
        }
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        let content = serde_yaml::to_string(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, content)
    }

    pub fn add_entry(&mut self, entry: PromotionEntry) {
        self.promotions.push(entry);
    }
}
