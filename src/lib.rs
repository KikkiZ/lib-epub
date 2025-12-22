#![allow(clippy::collapsible_if)]

//! Epub library
//!
//! A Rust library for reading and manipulating EPUB eBook files.
//!
//! This library provides complete EPUB file parsing functionality,
//! supporting EPUB 2 and EPUB 3 formats. It can extract metadata,
//! access content files, and handle encrypted resources.
//!
//! ## Features
//!
//! - Parse EPUB file structure and containers
//! - Extract book metadata (title, author, language, etc.)
//! - Access content files and resource files
//! - Handle encrypted content (font obfuscation, etc.; currently incomplete,
//!   will be improved in future versions)
//! - Optional EPUB build functionality (via the `builder` attribute)
//! - EPUB specification-compliant verification mechanism
//!
//! ## Quick Start
//!
//! ### Read EPUB Files
//!
//! ```rust, ignore
//! use lib_epub::epub::EpubDoc;
//!
//! let doc = EpubDoc::new("path/to/book.epub")?;
//! let title = doc.get_title()?;
//! println!("Title: {}", title);
//! ```
//!
//! ### Enable Builder Feature
//!
//! Enable the builder feature in `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! lib-epub = { version = "0.0.2", features = ["builder"] }
//! ```
//!
//! ## Module Description
//!
//! - [epub] - Core functionality for EPUB document parsing
//! - [error] - Error type definition
//! - [types] - Data structure definition
//! - [builder] - EPUB build functionality (requires enabling the `builder` feature)
//!
//! ### Exported Trait
//!
//! - [DecodeBytes] - Byte data decoding trait, used to convert raw bytes into strings

pub(crate) mod utils;

#[cfg(feature = "builder")]
pub mod builder;
pub mod epub;
pub mod error;
pub mod types;

pub use utils::DecodeBytes;
