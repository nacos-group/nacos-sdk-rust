use darling::FromMeta;
use syn::{parse_macro_input, parse_quote, AttributeArgs, ItemStruct, Path};

use self::{request::grpc_request, response::grpc_response};

pub(crate) mod request;
pub(crate) mod response;

#[derive(Debug, FromMeta)]
pub(crate) struct MacroArgs {
    identity: String,

    #[darling(default)]
    crates: Crates,

    module: Module,
}

#[derive(Debug, FromMeta)]
enum Module {
    Config,
    Naming,
    Internal,
}

impl ToString for Module {
    fn to_string(&self) -> String {
        match self {
            Module::Config => "config".to_string(),
            Module::Naming => "naming".to_string(),
            Module::Internal => "internal".to_string(),
        }
    }
}

#[derive(Debug, FromMeta)]
struct Crates {
    #[darling(default = "Self::default_serde")]
    serde: Path,

    #[darling(default = "Self::default_std")]
    std: Path,
}

impl Default for Crates {
    fn default() -> Self {
        Self::from_list(&[]).unwrap()
    }
}

impl Crates {
    fn default_serde() -> Path {
        parse_quote! { ::serde }
    }

    fn default_std() -> Path {
        parse_quote! { ::std }
    }
}

pub fn request(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let item_struct = parse_macro_input!(input as ItemStruct);

    let attr_args = parse_macro_input!(args as AttributeArgs);
    let macro_args = MacroArgs::from_list(&attr_args).unwrap();

    grpc_request(macro_args, item_struct).into()
}

pub fn response(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let item_struct = parse_macro_input!(input as ItemStruct);

    let attr_args = parse_macro_input!(args as AttributeArgs);
    let macro_args = MacroArgs::from_list(&attr_args).unwrap();

    grpc_response(macro_args, item_struct).into()
}
