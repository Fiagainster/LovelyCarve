//! Generate CLI dispatch from the actual desktop command registry. A new IPC
//! command automatically becomes a CLI command; unsupported signatures fail builds.
use std::{collections::BTreeSet, error::Error, fs, path::PathBuf};

use quote::{quote, ToTokens};
use syn::{parse::Parser, punctuated::Punctuated, visit::Visit, FnArg, Pat, Token, Type};

#[path = "cli_catalog/source.rs"]
mod source;

#[derive(Default)]
struct Registry(Vec<syn::Path>);
impl<'ast> Visit<'ast> for Registry {
    fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
        if invocation
            .path
            .segments
            .last()
            .is_some_and(|part| part.ident == "generate_handler")
        {
            self.0.extend(
                Punctuated::<syn::Path, Token![,]>::parse_terminated
                    .parse2(invocation.tokens.clone())
                    .expect("invalid command registry"),
            );
        }
        syn::visit::visit_macro(self, invocation);
    }
}

fn camel_case(name: &str) -> String {
    let mut parts = name.split('_');
    let mut output = parts.next().unwrap_or_default().to_owned();
    for part in parts {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            output.extend(first.to_uppercase());
        }
        output.extend(chars);
    }
    output
}

pub fn generate() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=build/cli_catalog.rs");
    println!("cargo:rerun-if-changed=build/cli_catalog/source.rs");
    let mut registry = Registry::default();
    registry.visit_file(&syn::parse_file(&fs::read_to_string("src/lib.rs")?)?);
    if registry.0.is_empty() {
        return Err("empty CLI command registry".into());
    }
    let mut sources = source::Sources::new("src/commands.rs");
    let mut names = BTreeSet::new();
    let mut arms = Vec::new();
    let mut catalog = Vec::new();
    for path in registry.0 {
        let parts: Vec<_> = path
            .segments
            .iter()
            .map(|part| part.ident.to_string())
            .collect();
        if parts.len() < 3 || parts[0] != "commands" {
            return Err("unsupported command path".into());
        }
        // The leaf module retains the historical CLI group after domain nesting.
        let module = &parts[parts.len() - 2];
        let name = &parts[parts.len() - 1];
        if !names.insert(name.clone()) {
            return Err(format!("duplicate registered command: {name}").into());
        }
        let function = sources.function(&parts[1..parts.len() - 1], name)?;
        let mut parameters = Vec::new();
        let mut arguments = Vec::new();
        let mut bindings = Vec::new();
        let mut keys = Vec::new();
        for argument in &function.sig.inputs {
            let FnArg::Typed(argument) = argument else {
                return Err("command receiver".into());
            };
            let Pat::Ident(pattern) = argument.pat.as_ref() else {
                return Err("command pattern".into());
            };
            let ident = &pattern.ident;
            let Type::Path(kind) = argument.ty.as_ref() else {
                return Err("unsupported command argument".into());
            };
            let kind_name = kind
                .path
                .segments
                .last()
                .ok_or("empty type")?
                .ident
                .to_string();
            match kind_name.as_str() {
                "AppHandle" => arguments.push(quote!(app.clone())),
                "State" => arguments.push(quote!(app.state::<crate::state::AppState>())),
                // Media leases use an explicit CLI owner rather than a WebView.
                "WebviewWindow" if name == "fs_media_temp" => arguments.push(quote!(None)),
                "WebviewWindow" | "Window" | "Channel" | "Request" => {
                    return Err(format!("CLI adapter required: {name}").into())
                }
                _ => {
                    let key = camel_case(&ident.to_string());
                    let required = kind_name != "Option";
                    bindings
                        .push(quote!(let #ident = super::argument(&mut args, #key, #required)?;));
                    arguments.push(quote!(#ident));
                    keys.push(key.clone());
                    parameters.push(serde_json::json!({"name":key,"flag":ident.to_string().replace('_', "-"),"type":argument.ty.to_token_stream().to_string(),"required":required}));
                }
            }
        }
        let call = if name == "fs_media_temp" {
            quote!(crate::commands::preview::filesystem_preview::fs_media_temp_for_owner(#(#arguments),*))
        } else {
            quote!(crate::#path(#(#arguments),*))
        };
        let call = if function.sig.asyncness.is_some() {
            quote!(#call.await)
        } else {
            call
        };
        let returns_result = matches!(&function.sig.output, syn::ReturnType::Type(_, kind) if matches!(kind.as_ref(), Type::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "Result")));
        let call = if returns_result {
            quote!(#call.map_err(super::command_error)?)
        } else {
            call
        };
        arms.push(quote!(#name => {
            super::validate_keys(&args, &[#(#keys),*])?;
            #(#bindings)*
            super::response(#call)
        }));
        let docs = function
            .attrs
            .iter()
            .filter_map(|attr| {
                if !attr.path().is_ident("doc") {
                    return None;
                }
                let syn::Meta::NameValue(value) = &attr.meta else {
                    return None;
                };
                let syn::Expr::Lit(value) = &value.value else {
                    return None;
                };
                let syn::Lit::Str(value) = &value.lit else {
                    return None;
                };
                Some(value.value().trim().to_owned())
            })
            .collect::<Vec<_>>()
            .join(" ");
        catalog.push(serde_json::json!({"name":name,"group":module,"description":docs,"parameters":parameters}));
    }
    for file in sources.files() {
        println!("cargo:rerun-if-changed={}", file.display());
    }
    let generated = quote! {
        use tauri::Manager;
        pub async fn dispatch(app: &tauri::AppHandle, command: &str, mut args: serde_json::Map<String, serde_json::Value>) -> Result<super::Output, super::CliError> {
            match command { #(#arms),*, _ => Err(super::CliError::invalid(format!("未知命令: {command}"))) }
        }
    };
    let out = PathBuf::from(std::env::var_os("OUT_DIR").ok_or("OUT_DIR missing")?);
    fs::write(out.join("cli_dispatch.rs"), generated.to_string())?;
    fs::write(
        out.join("cli_catalog.json"),
        serde_json::to_vec_pretty(&catalog)?,
    )?;
    Ok(())
}
