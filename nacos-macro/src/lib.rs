mod request;
mod response;

use darling::FromMeta;
use request::grpc_request;
use response::grpc_response;
use syn::{parse_macro_input, parse_quote, AttributeArgs, ItemStruct, Path};

#[derive(Debug, FromMeta)]
pub(crate) struct MacroArgs {
    identity: String,

    #[darling(default)]
    crates: Crates,
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

#[proc_macro_attribute]
pub fn request(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let item_struct = parse_macro_input!(input as ItemStruct);

    let attr_args = parse_macro_input!(args as AttributeArgs);
    let macro_args = MacroArgs::from_list(&attr_args).unwrap();

    grpc_request(macro_args, item_struct).into()
}

#[proc_macro_attribute]
pub fn response(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let item_struct = parse_macro_input!(input as ItemStruct);

    let attr_args = parse_macro_input!(args as AttributeArgs);
    let macro_args = MacroArgs::from_list(&attr_args).unwrap();

    grpc_response(macro_args, item_struct).into()
}
