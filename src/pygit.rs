//! ## Feature Flags
#![cfg_attr(
    feature = "document-features",
    cfg_attr(doc, doc = ::document_features::document_features!())
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![deny(rust_2018_idioms)]
#![forbid(unsafe_code)]

use pyo3::prelude::*;

mod porcelain;
mod shared;

use anyhow::Result;

#[pymodule]
fn pygit(_py: Python<'_>, _m: &PyModule) -> PyResult<()> {
    porcelain::main();
    Ok(())
}

#[cfg(not(feature = "pretty-cli"))]
compile_error!("Please set 'pretty-cli' feature flag");
