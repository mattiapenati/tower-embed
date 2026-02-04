use std::borrow::Cow;

use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};
use quote::ToTokens;
use tower_embed_core::headers;

/// Derive the `Embed` trait for unit struct, embedding assets from a folder.
///
/// ## Usage
///
/// Apply `#[derive(Embed)]` to a unit struct and specify the folder to embed using the
/// `#[embed(folder = "...")]` attribute.
///
/// Optionally, specify the crate path with `#[embed(crate = path)]`. This is applicable when
/// invoking re-exported derive from a public macro in a different crate.
///
/// The name of file to serve as index for directories can be customized using #[embed(index =
/// "...")], the default is "index.html".
///
/// If the `astro` feature is enabled, you can enable Astro support using the attributes `astro`.
/// In such case, if `folder` is not specified, the project root used is the manifest folder. For
/// astro projects, the `index` attribute cannot be used to customize the index for directories.
#[proc_macro_derive(Embed, attributes(embed))]
pub fn derive_embed(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);

    expand_derive_embed(input)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

fn expand_derive_embed(input: syn::DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let input = DeriveEmbedFolder::from_ast(&input)?;

    let static_embed = expand_static_embed(&input)?;
    let dynamic_embed = expand_dynamic_embed(&input);

    let expanded = quote::quote! {
        #[cfg(not(debug_assertions))]
        #static_embed

        #[cfg(debug_assertions)]
        #dynamic_embed
    };

    Ok(expanded)
}

