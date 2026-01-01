# lib-epub

A Rust library for reading and manipulating EPUB eBook files.

This library provides complete EPUB file parsing functionality, supporting EPUB 2 and EPUB 3 formats. It can extract metadata, access content files, and handle encrypted resources. Furthermore, this library also provides a convenient way to build epub files from a set of resources.

## Features

- Parse EPUB file structure and containers, extract metadata, access resource files.
- Automatic handle encrypted content.
- Optional EPUB build functionality via 'builder' feature.
- EPUB specification-compliant verification mechanism.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
lib-epub = "0.0.5"
```

## Quick Start

Reading an EPUB file and extracting metadata:

```rust
use lib_epub::{error::EpubError, epub::EpubDoc};

fn main() -> Result<(), EpubError> {
    // Open EPUB file
    let mut doc = EpubDoc::new("path/to/epub/file.epub")?;

    // Get metadata
    println!("Title: {:?}", doc.get_title()?);
    println!("Creator: {:?}", doc.get_metadata_value("creator")?);

    // Read content
    let (_content, _mime) = doc.spine_current()?;
    let (_content, _mime) = doc.next_spine()?;

    Ok(())
}
```

Building an EPUB file:

```rust
use lib_epub::{
    builder::{EpubBuilder, EpubVersion3},
    error::EpubError,
    types::{MetadataItem, ManifestItem, NavPoint, SpineItem},
};

fn main() -> Result<(), EpubError> {
    let mut builder = EpubBuilder::<EpubVersion3>::new()?;

    builder
        .add_rootfile("EPUB/content.opf")?
        .add_metadata(MetadataItem::new("title", "Test Book"))
        .add_metadata(MetadataItem::new("language", "en"))
        .add_metadata(
            MetadataItem::new("identifier", "unique-id")
                .with_id("pub-id")
                .build(),
        )
        .add_manifest(
            "./test_case/Overview.xhtml",
            ManifestItem::new("content", "target/path")?,
        )?
        .add_spine(SpineItem::new("content"))
        .add_catalog_item(NavPoint::new("label"));

    builder.build("output.epub")?;

    Ok(())
}
```

## Enable features

Enable the builder feature in `Cargo.toml`:

```toml
[dependencies]
lib-epub = { version = "0.0.5", features = ["builder"] }
```

## MSRV

The minimum supported Rust version is 1.85.0.

## More information

- Documentation: https://docs.rs/lib-epub
- Crate: https://crates.io/crates/lib-epub

## License

This project is licensed under the MIT License.
