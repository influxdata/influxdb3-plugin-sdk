//! Built-in template catalog.
//!
//! Each template lives in its own submodule and exposes a `METADATA`
//! constant, a `#[derive(clap::Args)]` `Args` struct, and a `run()`
//! entrypoint. Adding a template is a two-step: add a submodule here
//! plus a variant in [`super::NewCommand`] pointing at its `Args`.

pub(crate) mod index;
pub(crate) mod process_request;
pub(crate) mod process_scheduled_call;
pub(crate) mod process_writes;

#[derive(Debug)]
pub(crate) struct TemplateMetadata {
    pub name: &'static str,
    pub short_name: &'static str,
    // Redundant with each template's clap `about`; kept so the metadata
    // shape stays uniform for future consumers.
    #[allow(dead_code)]
    pub description: &'static str,
}

/// All templates, in display order. Used by `new list`.
pub(crate) const ALL: &[&TemplateMetadata] = &[
    &process_writes::METADATA,
    &process_scheduled_call::METADATA,
    &process_request::METADATA,
    &index::METADATA,
];
