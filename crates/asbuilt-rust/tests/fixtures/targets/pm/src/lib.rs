use proc_macro::TokenStream;

#[proc_macro]
pub fn m(input: TokenStream) -> TokenStream {
    input
}
