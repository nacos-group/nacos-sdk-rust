pub(crate) mod message;

#[proc_macro_attribute]
pub fn request(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    message::request(args, input)
}

#[proc_macro_attribute]
pub fn response(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    message::response(args, input)
}
