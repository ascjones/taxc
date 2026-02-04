use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Lit, Meta, Type};

/// Derive macro that generates CSV schema information from struct fields.
///
/// For each field, extracts:
/// - Field name (respects #[serde(rename = "...")])
/// - Required (true if not Option<T>)
/// - Description (from doc comments)
///
/// Generates a `csv_schema() -> &'static [CsvField]` method.
#[proc_macro_derive(CsvSchema, attributes(serde))]
pub fn derive_csv_schema(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => panic!("CsvSchema only supports structs with named fields"),
        },
        _ => panic!("CsvSchema only supports structs"),
    };

    let field_info: Vec<_> = fields
        .iter()
        .map(|field| {
            let field_name = field.ident.as_ref().unwrap().to_string();

            // Check for #[serde(rename = "...")]
            let csv_name = get_serde_rename(&field.attrs).unwrap_or(field_name);

            // Check if type is Option<T>
            let is_optional = is_option_type(&field.ty);

            // Extract doc comments
            let doc = get_doc_comment(&field.attrs);

            (csv_name, !is_optional, doc)
        })
        .collect();

    let field_entries = field_info.iter().map(|(name, required, desc)| {
        quote! {
            CsvField {
                name: #name,
                required: #required,
                description: #desc,
            }
        }
    });

    let expanded = quote! {
        impl #name {
            pub fn csv_schema() -> &'static [CsvField] {
                static SCHEMA: &[CsvField] = &[
                    #(#field_entries),*
                ];
                SCHEMA
            }
        }
    };

    TokenStream::from(expanded)
}

fn get_serde_rename(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }

        if let Meta::List(meta_list) = &attr.meta {
            let tokens = meta_list.tokens.to_string();
            // Simple parsing: look for rename = "..."
            if let Some(start) = tokens.find("rename") {
                let rest = &tokens[start..];
                if let Some(eq_pos) = rest.find('=') {
                    let after_eq = rest[eq_pos + 1..].trim();
                    if let Some(stripped) = after_eq.strip_prefix('"') {
                        if let Some(end_quote) = stripped.find('"') {
                            return Some(stripped[..end_quote].to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

fn get_doc_comment(attrs: &[syn::Attribute]) -> String {
    attrs
        .iter()
        .filter_map(|attr| {
            if !attr.path().is_ident("doc") {
                return None;
            }
            if let Meta::NameValue(meta) = &attr.meta {
                if let syn::Expr::Lit(expr_lit) = &meta.value {
                    if let Lit::Str(lit_str) = &expr_lit.lit {
                        return Some(lit_str.value().trim().to_string());
                    }
                }
            }
            None
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Option";
        }
    }
    false
}
