//! Runtime support for configuration documentation.
//!
//! This crate provides the `ConfigDoc` trait and types for generating
//! configuration documentation from Rust structs.
//!
//! # Example
//!
//! ```ignore
//! use config_doc::ConfigDoc;
//!
//! #[derive(ConfigDoc)]
//! #[config_doc(header = "My Configuration")]
//! pub struct Config {
//!     /// The version of the configuration schema
//!     #[config_doc(default = "1")]
//!     pub version: u32,
//!
//!     /// List of enabled features
//!     #[config_doc(example = "[\"feature1\", \"feature2\"]")]
//!     pub features: Vec<String>,
//! }
//!
//! // Generate documentation
//! println!("{}", Config::generate_docs());
//!
//! // Generate example YAML
//! println!("{}", Config::generate_example());
//! ```

pub use config_doc_derive::ConfigDoc;

/// Documentation for a configuration field.
#[derive(Debug, Clone)]
pub struct DocField {
    /// Field name
    pub name: &'static str,
    /// Type name (as string)
    pub type_name: &'static str,
    /// Description from doc comments
    pub description: &'static str,
    /// Default value (if any)
    pub default: Option<&'static str>,
    /// Example value (if any)
    pub example: Option<&'static str>,
    /// Environment variable (if any)
    pub env: Option<&'static str>,
    /// Whether this field is required
    pub required: bool,
    /// Nested fields (for complex types)
    pub nested: Option<Vec<DocField>>,
}

/// Trait for configuration documentation.
pub trait ConfigDoc: Sized {
    /// Header text for the documentation (e.g., struct name)
    fn doc_header() -> &'static str {
        ""
    }

    /// List of documentation fields
    fn doc_fields() -> Vec<DocField>;

    /// Generate formatted documentation string.
    fn generate_docs() -> String {
        let mut output = String::new();

        let header = Self::doc_header();
        if !header.is_empty() {
            output.push_str(&format!("\x1b[1m{}\x1b[0m\n", header));
            output.push_str(&format!("{}\n\n", "─".repeat(header.len())));
        }

        for field in Self::doc_fields() {
            output.push_str(&format_field(&field));
        }

        output
    }

    /// Generate example YAML with comments.
    fn generate_example() -> String {
        let mut output = String::new();

        for field in Self::doc_fields() {
            output.push_str(&format_example_field(&field));
        }

        output
    }
}

fn format_field(field: &DocField) -> String {
    let mut output = String::new();

    let required_marker = if field.required { " (required)" } else { "" };
    output.push_str(&format!(
        "\x1b[1m\x1b[36m{}\x1b[0m{}\n",
        field.name, required_marker
    ));

    output.push_str(&format!(
        "  \x1b[2mType:\x1b[0m {}\n",
        clean_type_name(field.type_name)
    ));

    if !field.description.is_empty() {
        output.push_str(&format!("  {}\n", field.description));
    }

    if let Some(default) = field.default {
        output.push_str(&format!("  \x1b[2mDefault:\x1b[0m {}\n", default));
    }

    if let Some(example) = field.example {
        output.push_str(&format!("  \x1b[2mExample:\x1b[0m {}\n", example));
    }

    if let Some(env) = field.env {
        output.push_str(&format!("  \x1b[2mEnv:\x1b[0m {}\n", env));
    }

    output.push('\n');
    output
}

fn format_example_field(field: &DocField) -> String {
    let mut output = String::new();

    if !field.description.is_empty() {
        for line in field.description.lines() {
            output.push_str(&format!("# {}\n", line));
        }
    }

    if let Some(default) = field.default {
        output.push_str(&format!("# Default: {}\n", default));
    }

    if let Some(example) = field.example {
        output.push_str(&format!("{}: {}\n\n", field.name, example));
    } else {
        output.push_str(&format!(
            "# {}: <{}>\n\n",
            field.name,
            clean_type_name(field.type_name)
        ));
    }

    output
}

fn clean_type_name(type_name: &str) -> String {
    let type_name = type_name
        .replace("std::collections::HashMap", "map")
        .replace("std::collections::HashSet", "set")
        .replace("alloc::string::String", "string")
        .replace("alloc::vec::Vec", "array");

    type_name.trim().to_string()
}
