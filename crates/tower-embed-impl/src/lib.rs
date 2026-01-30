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
#[proc_macro_derive(Embed, attributes(embed))]
pub fn derive_embed(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);

    expand_derive_embed(input)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

fn expand_derive_embed(input: syn::DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let DeriveEmbed { ident, attrs } = DeriveEmbed::from_ast(&input)?;
    let DeriveEmbedAttrs {
        folder,
        crate_path,
        index,
    } = attrs;

    let root = root_absolute_path(&folder);
    let embedded_files = get_files(&root, &index).map(|file| {
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

        match file.kind {
            FileKind::File => quote::quote! {{
                let content = include_bytes!(#absolute_path).as_slice();
                let metadata = Metadata {
                    content_type: #crate_path::core::content_type(Path::new(#relative_path)),
                    etag: Some(#crate_path::core::etag(content)),
                    last_modified: #last_modified,
                };
                (concat!("/", #relative_path), Entry::File(content, metadata))
            }},
            FileKind::Dir => quote::quote! {{
                let redirect_path = concat!(#relative_path, "/", #index);
                (concat!("/", #relative_path), Entry::Redirect(redirect_path))
            }},
        }
    });

    let root = root.as_str();

    let expanded = quote::quote! {
        impl #crate_path::Embed for #ident {
            #[cfg(not(debug_assertions))]
            fn get(path: &str) -> impl Future<Output = std::io::Result<#crate_path::core::Embedded>> + Send + 'static {
                use std::{collections::HashMap, sync::LazyLock, path::Path};

                use #crate_path::core::{Content, Embedded, Metadata, headers};

                enum Entry {
                    File(&'static [u8], Metadata),
                    Redirect(&'static str),
                }

                const FILES: LazyLock<HashMap<&'static str, Entry>> = LazyLock::new(|| {
                    let mut m = HashMap::new();
                    #({
                        let (key, value) = #embedded_files;
                        m.insert(key, value);
                    })*
                    m
                });

                let mut path = path.strip_suffix('/').unwrap_or(path);
                if path.is_empty() {
                    path = "/";
                }

                let output = loop {
                    match FILES.get(path) {
                        Some(Entry::File(bytes, metadata)) => break Ok(Embedded {
                            content: Content::from_static(bytes),
                            metadata: metadata.clone(),
                        }),
                        Some(Entry::Redirect(redirect_path)) => {
                            path = redirect_path;
                        }
                        None => break Err(std::io::ErrorKind::NotFound.into()),
                    };
                };
                std::future::ready(output)
            }

            #[cfg(debug_assertions)]
            fn get(path: &str) -> impl Future<Output = std::io::Result<#crate_path::core::Embedded>> + Send + 'static {
                use std::path::Path;

                use #crate_path::core::{Content, Embedded, Metadata};

                const ROOT: &str = #root;

                let mut filename = Path::new(ROOT).join(path.trim_start_matches('/'));
                if filename.is_dir() {
                    filename = filename.join(#index);
                }

                let metadata = Metadata {
                    content_type: #crate_path::core::content_type(&filename),
                    etag: None,
                    last_modified: None,
                };

                async move {
                    #crate_path::file::File::open(&filename).await.map(|file| {
                        Embedded {
                            content: Content::from_stream(file),
                            metadata,
                        }
                    })
                }
            }
        }
    };

    Ok(expanded)
}

/// A source data annotated with `#[derive(Embed)]``
struct DeriveEmbed {
    /// The struct name
    ident: syn::Ident,
    /// Attributes of structure
    attrs: DeriveEmbedAttrs,
}

/// Attributes for `Embed` derive macro.
struct DeriveEmbedAttrs {
    /// The folder to embed
    folder: String,
    /// The path to the crate `tower_embed`
    crate_path: syn::Path,
    /// The index file name
    index: Cow<'static, str>,
}

impl DeriveEmbed {
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
        let attrs = DeriveEmbedAttrs::from_ast(input)?;

        Ok(Self { ident, attrs })
    }
}

impl DeriveEmbedAttrs {
    fn from_ast(input: &syn::DeriveInput) -> syn::Result<Self> {
        let mut folder = None;
        let mut crate_path = None;
        let mut index = None;

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
                } else {
                    let name = meta.path.to_token_stream();
                    return Err(syn::Error::new_spanned(
                        meta.path,
                        format_args!("unknown `embed` attribute for `{}`", name),
                    ));
                }
                Ok(())
            })?;
        }

        let Some(folder) = folder else {
            return Err(syn::Error::new_spanned(
                input,
                "#[derive(Embed)] requires `folder` attribute",
            ));
        };

        let crate_path = crate_path.unwrap_or_else(|| syn::parse_quote! { tower_embed });
        let index = index.unwrap_or_else(|| Cow::Borrowed("index.html"));

        Ok(Self {
            folder,
            crate_path,
            index,
        })
    }
}

fn root_absolute_path(folder: &str) -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("missing CARGO_MANIFEST_DIR environment variable");

    Path::new(&manifest_dir).join(folder)
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
