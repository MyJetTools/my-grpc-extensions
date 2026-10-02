pub fn inject_telemetry_line(fn_name: &str) -> proc_macro2::TokenStream {
    quote::quote! {
        let my_telemetry_ctx = my_grpc_extensions::get_telemetry(
            &request.metadata(),
            request.remote_addr(),
            #fn_name
        );

        let my_telemetry = my_telemetry_ctx.get_ctx();
    }
}