fn expand_static_embed(input: &DeriveEmbedFolder) -> syn::Result<proc_macro2::TokenStream> {
    let DeriveEmbedFolder { ident, attrs } = input;
    let DeriveEmbedFolderAttrs {
        folder,
        crate_path,
        index,
        ..
    } = attrs;

    let root = root_absolute_path(folder);

    #[cfg(feature = "astro")]
    let root = if attrs.astro {
        tower_embed_core::astro::build_project(root.as_std_path())
            .map_err(|err| {
                syn::Error::new_spanned(ident, format!("Failed to build Astro project: {err}"))
            })?
            .try_into()
            .unwrap()
    } else {
        root
    };

    let embedded_files = get_files(&root, index).map(|file| {
        let last_modified = tower_embed_core::last_modified(file.absolute_path.as_std_path())
            .ok()
            .and_then(|headers::LastModified(time)| {
                time.duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_secs())
                    .ok()
            });
        let last_modified = match last_modified {
            Some(secs) => quote::quote! { headers::LastModified::from_unix_timestamp(#secs) },
            None => quote::quote! { None },
        };

        let relative_path = file.relative_path.as_str();
        let absolute_path = file.absolute_path.as_str();
        let redirect_path = format!("{relative_path}/{index}");
        let redirect_path = redirect_path.trim_start_matches('/');

        match file.kind {
            FileKind::File => quote::quote! {{
                let content = include_bytes!(#absolute_path).as_slice();
                let metadata = Metadata {
                    content_type: #crate_path::core::content_type(Path::new(#relative_path)),
                    etag: Some(#crate_path::core::etag(content)),
                    last_modified: #last_modified,
                };
                [(#relative_path, Entry::File(content, metadata))]
            }},
            FileKind::Dir => quote::quote! {{
                [
                    (#relative_path, Entry::Redirect(#redirect_path)),
                    (concat!(#relative_path, "/"), Entry::Redirect(#redirect_path)),
                ]
            }},
        }
    });

    Ok(quote::quote! {
        impl #crate_path::core::Embed for #ident {
            fn forward(
                req: #crate_path::core::http::Request<()>,
            ) -> impl Future<Output = #crate_path::core::http::Response<#crate_path::core::Body>> + Send + 'static
            {
                use std::{collections::HashMap, sync::LazyLock, path::Path};
                use #crate_path::core::{Content, Embedded, EmbeddedExt, Metadata, headers};

                enum Entry {
                    File(&'static [u8], Metadata),
                    Redirect(&'static str),
                }

                static FILES: LazyLock<HashMap<&'static str, Entry>> = LazyLock::new(|| {
                    let mut m = HashMap::new();
                    #(m.extend(#embedded_files);)*
                    m
                });

                let mut path = req.uri().path().trim_start_matches('/');
                let output = loop {
                    match FILES.get(path) {
                        Some(Entry::File(bytes, metadata)) => break Ok(Embedded {
                            content: Content::from_static(bytes),
                            metadata: metadata.clone(),
                        }),
                        Some(Entry::Redirect(redirect)) => {
                            path = redirect;
                        }
                        None => break Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
                    };
                };
                std::future::ready(output.into_response(req))
            }

        }
    })
}

fn expand_dynamic_embed(input: &DeriveEmbedFolder) -> proc_macro2::TokenStream {
    let DeriveEmbedFolder { ident, attrs } = input;
    let DeriveEmbedFolderAttrs {
        folder,
        crate_path,
        index,
        astro,
    } = attrs;

    let root = root_absolute_path(folder);
    let root = root.as_str();

    if *astro {
        quote::quote! {
            impl #crate_path::core::Embed for #ident {
                fn forward(
                    req: #crate_path::core::http::Request<()>,
                ) -> impl Future<Output = #crate_path::core::http::Response<#crate_path::core::Body>> + Send + 'static
                {
                    use std::{path::Path, sync::LazyLock};
                    use #crate_path::core::astro::AstroProxy;

                    static ASTRO: LazyLock<AstroProxy> = LazyLock::new(|| {
                        AstroProxy::new(&Path::new(#root)).expect("Failed to start Astro dev server")
                    });

                    ASTRO.send_request(req)
                }
            }
        }
    } else {
        quote::quote! {
            impl #crate_path::core::Embed for #ident {
                fn forward(
                    req: #crate_path::core::http::Request<()>,
                ) -> impl Future<Output = #crate_path::core::http::Response<#crate_path::core::Body>> + Send + 'static
                {
                    let path = req.uri().path().trim_start_matches('/').to_string();
                    async move {
                        use #crate_path::core::EmbeddedExt;
                        #crate_path::core::Embedded::load_file(path, #root, #index).await.into_response(req)
                    }
                }
            }
        }
    }
}

/// A source data annotated with `#[derive(Embed)]``
struct DeriveEmbedFolder {
    /// The struct name
    ident: syn::Ident,
    /// Attributes of structure
    attrs: DeriveEmbedFolderAttrs,
}

/// Attributes for `Embed` derive macro.
struct DeriveEmbedFolderAttrs {
    /// The folder to embed
    folder: String,
    /// The path to the crate `tower_embed`
    crate_path: syn::Path,
    /// The index file name
    index: Cow<'static, str>,
    /// Enable support to Astro
    astro: bool,
}

impl DeriveEmbedFolder {
    fn from_ast(input: &syn::DeriveInput) -> syn::Result<Self> {
        let syn::Data::Struct(data) = &input.data else {
            return Err(syn::Error::new_spanned(
                input,
                "`Embed` can only be derived for unit structs",
            ));
        };

        if !matches!(&data.fields, syn::Fields::Unit) {
            return Err(syn::Error::new_spanned(
                &data.fields,
                "`Embed` can only be derived for unit structs",
            ));
        }

        let ident = input.ident.clone();
        let attrs = DeriveEmbedFolderAttrs::from_ast(input)?;

        Ok(Self { ident, attrs })
    }
}

impl DeriveEmbedFolderAttrs {
    fn from_ast(input: &syn::DeriveInput) -> syn::Result<Self> {
        let mut folder = None;
        let mut crate_path = None;
        let mut index = None;
        let mut astro = false;

        for attr in &input.attrs {
            if !attr.path().is_ident("embed") {
                continue;
            }

            let list = attr.meta.require_list()?;
            if list.tokens.is_empty() {
                continue;
            }

            list.parse_nested_meta(|meta| {
                if meta.path.is_ident("folder") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    folder = Some(value.value());
                } else if meta.path.is_ident("crate") {
                    let value: syn::Path = meta.value()?.parse()?;
                    crate_path = Some(value);
                } else if meta.path.is_ident("index") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    index = Some(Cow::Owned(value.value()));
                } else if meta.path.is_ident("astro") {
                    if cfg!(not(feature = "astro")) {
                        return Err(syn::Error::new_spanned(
                            meta.path,
                            "`astro` feature is not enabled",
                        ));
                    } else {
                        astro = true;
                    }
                } else {
                    let name = meta.path.to_token_stream();
                    return Err(syn::Error::new_spanned(
                        meta.path,
                        format_args!("unknown `{}` attribute for `embed`", name),
                    ));
                }
                Ok(())
            })?;
        }

        // If astro is enabled and folder is not specified, use CARGO_MANIFEST_DIR as project root
        if astro && folder.is_none() {
            folder = Some(manifest_dir().to_string());
        }

        if astro && index.is_some() {
            return Err(syn::Error::new_spanned(
                input,
                "`index` attribute cannot be used with `astro` attribute",
            ));
        }

        let Some(folder) = folder else {
            return Err(syn::Error::new_spanned(
                input,
                "#[derive(Embed)] requires `folder` attribute",
            ));
        };

        let crate_path = crate_path.unwrap_or_else(|| syn::parse_quote! { tower_embed });
        let index = index.unwrap_or(Cow::Borrowed("index.html"));

        Ok(Self {
            folder,
            crate_path,
            index,
            astro,
        })
    }
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR")
            .expect("missing CARGO_MANIFEST_DIR environment variable"),
    )
}

fn root_absolute_path(folder: &str) -> PathBuf {
    Path::new(&manifest_dir()).join(folder)
}

fn get_files(root: &Path, index: &str) -> impl Iterator<Item = File> {
    walkdir::WalkDir::new(root)
        .follow_links(true)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(move |entry| {
            let kind = if entry.file_type().is_file() {
                FileKind::File
            } else if entry.file_type().is_dir() {
                if !entry.path().join(index).is_file() {
                    return None;
                }

                FileKind::Dir
            } else {
                return None;
            };

            let absolute_path: &Path = entry.path().try_into().unwrap();
            let absolute_path = absolute_path.to_path_buf();

            let relative_path = absolute_path
                .canonicalize_utf8()
                .unwrap()
                .strip_prefix(root)
                .unwrap()
                .to_path_buf();

            Some(File {
                kind,
                relative_path,
                absolute_path,
            })
        })
}

struct File {
    kind: FileKind,
    relative_path: PathBuf,
    absolute_path: PathBuf,
}

enum FileKind {
    File,
    Dir,
}
