use proc_macro::*;

#[proc_macro_attribute]
pub fn main(args: TokenStream, input: TokenStream) -> TokenStream {
    let name: syn::LitStr = syn::parse(args).unwrap();
    let mut input: syn::ItemFn = syn::parse(input).unwrap();
    input.sig.ident = quote::format_ident!("__containerd_shim_wasm_macro_runtime_main");

    let engine_ident = quote::format_ident!("{}Engine", name.value());
    let instance_ident = quote::format_ident!("{}Instance", name.value());
    let lower_name = syn::LitStr::new(name.value().to_lowercase().as_str(), name.span());

    let (async_runtime, get_async_runtime) = async_quotes();
    let async_wasi_termination = if cfg!(feature = "tokio") {
        quote::quote! {
            trait WasiTerminationFuture {
                fn __containerd_wasm_shim_macro_exit_shim_process(self);
            }

            impl<F: ::std::future::Future> WasiTerminationFuture for F
            where
                F::Output: WasiTermination,
            {
                fn __containerd_wasm_shim_macro_exit_shim_process(self) {
                    #get_async_runtime.block_on(self).__containerd_wasm_shim_macro_exit_shim_process()
                }
            }
        }
    } else {
        quote::quote! {}
    };

    let expanded = quote::quote! {
        #async_runtime

        trait __ContainerdShimWasmMacroRuntimeTrait {
            fn can_handle(ctx: &impl ::containerd_shim_wasm::container::RuntimeContext) -> ::anyhow::Result<()> {
                ctx.resolved_wasi_entrypoint()?;
                Ok(())
            }
        }

        struct __ContainerdShimWasmMacroRuntimeStruct;

        impl __ContainerdShimWasmMacroRuntimeTrait for __ContainerdShimWasmMacroRuntimeStruct {}

        #[derive(Clone, Default)]
        struct #engine_ident;

        type #instance_ident = ::containerd_shim_wasm::container::Instance<#engine_ident>;

        impl ::containerd_shim_wasm::container::Engine for #engine_ident {
            fn name() -> &'static str {
                #lower_name
            }

            fn can_handle(&self, ctx: &impl ::containerd_shim_wasm::container::RuntimeContext) -> ::anyhow::Result<()> {
                __ContainerdShimWasmMacroRuntimeStruct::can_handle(ctx)
            }

            fn run_wasi(&self, ctx: &impl ::containerd_shim_wasm::container::RuntimeContext, stdio: ::containerd_shim_wasm::container::Stdio) -> ::anyhow::Result<i32> {
                #input

                trait WasiTermination {
                    fn __containerd_wasm_shim_macro_exit_shim_process(self);
                }

                impl WasiTermination for () {
                    fn __containerd_wasm_shim_macro_exit_shim_process(self) {
                        ::std::process::exit(0);
                    }
                }

                impl WasiTermination for i32 {
                    fn __containerd_wasm_shim_macro_exit_shim_process(self) {
                        ::std::process::exit(self);
                    }
                }

                impl WasiTermination for ::std::convert::Infallible {
                    fn __containerd_wasm_shim_macro_exit_shim_process(self) {
                        match self {}
                    }
                }

                impl<T: WasiTermination, E: Into<::anyhow::Error>> WasiTermination for ::std::result::Result<T, E> {
                    fn __containerd_wasm_shim_macro_exit_shim_process(self) {
                        match self {
                            Ok(term) => term.__containerd_wasm_shim_macro_exit_shim_process(),
                            Err(err) => {
                                ::log::info!("error: {}", err.into());
                                ::std::process::exit(137);
                            },
                        }
                    }
                }

                #async_wasi_termination

                __containerd_shim_wasm_macro_runtime_main(ctx, stdio).__containerd_wasm_shim_macro_exit_shim_process();
                unreachable!();
            }
        }

        fn main() {
            let os_args: ::std::vec::Vec<_> = ::std::env::args_os().collect();
            let flags = ::containerd_shim_wasm::shim::parse(&os_args[1..]).unwrap();
            let argv0 = ::std::path::PathBuf::from(&os_args[0]);
            let argv0 = argv0.file_stem().unwrap_or_default().to_string_lossy();

            if flags.version {
                ::std::println!("{argv0}:");
                ::std::println!("  Runtime: {}", #name);
                ::std::println!("  Version: {}", env!("CARGO_PKG_VERSION"));
                ::std::println!("  Revision: {}", env!("CARGO_GIT_HASH"));
                ::std::println!();

                ::std::process::exit(0);
            }

            let shim_cli    = ::std::format!("containerd-shim-{}-v1", #lower_name);
            let shim_client = ::std::format!("containerd-shim-{}d-v1", #lower_name);
            let shim_daemon = ::std::format!("containerd-{}d", #lower_name);

            if argv0 == shim_cli {
                let id = ::std::format!("io.containerd.{}.v1", #lower_name);
                ::containerd_shim_wasm::shim::run::<::containerd_shim_wasm::sandbox::ShimCli<#instance_ident>>(&id, None);
            } else if argv0 == shim_client {
                ::containerd_shim_wasm::shim::run::<::containerd_shim_wasm::sandbox::manager::Shim>(&shim_client, None)
            } else if argv0 == shim_daemon {
                ::log::info!("starting up!");
                let s: ::containerd_shim_wasm::sandbox::ManagerService<::containerd_shim_wasm::sandbox::Local<#instance_ident>> = ::std::default::Default::default();
                let s = ::std::sync::Arc::new(Box::new(s) as Box<dyn ::containerd_shim_wasm::services::sandbox_ttrpc::Manager + Send + Sync>);
                let service = ::containerd_shim_wasm::services::sandbox_ttrpc::create_manager(s);

                let mut server = ::containerd_shim_wasm::ttrpc::Server::new()
                    .bind("unix:///run/io.containerd.wasmwasi.v1/manager.sock")
                    .expect("failed to bind to socket")
                    .register_service(service);

                server.start().expect("failed to start daemon");
                ::log::info!("server started!");
                let (_tx, rx) = ::std::sync::mpsc::channel::<()>();
                rx.recv().unwrap();
            } else {
                eprintln!("error: unrecognized binary name, expected one of {shim_cli}, {shim_client}, or {shim_daemon}.");
                ::std::process::exit(137);
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_attribute]
pub fn validate(_args: TokenStream, input: TokenStream) -> TokenStream {
    let mut input: syn::ItemFn = syn::parse(input).unwrap();
    input.sig.ident = quote::format_ident!("__containerd_shim_wasm_macro_runtime_validate");

    let (_, get_async_runtime) = async_quotes();
    let async_wasi_can_handle = if cfg!(feature = "tokio") {
        quote::quote! {
            trait WasiCanHandleFuture {
                fn __containerd_wasm_shim_macro_report_can_handle(self) -> ::anyhow::Result<()>;
            }

            impl<F: ::std::future::Future> WasiCanHandleFuture for F
            where
                F::Output: WasiCanHandle,
            {
                fn __containerd_wasm_shim_macro_report_can_handle(self) -> ::anyhow::Result<()> {
                    #get_async_runtime.block_on(self).__containerd_wasm_shim_macro_report_can_handle()
                }
            }
        }
    } else {
        quote::quote! {}
    };

    let expanded = quote::quote! {
        impl __ContainerdShimWasmMacroRuntimeStruct {
            fn can_handle(ctx: &impl ::containerd_shim_wasm::container::RuntimeContext) -> ::anyhow::Result<()> {
                #input

                trait WasiCanHandle {
                    fn __containerd_wasm_shim_macro_report_can_handle(self) -> ::anyhow::Result<()>;
                }

                impl WasiCanHandle for bool {
                    fn __containerd_wasm_shim_macro_report_can_handle(self) -> ::anyhow::Result<()> {
                        use ::anyhow::Context;
                        self.then_some(()).context("can't handle workflow")
                    }
                }

                impl WasiCanHandle for () {
                    fn __containerd_wasm_shim_macro_report_can_handle(self) -> ::anyhow::Result<()> {
                        Ok(())
                    }
                }

                impl<T: WasiCanHandle, E: Into<::anyhow::Error>> WasiCanHandle for ::std::result::Result<T, E> {
                    fn __containerd_wasm_shim_macro_report_can_handle(self) -> ::anyhow::Result<()> {
                        match self {
                            Ok(can_handle) => can_handle.__containerd_wasm_shim_macro_report_can_handle(),
                            Err(err) => Err(err.into()),
                        }
                    }
                }

                #async_wasi_can_handle

                __containerd_shim_wasm_macro_runtime_validate(ctx).__containerd_wasm_shim_macro_report_can_handle()
            }
        }
    };

    TokenStream::from(expanded)
}

fn async_quotes() -> (proc_macro2::TokenStream, proc_macro2::TokenStream) {
    let async_runtime = if cfg!(feature = "tokio") {
        quote::quote! {
            static __CONTAINERD_SHIM_WASM_MACRO_ASYNC_RUNTIME: ::std::sync::OnceLock<::containerd_shim_wasm::tokio::runtime::Runtime> = ::std::sync::OnceLock::new();
        }
    } else {
        quote::quote! {}
    };

    let get_async_runtime = if cfg!(feature = "tokio") {
        quote::quote! {
            __CONTAINERD_SHIM_WASM_MACRO_ASYNC_RUNTIME.get_or_init(|| {
                ::containerd_shim_wasm::tokio::runtime::Runtime::new().expect("failed to create runtime")
            })
        }
    } else {
        quote::quote! {}
    };

    (async_runtime, get_async_runtime)
}
