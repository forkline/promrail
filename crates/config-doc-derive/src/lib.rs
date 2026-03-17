//! Derive macro for generating configuration documentation.
//!
//! # Example
//!
//! ```ignore
//! use config_doc::ConfigDoc;
//!
//! #[derive(ConfigDoc)]
//! pub struct Config {
//!     /// Config schema version
//!     #[config_doc(default = "1")]
//!     pub version: u32,
//!
//!     /// Repository definitions
//!     #[config_doc(example = "gitops: { path: ~/gitops }")]
//!     pub repos: HashMap<String, RepoConfig>,
//! }
//! ```

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Lit, Meta, parse_macro_input};

#[proc_macro_derive(ConfigDoc, attributes(config_doc))]
pub fn derive_config_doc(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match derive_config_doc_impl(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn derive_config_doc_impl(input: DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let header = extract_header(&input.attrs)?;

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return Err(syn::Error::new_spanned(
                    &input,
                    "ConfigDoc only supports structs with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                &input,
                "ConfigDoc only supports structs",
            ));
        }
    };

    let field_docs: Vec<TokenStream2> = fields
        .iter()
        .map(|field| {
            let field_name = field.ident.as_ref().unwrap();
            let field_type = &field.ty;
            let description = extract_docs(&field.attrs);
            let attrs = extract_config_attrs(&field.attrs);

            let default = match attrs.default {
                Some(s) => quote!(Some(#s)),
                None => quote!(None),
            };
            let example = match attrs.example {
                Some(s) => quote!(Some(#s)),
                None => quote!(None),
            };
            let env = match attrs.env {
                Some(s) => quote!(Some(#s)),
                None => quote!(None),
            };
            let required = attrs.required;

            quote! {
                config_doc::DocField {
                    name: stringify!(#field_name),
                    type_name: stringify!(#field_type),
                    description: #description,
                    default: #default,
                    example: #example,
                    env: #env,
                    required: #required,
                    nested: None,
                }
            }
        })
        .collect();

    Ok(quote! {
        impl config_doc::ConfigDoc for #name {
            fn doc_header() -> &'static str {
                #header
            }

            fn doc_fields() -> Vec<config_doc::DocField> {
                vec![#(#field_docs),*]
            }
        }
    })
}

fn extract_header(attrs: &[syn::Attribute]) -> syn::Result<&'static str> {
    for attr in attrs {
        if attr.path().is_ident("config_doc") {
            let mut header_value: Option<String> = None;

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("header") {
                    let value: Lit = meta.value()?.parse()?;
                    if let Lit::Str(lit) = value {
                        header_value = Some(lit.value());
                    }
                }
                Ok(())
            })?;

            if let Some(value) = header_value {
                return Ok(Box::leak(value.into_boxed_str()));
            }
        }
    }
    Ok("")
}

fn extract_docs(attrs: &[syn::Attribute]) -> String {
    attrs
        .iter()
        .filter_map(|attr| {
            if attr.path().is_ident("doc")
                && let Some(Meta::NameValue(nv)) = attr.parse_args().ok()
                && let syn::Expr::Lit(expr_lit) = nv.value
                && let Lit::Str(lit) = expr_lit.lit
            {
                return Some(lit.value());
            }
            None
        })
        .collect::<Vec<_>>()
        .join(" ")
}

struct ConfigAttrs {
    default: Option<String>,
    example: Option<String>,
    env: Option<String>,
    required: bool,
}

fn extract_config_attrs(attrs: &[syn::Attribute]) -> ConfigAttrs {
    let mut result = ConfigAttrs {
        default: None,
        example: None,
        env: None,
        required: false,
    };

    for attr in attrs {
        if attr.path().is_ident("config_doc") {
            let _ = attr.parse_nested_meta(|meta| {
                let path_ident = meta.path.get_ident();
                if let Some(ident) = path_ident {
                    let ident_str = ident.to_string();

                    // Check for boolean flags like "required"
                    if ident_str == "required" {
                        result.required = true;
                        return Ok(());
                    }

                    // Otherwise, expect a value
                    let value: Lit = meta.value()?.parse()?;
                    if let Lit::Str(lit) = value {
                        match ident_str.as_str() {
                            "default" => result.default = Some(lit.value()),
                            "example" => result.example = Some(lit.value()),
                            "env" => result.env = Some(lit.value()),
                            _ => {}
                        }
                    }
                }
                Ok(())
            });
        }
    }

    result
}
