//! EPUB build functionality
//!
//! This module provides functionality for creating and building EPUB eBook files.
//! The `EpubBuilder` structure implements the build logic of the EPUB 3.0 specification,
//! allowing users to create standard-compliant EPUB files from scratch.
//!
//! ## Usage
//!
//! ```rust, no_run
//! # #[cfg(feature = "builder")] {
//! # fn main() -> Result<(), lib_epub::error::EpubError> {
//! use lib_epub::{
//!     builder::{EpubBuilder, EpubVersion3},
//!     types::{MetadataItem, ManifestItem, SpineItem},
//! };
//!
//! let mut builder = EpubBuilder::<EpubVersion3>::new()?;
//! builder
//!     .add_rootfile("OEBPS/content.opf")?
//!     .add_metadata(MetadataItem::new("title", "Test Book"))
//!     .add_manifest(
//!         "path/to/content",
//!         ManifestItem::new("content_id", "target/path")?,
//!     )?
//!     .add_spine(SpineItem::new("content.xhtml"));
//!
//! builder.build("output.epub")?;
//! # Ok(())
//! # }
//! # }
//! ```
//!
//! ## Notes
//!
//! - Requires `builder` functionality to use this module.

#[cfg(feature = "no-indexmap")]
use std::collections::HashMap;
use std::{
    cmp::Reverse,
    env,
    fs::{self, File},
    io::{BufReader, Cursor, Read, Seek, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
};

use chrono::{SecondsFormat, Utc};
#[cfg(not(feature = "no-indexmap"))]
use indexmap::IndexMap;
use infer::Infer;
use log::warn;
use quick_xml::{
    Writer,
    events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event},
};
use walkdir::WalkDir;
use zip::{CompressionMethod, ZipWriter, write::FileOptions};

#[cfg(feature = "content-builder")]
use crate::builder::content::ContentBuilder;
use crate::{
    epub::EpubDoc,
    error::{EpubBuilderError, EpubError},
    types::{ManifestItem, MetadataItem, NavPoint, SpineItem},
    utils::{
        ELEMENT_IN_DC_NAMESPACE, check_realtive_link_leakage, local_time, remove_leading_slash,
    },
};

#[cfg(feature = "content-builder")]
pub mod content;

type XmlWriter = Writer<Cursor<Vec<u8>>>;

// struct EpubVersion2;
#[cfg_attr(test, derive(Debug))]
pub struct EpubVersion3;

/// Rootfile builder for EPUB container
///
/// The `RootfileBuilder` is responsible for managing the rootfile paths in the EPUB container.
/// Each rootfile points to an OPF (Open Packaging Format) file that defines the structure
/// and content of an EPUB publication.
///
/// In EPUB 3.0, a single rootfile is typically used, but the structure supports multiple
/// rootfiles for more complex publications.
///
/// ## Notes
///
/// - Rootfile paths must be relative and cannot start with "../" or "/"
/// - At least one rootfile must be added before building the EPUB
#[derive(Debug)]
pub struct RootfileBuilder {
    /// List of rootfile paths
    pub(crate) rootfiles: Vec<String>,
}

impl RootfileBuilder {
    /// Creates a new empty `RootfileBuilder` instance
    pub(crate) fn new() -> Self {
        Self { rootfiles: Vec::new() }
    }

    /// Add a rootfile path
    ///
    /// Adds a new rootfile path to the builder. The rootfile points to the OPF file
    /// that will be created when building the EPUB.
    ///
    /// ## Parameters
    /// - `rootfile`: The relative path to the OPF file
    ///
    /// ## Return
    /// - `Ok(&mut Self)`: Successfully added the rootfile
    /// - `Err(EpubError)`: Error if the path is invalid (starts with "/" or "../")
    pub fn add(&mut self, rootfile: impl AsRef<str>) -> Result<&mut Self, EpubError> {
        let rootfile = rootfile.as_ref();

        let rootfile = if rootfile.starts_with("/") || rootfile.starts_with("../") {
            return Err(EpubBuilderError::IllegalRootfilePath.into());
        } else if let Some(rootfile) = rootfile.strip_prefix("./") {
            rootfile
        } else {
            rootfile
        };

        self.rootfiles.push(rootfile.into());
        Ok(self)
    }

    /// Clear all rootfiles
    ///
    /// Removes all rootfile paths from the builder.
    pub fn clear(&mut self) -> &mut Self {
        self.rootfiles.clear();
        self
    }

    /// Check if the builder is empty
    pub(crate) fn is_empty(&self) -> bool {
        self.rootfiles.is_empty()
    }

    /// Get the first rootfile
    pub(crate) fn first(&self) -> Option<&String> {
        self.rootfiles.first()
    }

    /// Generate the container.xml content
    ///
    /// Writes the XML representation of the container and rootfiles to the provided writer.
    pub(crate) fn make(&self, writer: &mut XmlWriter) -> Result<(), EpubError> {
        writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;

        writer.write_event(Event::Start(BytesStart::new("container").with_attributes(
            [
                ("version", "1.0"),
                ("xmlns", "urn:oasis:names:tc:opendocument:xmlns:container"),
            ],
        )))?;
        writer.write_event(Event::Start(BytesStart::new("rootfiles")))?;

        for rootfile in &self.rootfiles {
            writer.write_event(Event::Empty(BytesStart::new("rootfile").with_attributes([
                ("full-path", rootfile.as_str()),
                ("media-type", "application/oebps-package+xml"),
            ])))?;
        }

        writer.write_event(Event::End(BytesEnd::new("rootfiles")))?;
        writer.write_event(Event::End(BytesEnd::new("container")))?;

        Ok(())
    }
}

/// Metadata builder for EPUB publications
///
/// The `MetadataBuilder` is responsible for managing metadata items in an EPUB publication.
/// Metadata includes essential information such as title, author, language, identifier,
/// publisher, and other descriptive information about the publication.
///
/// ## Required Metadata
///
/// According to the EPUB specification, the following metadata are required:
/// - `title`: The publication title
/// - `language`: The language of the publication (e.g., "en", "zh-CN")
/// - `identifier`: A unique identifier for the publication with id "pub-id"
#[derive(Debug)]
pub struct MetadataBuilder {
    /// List of metadata items
    pub(crate) metadata: Vec<MetadataItem>,
}

impl MetadataBuilder {
    /// Creates a new empty `MetadataBuilder` instance
    pub(crate) fn new() -> Self {
        Self { metadata: Vec::new() }
    }

    /// Add a metadata item
    ///
    /// Appends a new metadata item to the builder.
    ///
    /// ## Parameters
    /// - `item`: The metadata item to add
    ///
    /// ## Return
    /// - `&mut Self`: Returns a mutable reference to itself for method chaining
    pub fn add(&mut self, item: MetadataItem) -> &mut Self {
        self.metadata.push(item);
        self
    }

    /// Clear all metadata items
    ///
    /// Removes all metadata items from the builder.
    pub fn clear(&mut self) -> &mut Self {
        self.metadata.clear();
        self
    }

    /// Generate the metadata XML content
    ///
    /// Writes the XML representation of the metadata to the provided writer.
    /// This includes all metadata items and their refinements, as well as
    /// automatically adding a `dcterms:modified` timestamp.
    pub(crate) fn make(&mut self, writer: &mut XmlWriter) -> Result<(), EpubError> {
        self.metadata.push(MetadataItem {
            id: None,
            property: "dcterms:modified".to_string(),
            value: Utc::now().to_rfc3339_opts(SecondsFormat::AutoSi, true),
            lang: None,
            refined: vec![],
        });

        writer.write_event(Event::Start(BytesStart::new("metadata")))?;

        for metadata in &self.metadata {
            let tag_name = if ELEMENT_IN_DC_NAMESPACE.contains(&metadata.property.as_str()) {
                format!("dc:{}", metadata.property)
            } else {
                "meta".to_string()
            };

            writer.write_event(Event::Start(
                BytesStart::new(tag_name.as_str()).with_attributes(metadata.attributes()),
            ))?;
            writer.write_event(Event::Text(BytesText::new(metadata.value.as_str())))?;
            writer.write_event(Event::End(BytesEnd::new(tag_name.as_str())))?;

            for refinement in &metadata.refined {
                writer.write_event(Event::Start(
                    BytesStart::new("meta").with_attributes(refinement.attributes()),
                ))?;
                writer.write_event(Event::Text(BytesText::new(refinement.value.as_str())))?;
                writer.write_event(Event::End(BytesEnd::new("meta")))?;
            }
        }

        writer.write_event(Event::End(BytesEnd::new("metadata")))?;

        Ok(())
    }

    /// Verify metadata integrity
    ///
    /// Check if the required metadata items are included: title, language, and identifier with pub-id.
    pub(crate) fn validate(&self) -> Result<(), EpubError> {
        let has_title = self.metadata.iter().any(|item| item.property == "title");
        let has_language = self.metadata.iter().any(|item| item.property == "language");
        let has_identifier = self.metadata.iter().any(|item| {
            item.property == "identifier" && item.id.as_ref().is_some_and(|id| id == "pub-id")
        });

        if has_title && has_identifier && has_language {
            Ok(())
        } else {
            Err(EpubBuilderError::MissingNecessaryMetadata.into())
        }
    }
}

/// Manifest builder for EPUB resources
///
/// The `ManifestBuilder` is responsible for managing manifest items in an EPUB publication.
/// The manifest declares all resources (HTML files, images, stylesheets, fonts, etc.)
/// that are part of the EPUB publication.
///
/// Each manifest item must have a unique identifier and a path to the resource file.
/// The builder automatically determines the MIME type of each resource based on its content.
///
/// ## Resource Fallbacks
///
/// The manifest supports fallback chains for resources that may not be supported by all
/// reading systems. When adding a resource with a fallback, the builder validates that:
/// - The fallback chain does not contain circular references
/// - All referenced fallback resources exist in the manifest
///
/// ## Navigation Document
///
/// The manifest must contain exactly one item with the `nav` property, which serves
/// as the navigation document (table of contents) of the publication.
#[derive(Debug)]
pub struct ManifestBuilder {
    /// Temporary directory for storing files during build
    temp_dir: PathBuf,

    /// Rootfile path (OPF file location)
    rootfile: Option<String>,

    /// Manifest items stored in a map keyed by ID
    #[cfg(feature = "no-indexmap")]
    pub(crate) manifest: HashMap<String, ManifestItem>,
    #[cfg(not(feature = "no-indexmap"))]
    pub(crate) manifest: IndexMap<String, ManifestItem>,
}

impl ManifestBuilder {
    /// Creates a new `ManifestBuilder` instance
    ///
    /// ## Parameters
    /// - `temp_dir`: Temporary directory path for storing files during the build process
    pub(crate) fn new(temp_dir: impl AsRef<Path>) -> Self {
        Self {
            temp_dir: temp_dir.as_ref().to_path_buf(),
            rootfile: None,
            #[cfg(feature = "no-indexmap")]
            manifest: HashMap::new(),
            #[cfg(not(feature = "no-indexmap"))]
            manifest: IndexMap::new(),
        }
    }

    /// Set the rootfile path
    ///
    /// This must be called before adding manifest items.
    ///
    /// ## Parameters
    /// - `rootfile`: The rootfile path
    pub(crate) fn set_rootfile(&mut self, rootfile: impl Into<String>) {
        self.rootfile = Some(rootfile.into());
    }

    /// Add a manifest item and copy the resource file
    ///
    /// Adds a new resource to the manifest and copies the source file to the
    /// temporary directory. The builder automatically determines the MIME type
    /// based on the file content.
    ///
    /// ## Parameters
    /// - `manifest_source`: Path to the source file on the local filesystem
    /// - `manifest_item`: Manifest item with ID and target path
    ///
    /// ## Return
    /// - `Ok(&mut Self)`: Successfully added the resource
    /// - `Err(EpubError)`: Error if the source file doesn't exist or has an unknown format
    pub fn add(
        &mut self,
        manifest_source: impl Into<String>,
        manifest_item: ManifestItem,
    ) -> Result<&mut Self, EpubError> {
        // Check if the source path is a file
        let manifest_source = manifest_source.into();
        let source = PathBuf::from(&manifest_source);
        if !source.is_file() {
            return Err(EpubBuilderError::TargetIsNotFile { target_path: manifest_source }.into());
        }

        // Get the file extension
        let extension = match source.extension() {
            Some(ext) => ext.to_string_lossy().to_lowercase(),
            None => String::new(),
        };

        // Read the file
        let buf = fs::read(source)?;

        // Get the mime type
        let real_mime = match Infer::new().get(&buf) {
            Some(infer_mime) => refine_mime_type(infer_mime.mime_type(), &extension),
            None => {
                return Err(
                    EpubBuilderError::UnknownFileFormat { file_path: manifest_source }.into(),
                );
            }
        };

        let target_path = normalize_manifest_path(
            &self.temp_dir,
            self.rootfile
                .as_ref()
                .ok_or(EpubBuilderError::MissingRootfile)?,
            &manifest_item.path,
            &manifest_item.id,
        )?;
        if let Some(parent_dir) = target_path.parent() {
            if !parent_dir.exists() {
                fs::create_dir_all(parent_dir)?
            }
        }

        match fs::write(target_path, buf) {
            Ok(_) => {
                self.manifest
                    .insert(manifest_item.id.clone(), manifest_item.set_mime(real_mime));
                Ok(self)
            }
            Err(err) => Err(err.into()),
        }
    }

    /// Clear all manifest items
    ///
    /// Removes all manifest items from the builder and deletes the associated files
    /// from the temporary directory.
    pub fn clear(&mut self) -> &mut Self {
        let paths = self
            .manifest
            .values()
            .map(|manifest| &manifest.path)
            .collect::<Vec<&PathBuf>>();

        for path in paths {
            let _ = fs::remove_file(path);
        }

        self.manifest.clear();

        self
    }

    /// Insert a manifest item directly
    ///
    /// This method allows direct insertion of a manifest item without copying
    /// any files. Use this when the file already exists in the temporary directory.
    pub(crate) fn insert(
        &mut self,
        key: impl Into<String>,
        value: ManifestItem,
    ) -> Option<ManifestItem> {
        self.manifest.insert(key.into(), value)
    }

    /// Generate the manifest XML content
    ///
    /// Writes the XML representation of the manifest to the provided writer.
    pub(crate) fn make(&self, writer: &mut XmlWriter) -> Result<(), EpubError> {
        writer.write_event(Event::Start(BytesStart::new("manifest")))?;

        for manifest in self.manifest.values() {
            writer.write_event(Event::Empty(
                BytesStart::new("item").with_attributes(manifest.attributes()),
            ))?;
        }

        writer.write_event(Event::End(BytesEnd::new("manifest")))?;

        Ok(())
    }

    /// Validate manifest integrity
    ///
    /// Checks fallback chains for circular references and missing items,
    /// and verifies that exactly one nav item exists.
    pub(crate) fn validate(&self) -> Result<(), EpubError> {
        self.validate_fallback_chains()?;
        self.validate_nav()?;

        Ok(())
    }

    /// Get manifest item keys
    ///
    /// Returns an iterator over the keys (IDs) of all manifest items.
    ///
    /// ## Return
    /// - `impl Iterator<Item = &String>`: Iterator over manifest item keys
    pub(crate) fn keys(&self) -> impl Iterator<Item = &String> {
        self.manifest.keys()
    }

    // TODO: consider using BFS to validate fallback chains, to provide efficient
    /// Validate all fallback chains in the manifest
    ///
    /// Iterates through all manifest items and validates each fallback chain
    /// to ensure there are no circular references and all referenced items exist.
    fn validate_fallback_chains(&self) -> Result<(), EpubError> {
        for (id, item) in &self.manifest {
            if item.fallback.is_none() {
                continue;
            }

            let mut fallback_chain = Vec::new();
            self.validate_fallback_chain(id, &mut fallback_chain)?;
        }

        Ok(())
    }

    /// Recursively verify the validity of a single fallback chain
    ///
    /// This function recursively traces the fallback chain to check for the following issues:
    /// - Circular reference
    /// - The referenced fallback resource does not exist
    fn validate_fallback_chain(
        &self,
        manifest_id: &str,
        fallback_chain: &mut Vec<String>,
    ) -> Result<(), EpubError> {
        if fallback_chain.contains(&manifest_id.to_string()) {
            fallback_chain.push(manifest_id.to_string());

            return Err(EpubBuilderError::ManifestCircularReference {
                fallback_chain: fallback_chain.join("->"),
            }
            .into());
        }

        // Get the current item; its existence can be ensured based on the calling context.
        let item = self.manifest.get(manifest_id).unwrap();

        if let Some(fallback_id) = &item.fallback {
            if !self.manifest.contains_key(fallback_id) {
                return Err(EpubBuilderError::ManifestNotFound {
                    manifest_id: fallback_id.to_owned(),
                }
                .into());
            }

            fallback_chain.push(manifest_id.to_string());
            self.validate_fallback_chain(fallback_id, fallback_chain)
        } else {
            // The end of the fallback chain
            Ok(())
        }
    }

    /// Validate navigation list items
    ///
    /// Check if there is only one list item with the `nav` property.
    fn validate_nav(&self) -> Result<(), EpubError> {
        if self
            .manifest
            .values()
            .filter(|&item| {
                if let Some(properties) = &item.properties {
                    properties.split(" ").any(|property| property == "nav")
                } else {
                    false
                }
            })
            .count()
            == 1
        {
            Ok(())
        } else {
            Err(EpubBuilderError::TooManyNavFlags.into())
        }
    }
}

/// Spine builder for EPUB reading order
///
/// The `SpineBuilder` is responsible for managing the spine items in an EPUB publication.
/// The spine defines the default reading order of the publication - the sequence in which
/// the reading system should present the content documents to the reader.
///
/// Each spine item references a manifest item by its ID (idref), indicating which
/// resource should be displayed at that point in the reading order.
#[derive(Debug)]
pub struct SpineBuilder {
    /// List of spine items defining the reading order
    pub(crate) spine: Vec<SpineItem>,
}

impl SpineBuilder {
    /// Creates a new empty `SpineBuilder` instance
    pub(crate) fn new() -> Self {
        Self { spine: Vec::new() }
    }

    /// Add a spine item
    ///
    /// Appends a new spine item to the builder, defining the next position in
    /// the reading order.
    ///
    /// ## Parameters
    /// - `item`: The spine item to add
    ///
    /// ## Return
    /// - `&mut Self`: Returns a mutable reference to itself for method chaining
    pub fn add(&mut self, item: SpineItem) -> &mut Self {
        self.spine.push(item);
        self
    }

    /// Clear all spine items
    ///
    /// Removes all spine items from the builder.
    pub fn clear(&mut self) -> &mut Self {
        self.spine.clear();
        self
    }

    /// Generate the spine XML content
    ///
    /// Writes the XML representation of the spine to the provided writer.
    pub(crate) fn make(&self, writer: &mut XmlWriter) -> Result<(), EpubError> {
        writer.write_event(Event::Start(BytesStart::new("spine")))?;

        for spine in &self.spine {
            writer.write_event(Event::Empty(
                BytesStart::new("itemref").with_attributes(spine.attributes()),
            ))?;
        }

        writer.write_event(Event::End(BytesEnd::new("spine")))?;

        Ok(())
    }

    /// Validate spine references
    ///
    /// Checks that all spine item idref values exist in the manifest.
    ///
    /// ## Parameters
    /// - `manifest_keys`: Iterator over manifest item keys
    pub(crate) fn validate(
        &self,
        manifest_keys: impl Iterator<Item = impl AsRef<str>>,
    ) -> Result<(), EpubError> {
        let manifest_keys: Vec<String> = manifest_keys.map(|k| k.as_ref().to_string()).collect();
        for spine in &self.spine {
            if !manifest_keys.contains(&spine.idref) {
                return Err(
                    EpubBuilderError::SpineManifestNotFound { idref: spine.idref.clone() }.into(),
                );
            }
        }
        Ok(())
    }
}

/// Catalog builder for EPUB navigation
///
/// The `CatalogBuilder` is responsible for building the navigation document (TOC)
/// of an EPUB publication. The navigation document provides a hierarchical table
/// of contents that allows readers to navigate through the publication's content.
///
/// The navigation document is a special XHTML document that uses the EPUB Navigation
/// Document specification.
#[derive(Debug)]
pub struct CatalogBuilder {
    /// Title of the navigation document
    pub(crate) title: String,

    /// Navigation points (table of contents entries)
    pub(crate) catalog: Vec<NavPoint>,
}

impl CatalogBuilder {
    /// Creates a new empty `CatalogBuilder` instance
    pub(crate) fn new() -> Self {
        Self {
            title: String::new(),
            catalog: Vec::new(),
        }
    }

    /// Set the catalog title
    ///
    /// Sets the title that will be displayed at the top of the navigation document.
    ///
    /// ## Parameters
    /// - `title`: The title to set
    ///
    /// ## Return
    /// - `&mut Self`: Returns a mutable reference to itself for method chaining
    pub fn set_title(&mut self, title: impl Into<String>) -> &mut Self {
        self.title = title.into();
        self
    }

    /// Add a navigation point
    ///
    /// Appends a new navigation point to the catalog. Navigation points can be
    /// nested by using the `append_child` method on `NavPoint`.
    ///
    /// ## Parameters
    /// - `item`: The navigation point to add
    ///
    /// ## Return
    /// - `&mut Self`: Returns a mutable reference to itself for method chaining
    pub fn add(&mut self, item: NavPoint) -> &mut Self {
        self.catalog.push(item);
        self
    }

    /// Clear all catalog items
    ///
    /// Removes the title and all navigation points from the builder.
    pub fn clear(&mut self) -> &mut Self {
        self.title.clear();
        self.catalog.clear();
        self
    }

    /// Check if the catalog is empty
    ///
    /// ## Return
    /// - `true`: No navigation points have been added
    /// - `false`: At least one navigation point has been added
    pub(crate) fn is_empty(&self) -> bool {
        self.catalog.is_empty()
    }

    /// Generate the navigation document
    ///
    /// Creates the EPUB Navigation Document (NAV) as XHTML content with the
    /// specified title and navigation points.
    pub(crate) fn make(&self, writer: &mut XmlWriter) -> Result<(), EpubError> {
        writer.write_event(Event::Start(BytesStart::new("html").with_attributes([
            ("xmlns", "http://www.w3.org/1999/xhtml"),
            ("xmlns:epub", "http://www.idpf.org/2007/ops"),
        ])))?;

        // make head
        writer.write_event(Event::Start(BytesStart::new("head")))?;
        writer.write_event(Event::Start(BytesStart::new("title")))?;
        writer.write_event(Event::Text(BytesText::new(&self.title)))?;
        writer.write_event(Event::End(BytesEnd::new("title")))?;
        writer.write_event(Event::End(BytesEnd::new("head")))?;

        // make body
        writer.write_event(Event::Start(BytesStart::new("body")))?;
        writer.write_event(Event::Start(
            BytesStart::new("nav").with_attributes([("epub:type", "toc")]),
        ))?;

        if !self.title.is_empty() {
            writer.write_event(Event::Start(BytesStart::new("h1")))?;
            writer.write_event(Event::Text(BytesText::new(&self.title)))?;
            writer.write_event(Event::End(BytesEnd::new("h1")))?;
        }

        Self::make_nav(writer, &self.catalog)?;

        writer.write_event(Event::End(BytesEnd::new("nav")))?;
        writer.write_event(Event::End(BytesEnd::new("body")))?;

        writer.write_event(Event::End(BytesEnd::new("html")))?;

        Ok(())
    }

    /// Generate navigation list items recursively
    ///
    /// Recursively writes the navigation list (ol/li elements) for the given
    /// navigation points.
    fn make_nav(writer: &mut XmlWriter, navgations: &Vec<NavPoint>) -> Result<(), EpubError> {
        writer.write_event(Event::Start(BytesStart::new("ol")))?;

        for nav in navgations {
            writer.write_event(Event::Start(BytesStart::new("li")))?;

            if let Some(path) = &nav.content {
                writer.write_event(Event::Start(
                    BytesStart::new("a").with_attributes([("href", path.to_string_lossy())]),
                ))?;
                writer.write_event(Event::Text(BytesText::new(nav.label.as_str())))?;
                writer.write_event(Event::End(BytesEnd::new("a")))?;
            } else {
                writer.write_event(Event::Start(BytesStart::new("span")))?;
                writer.write_event(Event::Text(BytesText::new(nav.label.as_str())))?;
                writer.write_event(Event::End(BytesEnd::new("span")))?;
            }

            if !nav.children.is_empty() {
                Self::make_nav(writer, &nav.children)?;
            }

            writer.write_event(Event::End(BytesEnd::new("li")))?;
        }

        writer.write_event(Event::End(BytesEnd::new("ol")))?;

        Ok(())
    }
}

#[cfg(feature = "content-builder")]
#[derive(Debug)]
pub struct DocumentBuilder {
    pub(crate) documents: Vec<(PathBuf, ContentBuilder)>,
}

#[cfg(feature = "content-builder")]
impl DocumentBuilder {
    /// Creates a new empty `DocumentBuilder` instance
    pub(crate) fn new() -> Self {
        Self { documents: Vec::new() }
    }

    /// Add a content document
    ///
    /// Appends a new content document to be processed during EPUB building.
    ///
    /// ## Parameters
    /// - `target`: The target path within the EPUB container where the content will be placed
    /// - `content`: The content builder containing the document content
    ///
    /// ## Return
    /// - `&mut Self`: Returns a mutable reference to itself for method chaining
    pub fn add(&mut self, target: impl AsRef<str>, content: ContentBuilder) -> &mut Self {
        self.documents
            .push((PathBuf::from(target.as_ref()), content));
        self
    }

    /// Clear all documents
    ///
    /// Removes all content documents from the builder.
    pub fn clear(&mut self) -> &mut Self {
        self.documents.clear();
        self
    }

    /// Generate manifest items from content documents
    ///
    /// Processes all content documents and generates the corresponding manifest items.
    /// Each content document may generate multiple manifest entries - one for the main
    /// document and additional entries for any resources (images, fonts, etc.) it contains.
    ///
    /// ## Parameters
    /// - `temp_dir`: The temporary directory path used during the EPUB build process
    /// - `rootfile`: The path to the OPF file (package document)
    ///
    /// ## Return
    /// - `Ok(Vec<ManifestItem>)`: List of manifest items generated from the content documents
    /// - `Err(EpubError)`: Error if document generation or file processing fails
    pub fn make(
        &mut self,
        temp_dir: PathBuf,
        rootfile: impl AsRef<str>,
    ) -> Result<Vec<ManifestItem>, EpubError> {
        let mut buf = vec![0; 512];
        let contents = std::mem::take(&mut self.documents);

        let mut manifest = Vec::new();
        for (target, mut content) in contents.into_iter() {
            let manifest_id = content.id.clone();

            // target is relative to the epub file, so we need to normalize it
            let absolute_target =
                normalize_manifest_path(&temp_dir, &rootfile, &target, &manifest_id)?;
            let mut resources = content.make(&absolute_target)?;

            // Helper to compute absolute container path
            let to_container_path = |p: &PathBuf| -> PathBuf {
                match p.strip_prefix(&temp_dir) {
                    Ok(rel) => PathBuf::from("/").join(rel.to_string_lossy().replace("\\", "/")),
                    Err(_) => unreachable!("path MUST under temp directory"),
                }
            };

            // Document (first element, guaranteed to exist)
            let path = resources.swap_remove(0);
            let mut file = std::fs::File::open(&path)?;
            let _ = file.read(&mut buf)?;
            let extension = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let mime = match Infer::new().get(&buf) {
                Some(infer) => refine_mime_type(infer.mime_type(), &extension),
                None => {
                    return Err(EpubBuilderError::UnknownFileFormat {
                        file_path: path.to_string_lossy().to_string(),
                    }
                    .into());
                }
            }
            .to_string();

            manifest.push(ManifestItem {
                id: manifest_id.clone(),
                path: to_container_path(&path),
                mime,
                properties: None,
                fallback: None,
            });

            // Other resources (if any): generate stable ids and add to manifest
            for res in resources {
                let mut file = fs::File::open(&res)?;
                let _ = file.read(&mut buf)?;
                let extension = res
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                let mime = match Infer::new().get(&buf) {
                    Some(ft) => refine_mime_type(ft.mime_type(), &extension),
                    None => {
                        return Err(EpubBuilderError::UnknownFileFormat {
                            file_path: path.to_string_lossy().to_string(),
                        }
                        .into());
                    }
                }
                .to_string();

                let file_name = res
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let res_id = format!("{}-{}", manifest_id, file_name);

                manifest.push(ManifestItem {
                    id: res_id,
                    path: to_container_path(&res),
                    mime,
                    properties: None,
                    fallback: None,
                });
            }
        }

        Ok(manifest)
    }
}

/// EPUB Builder
///
/// The main structure used to create and build EPUB ebook files.
/// Supports the EPUB 3.0 specification and can build a complete EPUB file structure.
///
/// ## Usage
///
/// ```rust, no_run
/// # #[cfg(feature = "builder")]
/// # fn main() -> Result<(), lib_epub::error::EpubError> {
/// use lib_epub::{
///     builder::{EpubBuilder, EpubVersion3},
///     types::{MetadataItem, ManifestItem, NavPoint, SpineItem},
/// };
///
/// let mut builder = EpubBuilder::<EpubVersion3>::new()?;
///
/// builder
///     .add_rootfile("EPUB/content.opf")?
///     .add_metadata(MetadataItem::new("title", "Test Book"))
///     .add_metadata(MetadataItem::new("language", "en"))
///     .add_metadata(
///         MetadataItem::new("identifier", "unique-id")
///             .with_id("pub-id")
///             .build(),
///     )
///     .add_manifest(
///         "./test_case/Overview.xhtml",
///         ManifestItem::new("content", "target/path")?,
///     )?
///     .add_spine(SpineItem::new("content"))
///     .add_catalog_item(NavPoint::new("label"));
///
/// builder.build("output.epub")?;
///
/// # Ok(())
/// # }
/// ```
///
/// ## Notes
///
/// - All resource files **must** exist on the local file system.
/// - **At least one rootfile** must be added before adding manifest items.
/// - Requires at least one `title`, `language`, and `identifier` with id `pub-id`.
#[cfg_attr(test, derive(Debug))]
pub struct EpubBuilder<Version> {
    /// EPUB version placeholder
    epub_version: PhantomData<Version>,

    /// Temporary directory path for storing files during the build process
    temp_dir: PathBuf,

    rootfiles: RootfileBuilder,
    metadata: MetadataBuilder,
    manifest: ManifestBuilder,
    spine: SpineBuilder,
    catalog: CatalogBuilder,

    #[cfg(feature = "content-builder")]
    content: DocumentBuilder,
}

impl EpubBuilder<EpubVersion3> {
    /// Create a new `EpubBuilder` instance
    ///
    /// ## Return
    /// - `Ok(EpubBuilder)`: Builder instance created successfully
    /// - `Err(EpubError)`: Error occurred during builder initialization
    pub fn new() -> Result<Self, EpubError> {
        let temp_dir = env::temp_dir().join(local_time());
        fs::create_dir(&temp_dir)?;
        fs::create_dir(temp_dir.join("META-INF"))?;

        let mime_file = temp_dir.join("mimetype");
        fs::write(mime_file, "application/epub+zip")?;

        Ok(EpubBuilder {
            epub_version: PhantomData,
            temp_dir: temp_dir.clone(),

            rootfiles: RootfileBuilder::new(),
            metadata: MetadataBuilder::new(),
            manifest: ManifestBuilder::new(temp_dir),
            spine: SpineBuilder::new(),
            catalog: CatalogBuilder::new(),

            #[cfg(feature = "content-builder")]
            content: DocumentBuilder::new(),
        })
    }

    /// Add a rootfile path
    ///
    /// The added path points to an OPF file that does not yet exist
    /// and will be created when building the Epub file.
    ///
    /// ## Parameters
    /// - `rootfile`: Rootfile path
    ///
    /// ## Notes
    /// - The added rootfile path must be a relative path and cannot start with "../".
    /// - At least one rootfile must be added before adding metadata items.
    pub fn add_rootfile(&mut self, rootfile: impl AsRef<str>) -> Result<&mut Self, EpubError> {
        match self.rootfiles.add(rootfile) {
            Ok(_) => Ok(self),
            Err(err) => Err(err),
        }
    }

    /// Add metadata item
    ///
    /// Required metadata includes title, language, and an identifier with 'pub-id'.
    /// Missing this data will result in an error when building the epub file.
    ///
    /// ## Parameters
    /// - `item`: Metadata items to add
    pub fn add_metadata(&mut self, item: MetadataItem) -> &mut Self {
        let _ = self.metadata.add(item);
        self
    }

    /// Add manifest item and corresponding resource file
    ///
    /// The builder will automatically recognize the file type of
    /// the added resource and update it in `ManifestItem`.
    ///
    /// ## Parameters
    /// - `manifest_source` - Local resource file path
    /// - `manifest_item` - Manifest item information
    ///
    /// ## Return
    /// - `Ok(&mut Self)` - Successful addition, returns a reference to itself
    /// - `Err(EpubError)` - Error occurred during the addition process
    ///
    /// ## Notes
    /// - At least one rootfile must be added before adding manifest items.
    /// - If the manifest item ID already exists in the manifest, the manifest item will be overwritten.
    pub fn add_manifest(
        &mut self,
        manifest_source: impl Into<String>,
        manifest_item: ManifestItem,
    ) -> Result<&mut Self, EpubError> {
        if self.rootfiles.is_empty() {
            return Err(EpubBuilderError::MissingRootfile.into());
        } else {
            self.manifest
                .set_rootfile(self.rootfiles.first().expect("Unreachable"));
        }

        match self.manifest.add(manifest_source, manifest_item) {
            Ok(_) => Ok(self),
            Err(err) => Err(err),
        }
    }

    /// Add spine item
    ///
    /// The spine item defines the reading order of the book.
    ///
    /// ## Parameters
    /// - `item`: Spine item to add
    pub fn add_spine(&mut self, item: SpineItem) -> &mut Self {
        self.spine.add(item);
        self
    }

    /// Set catalog title
    ///
    /// ## Parameters
    /// - `title`: Catalog title
    pub fn set_catalog_title(&mut self, title: impl Into<String>) -> &mut Self {
        let _ = self.catalog.set_title(title);
        self
    }

    /// Add catalog item
    ///
    /// Added directory items will be added to the end of the existing list.
    ///
    /// ## Parameters
    /// - `item`: Catalog item to add
    pub fn add_catalog_item(&mut self, item: NavPoint) -> &mut Self {
        let _ = self.catalog.add(item);
        self
    }

    /// Add content
    ///
    /// The content builder can be used to generate content for the book.
    /// It is recommended to use the `content-builder` feature to use this function.
    ///
    /// ## Parameters
    /// - `target_path`: The path to the resource file within the EPUB container
    /// - `content`: The content builder to generate content
    #[cfg(feature = "content-builder")]
    pub fn add_content(
        &mut self,
        target_path: impl AsRef<str>,
        content: ContentBuilder,
    ) -> &mut Self {
        self.content.add(target_path, content);
        self
    }

    /// Clear all data from the builder
    ///
    /// This function clears all metadata, manifest items, spine items, catalog items, etc.
    /// from the builder, effectively resetting it to an empty state.
    ///
    /// ## Return
    /// - `Ok(&mut Self)`: Successfully cleared all data
    /// - `Err(EpubError)`: Error occurred during the clearing process (specifically during manifest clearing)
    pub fn clear_all(&mut self) -> &mut Self {
        self.rootfiles.clear();
        self.metadata.clear();
        self.manifest.clear();
        self.spine.clear();
        self.catalog.clear();
        #[cfg(feature = "content-builder")]
        self.content.clear();

        self
    }

    /// Get a mutable reference to the rootfile builder
    ///
    /// Allows direct manipulation of rootfile entries.
    ///
    /// ## Return
    /// - `&mut RootfileBuilder`: Mutable reference to the rootfile builder
    pub fn rootfile(&mut self) -> &mut RootfileBuilder {
        &mut self.rootfiles
    }

    /// Get a mutable reference to the metadata builder
    ///
    /// Allows direct manipulation of metadata items.
    ///
    /// ## Return
    /// - `&mut MetadataBuilder`: Mutable reference to the metadata builder
    pub fn metadata(&mut self) -> &mut MetadataBuilder {
        &mut self.metadata
    }

    /// Get a mutable reference to the manifest builder
    ///
    /// Allows direct manipulation of manifest items.
    ///
    /// ## Return
    /// - `&mut ManifestBuilder`: Mutable reference to the manifest builder
    pub fn manifest(&mut self) -> &mut ManifestBuilder {
        &mut self.manifest
    }

    /// Get a mutable reference to the spine builder
    ///
    /// Allows direct manipulation of spine items.
    ///
    /// ## Return
    /// - `&mut SpineBuilder`: Mutable reference to the spine builder
    pub fn spine(&mut self) -> &mut SpineBuilder {
        &mut self.spine
    }

    /// Get a mutable reference to the catalog builder
    ///
    /// Allows direct manipulation of navigation/catalog items.
    ///
    /// ## Return
    /// - `&mut CatalogBuilder`: Mutable reference to the catalog builder
    pub fn catalog(&mut self) -> &mut CatalogBuilder {
        &mut self.catalog
    }

    /// Get a mutable reference to the content builder
    ///
    /// Allows direct manipulation of content documents.
    ///
    /// ## Return
    /// - `&mut DocumentBuilder`: Mutable reference to the document builder
    #[cfg(feature = "content-builder")]
    pub fn content(&mut self) -> &mut DocumentBuilder {
        &mut self.content
    }

    /// Builds an EPUB file and saves it to the specified path
    ///
    /// ## Parameters
    /// - `output_path`: Output file path
    ///
    /// ## Return
    /// - `Ok(())`: Build successful
    /// - `Err(EpubError)`: Error occurred during the build process
    pub fn make(mut self, output_path: impl AsRef<Path>) -> Result<(), EpubError> {
        // Create the container.xml, navigation document, and OPF files in sequence.
        // The associated metadata will initialized when navigation document is created;
        // therefore, the navigation document must be created before the opf file is created.
        self.make_container_xml()?;
        self.make_navigation_document()?;
        #[cfg(feature = "content-builder")]
        self.make_contents()?;
        self.make_opf_file()?;
        self.remove_empty_dirs()?;

        if let Some(parent) = output_path.as_ref().parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        // pack zip file
        let file = File::create(output_path)?;
        let mut zip = ZipWriter::new(file);
        let options = FileOptions::<()>::default().compression_method(CompressionMethod::Stored);

        for entry in WalkDir::new(&self.temp_dir) {
            let entry = entry?;
            let path = entry.path();

            // It can be asserted that the path is prefixed with temp_dir,
            // and there will be no boundary cases of symbolic links and hard links, etc.
            let relative_path = path.strip_prefix(&self.temp_dir).unwrap();
            let target_path = relative_path.to_string_lossy().replace("\\", "/");

            if path.is_file() {
                zip.start_file(target_path, options)?;

                let mut buf = Vec::new();
                File::open(path)?.read_to_end(&mut buf)?;

                zip.write_all(&buf)?;
            } else if path.is_dir() {
                zip.add_directory(target_path, options)?;
            }
        }

        zip.finish()?;
        Ok(())
    }

    /// Builds an EPUB file and returns a `EpubDoc`
    ///
    /// Builds an EPUB file at the specified location and parses it into a usable EpubDoc object.
    ///
    /// ## Parameters
    /// - `output_path`: Output file path
    ///
    /// ## Return
    /// - `Ok(EpubDoc)`: Build successful
    /// - `Err(EpubError)`: Error occurred during the build process
    pub fn build(
        self,
        output_path: impl AsRef<Path>,
    ) -> Result<EpubDoc<BufReader<File>>, EpubError> {
        self.make(&output_path)?;

        EpubDoc::new(output_path)
    }

    /// Creates an `EpubBuilder` instance from an existing `EpubDoc`
    ///
    /// This function takes an existing parsed EPUB document and creates a new builder
    /// instance with all the document's metadata, manifest items, spine, and catalog information.
    /// It essentially reverses the EPUB building process by extracting all the necessary
    /// components from the parsed document and preparing them for reconstruction.
    ///
    /// The function copies the following information from the provided `EpubDoc`:
    /// - Rootfile path (based on the document's base path)
    /// - All metadata items (title, author, identifier, etc.)
    /// - Spine items (reading order of the publication)
    /// - Catalog information (navigation points)
    /// - Catalog title
    /// - All manifest items (except those with 'nav' property, which are skipped)
    ///
    /// ## Parameters
    /// - `doc`: A mutable reference to an `EpubDoc` instance that contains the parsed EPUB data
    ///
    /// ## Return
    /// - `Ok(EpubBuilder)`: Successfully created builder instance populated with the document's data
    /// - `Err(EpubError)`: Error occurred during the extraction process
    ///
    /// ## Notes
    /// - This type of conversion will upgrade Epub2.x publications to Epub3.x.
    ///   This upgrade conversion may encounter unknown errors (it is unclear whether
    ///   it will cause errors), so please use it with caution.
    pub fn from<R: Read + Seek>(doc: &mut EpubDoc<R>) -> Result<Self, EpubError> {
        let mut builder = Self::new()?;

        builder.add_rootfile(doc.package_path.clone().to_string_lossy())?;
        builder.metadata.metadata = doc.metadata.clone();
        builder.spine.spine = doc.spine.clone();
        builder.catalog.catalog = doc.catalog.clone();
        builder.catalog.title = doc.catalog_title.clone();

        // clone manifest hashmap to avoid mut borrow conflict
        for (_, mut manifest) in doc.manifest.clone().into_iter() {
            if let Some(properties) = &manifest.properties {
                if properties.contains("nav") {
                    continue;
                }
            }

            // because manifest paths in EpubDoc are converted to absolute paths rooted in containers,
            // but in the form of 'path/to/manifest', they need to be converted here to absolute paths
            // in the form of '/path/to/manifest'.
            manifest.path = PathBuf::from("/").join(manifest.path);

            let (buf, _) = doc.get_manifest_item(&manifest.id)?; // read raw file
            let target_path = normalize_manifest_path(
                &builder.temp_dir,
                builder.rootfiles.first().expect("Unreachable"),
                &manifest.path,
                &manifest.id,
            )?;
            if let Some(parent_dir) = target_path.parent() {
                if !parent_dir.exists() {
                    fs::create_dir_all(parent_dir)?
                }
            }

            fs::write(target_path, buf)?;
            builder
                .manifest
                .manifest
                .insert(manifest.id.clone(), manifest);
        }

        Ok(builder)
    }

    /// Creates the `container.xml` file
    ///
    /// An error will occur if the `rootfile` path is not set
    fn make_container_xml(&self) -> Result<(), EpubError> {
        if self.rootfiles.is_empty() {
            return Err(EpubBuilderError::MissingRootfile.into());
        }

        let mut writer = Writer::new(Cursor::new(Vec::new()));
        self.rootfiles.make(&mut writer)?;

        let file_path = self.temp_dir.join("META-INF").join("container.xml");
        let file_data = writer.into_inner().into_inner();
        fs::write(file_path, file_data)?;

        Ok(())
    }

    /// Creates the content document
    #[cfg(feature = "content-builder")]
    fn make_contents(&mut self) -> Result<(), EpubError> {
        let manifest_list = self.content.make(
            self.temp_dir.clone(),
            self.rootfiles.first().expect("Unreachable"),
        )?;

        for item in manifest_list.into_iter() {
            self.manifest.insert(item.id.clone(), item);
        }

        Ok(())
    }

    /// Creates the `navigation document`
    ///
    /// An error will occur if navigation information is not initialized.
    fn make_navigation_document(&mut self) -> Result<(), EpubError> {
        if self.catalog.is_empty() {
            return Err(EpubBuilderError::NavigationInfoUninitalized.into());
        }

        let mut writer = Writer::new(Cursor::new(Vec::new()));
        self.catalog.make(&mut writer)?;

        let file_path = self.temp_dir.join("nav.xhtml");
        let file_data = writer.into_inner().into_inner();
        fs::write(file_path, file_data)?;

        self.manifest.insert(
            "nav".to_string(),
            ManifestItem {
                id: "nav".to_string(),
                path: PathBuf::from("/nav.xhtml"),
                mime: "application/xhtml+xml".to_string(),
                properties: Some("nav".to_string()),
                fallback: None,
            },
        );

        Ok(())
    }

    /// Creates the `OPF` file
    ///
    /// ## Error conditions
    /// - Missing necessary metadata
    /// - Circular reference exists in the manifest backlink
    /// - Navigation information is not initialized
    fn make_opf_file(&mut self) -> Result<(), EpubError> {
        self.metadata.validate()?;
        self.manifest.validate()?;
        self.spine.validate(self.manifest.keys())?;

        let mut writer = Writer::new(Cursor::new(Vec::new()));

        writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;

        writer.write_event(Event::Start(BytesStart::new("package").with_attributes([
            ("xmlns", "http://www.idpf.org/2007/opf"),
            ("xmlns:dc", "http://purl.org/dc/elements/1.1/"),
            ("unique-identifier", "pub-id"),
            ("version", "3.0"),
        ])))?;

        self.metadata.make(&mut writer)?;
        self.manifest.make(&mut writer)?;
        self.spine.make(&mut writer)?;

        writer.write_event(Event::End(BytesEnd::new("package")))?;

        let file_path = self
            .temp_dir
            .join(self.rootfiles.first().expect("Unreachable"));
        let file_data = writer.into_inner().into_inner();
        fs::write(file_path, file_data)?;

        Ok(())
    }

    /// Remove empty directories under the builder temporary directory
    ///
    /// By enumerate directories under `self.temp_dir` (excluding the root itself)
    /// and deletes directories that are empty. Directories are processed from deepest
    /// to shallowest so that parent directories which become empty after child
    /// deletion can also be removed.
    ///
    /// ## Return
    /// - `Ok(())`: Successfully removed all empty directories
    /// - `Err(EpubError)`: IO error
    fn remove_empty_dirs(&self) -> Result<(), EpubError> {
        let mut dirs = WalkDir::new(self.temp_dir.as_path())
            .min_depth(1)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_dir())
            .map(|entry| entry.into_path())
            .collect::<Vec<PathBuf>>();

        dirs.sort_by_key(|p| Reverse(p.components().count()));

        for dir in dirs {
            if fs::read_dir(&dir)?.next().is_none() {
                fs::remove_dir(dir)?;
            }
        }

        Ok(())
    }
}

impl<Version> Drop for EpubBuilder<Version> {
    /// Remove temporary directory when dropped
    fn drop(&mut self) {
        if let Err(err) = fs::remove_dir_all(&self.temp_dir) {
            warn!("{}", err);
        };
    }
}

/// Refine the MIME type based on file extension
///
/// This function optimizes MIME types that are inferred from file content by using
/// the file extension to determine the correct EPUB-specific MIME type. Some file
/// types have different MIME types depending on how they are used in an EPUB context.
fn refine_mime_type<'a>(infer_mime: &'a str, extension: &'a str) -> &'a str {
    match (infer_mime, extension) {
        ("text/xml", "xhtml")
        | ("application/xml", "xhtml")
        | ("text/xml", "xht")
        | ("application/xml", "xht") => "application/xhtml+xml",

        ("text/xml", "opf") | ("application/xml", "opf") => "application/oebps-package+xml",

        ("text/xml", "ncx") | ("application/xml", "ncx") => "application/x-dtbncx+xml",

        ("application/zip", "epub") => "application/epub+zip",

        ("text/plain", "css") => "text/css",
        ("text/plain", "js") => "application/javascript",
        ("text/plain", "json") => "application/json",
        ("text/plain", "svg") => "image/svg+xml",

        _ => infer_mime,
    }
}

/// Normalize manifest path to absolute path within EPUB container
///
/// This function takes a path (relative or absolute) and normalizes it to an absolute
/// path within the EPUB container structure. It handles various path formats including:
/// - Relative paths starting with "../" (with security check to prevent directory traversal)
/// - Absolute paths starting with "/" (relative to EPUB root)
/// - Relative paths starting with "./" (current directory)
/// - Plain relative paths (relative to the OPF file location)
///
/// ## Parameters
/// - `temp_dir`: The temporary directory path used during the EPUB build process
/// - `rootfile`: The path to the OPF file (package document), used to determine the base directory
/// - `path`: The input path that may be relative or absolute. Can be any type that
///   implements `AsRef<Path>`, such as `&str`, `String`, `Path`, `PathBuf`, etc.
/// - `id`: The identifier of the manifest item being processed
///
/// ## Return
/// - `Ok(PathBuf)`: The normalized absolute path within the EPUB container,
///   which does not start with "/"
/// - `Err(EpubError)`: Error if path traversal is detected outside the EPUB container,
///   or if the absolute path cannot be determined
fn normalize_manifest_path<TempD: AsRef<Path>, S: AsRef<str>, P: AsRef<Path>>(
    temp_dir: TempD,
    rootfile: S,
    path: P,
    id: &str,
) -> Result<PathBuf, EpubError> {
    let opf_path = PathBuf::from(rootfile.as_ref());
    let basic_path = remove_leading_slash(opf_path.parent().unwrap());

    // convert manifest path to absolute path(physical path)
    let mut target_path = if path.as_ref().starts_with("../") {
        check_realtive_link_leakage(
            temp_dir.as_ref().to_path_buf(),
            basic_path.to_path_buf(),
            &path.as_ref().to_string_lossy(),
        )
        .map(PathBuf::from)
        .ok_or_else(|| EpubError::RelativeLinkLeakage {
            path: path.as_ref().to_string_lossy().to_string(),
        })?
    } else if let Ok(path) = path.as_ref().strip_prefix("/") {
        temp_dir.as_ref().join(path)
    } else if path.as_ref().starts_with("./") {
        // can not anlyze where the 'current' directory is
        Err(EpubBuilderError::IllegalManifestPath { manifest_id: id.to_string() })?
    } else {
        temp_dir.as_ref().join(basic_path).join(path)
    };

    #[cfg(windows)]
    {
        target_path = PathBuf::from(target_path.to_string_lossy().replace('\\', "/"));
    }

    Ok(target_path)
}

#[cfg(test)]
mod tests {
    use std::{env, fs, path::PathBuf};

    use crate::{
        builder::{EpubBuilder, EpubVersion3, normalize_manifest_path, refine_mime_type},
        epub::EpubDoc,
        error::{EpubBuilderError, EpubError},
        types::{ManifestItem, MetadataItem, NavPoint, SpineItem},
        utils::local_time,
    };

    #[test]
    fn test_epub_builder_new() {
        let builder = EpubBuilder::<EpubVersion3>::new();
        assert!(builder.is_ok());

        let builder = builder.unwrap();
        assert!(builder.temp_dir.exists());
        assert!(builder.rootfiles.is_empty());
        assert!(builder.metadata.metadata.is_empty());
        assert!(builder.manifest.manifest.is_empty());
        assert!(builder.spine.spine.is_empty());
        assert!(builder.catalog.title.is_empty());
        assert!(builder.catalog.is_empty());
    }

    #[test]
    fn test_add_rootfile() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
        assert!(builder.add_rootfile("content.opf").is_ok());

        assert_eq!(builder.rootfiles.rootfiles.len(), 1);
        assert_eq!(builder.rootfiles.rootfiles[0], "content.opf");

        assert!(builder.add_rootfile("./another.opf").is_ok());
        assert_eq!(builder.rootfiles.rootfiles.len(), 2);
        assert_eq!(
            builder.rootfiles.rootfiles,
            vec!["content.opf", "another.opf"]
        );
    }

    #[test]
    fn test_add_rootfile_fail() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        let result = builder.add_rootfile("/rootfile.opf");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            EpubBuilderError::IllegalRootfilePath.into()
        );

        let result = builder.add_rootfile("../rootfile.opf");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            EpubBuilderError::IllegalRootfilePath.into()
        );
    }

    #[test]
    fn test_add_metadata() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
        let metadata_item = MetadataItem::new("title", "Test Book");

        builder.add_metadata(metadata_item);

        assert_eq!(builder.metadata.metadata.len(), 1);
        assert_eq!(builder.metadata.metadata[0].property, "title");
        assert_eq!(builder.metadata.metadata[0].value, "Test Book");
    }

    #[test]
    fn test_add_manifest_success() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
        assert!(builder.add_rootfile("content.opf").is_ok());

        // Create a temporary file for testing
        let temp_dir = env::temp_dir().join(local_time());
        fs::create_dir_all(&temp_dir).unwrap();
        let test_file = temp_dir.join("test.xhtml");
        fs::write(&test_file, "<html><body>Hello World</body></html>").unwrap();

        let manifest_item = ManifestItem::new("test", "/epub/test.xhtml").unwrap();
        let result = builder.add_manifest(test_file.to_str().unwrap(), manifest_item);

        assert!(result.is_ok());
        assert_eq!(builder.manifest.manifest.len(), 1);
        assert!(builder.manifest.manifest.contains_key("test"));

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_add_manifest_no_rootfile() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        let manifest_item = ManifestItem {
            id: "main".to_string(),
            path: PathBuf::from("/Overview.xhtml"),
            mime: String::new(),
            properties: None,
            fallback: None,
        };

        let result = builder.add_manifest("./test_case/Overview.xhtml", manifest_item.clone());
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            EpubBuilderError::MissingRootfile.into()
        );

        let result = builder.add_rootfile("package.opf");
        assert!(result.is_ok());

        let result = builder.add_manifest("./test_case/Overview.xhtml", manifest_item);
        assert!(result.is_ok());
    }

    #[test]
    fn test_add_manifest_nonexistent_file() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
        assert!(builder.add_rootfile("content.opf").is_ok());

        let manifest_item = ManifestItem::new("test", "nonexistent.xhtml").unwrap();
        let result = builder.add_manifest("nonexistent.xhtml", manifest_item);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            EpubBuilderError::TargetIsNotFile {
                target_path: "nonexistent.xhtml".to_string()
            }
            .into()
        );
    }

    #[test]
    fn test_add_manifest_unknow_file_format() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
        let result = builder.add_rootfile("package.opf");
        assert!(result.is_ok());

        let result = builder.add_manifest(
            "./test_case/unknown_file_format.xhtml",
            ManifestItem {
                id: "file".to_string(),
                path: PathBuf::from("unknown_file_format.xhtml"),
                mime: String::new(),
                properties: None,
                fallback: None,
            },
        );

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            EpubBuilderError::UnknownFileFormat {
                file_path: "./test_case/unknown_file_format.xhtml".to_string(),
            }
            .into()
        )
    }

    #[test]
    fn test_add_spine() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
        let spine_item = SpineItem::new("test_item");

        builder.add_spine(spine_item.clone());

        assert_eq!(builder.spine.spine.len(), 1);
        assert_eq!(builder.spine.spine[0].idref, "test_item");
    }

    #[test]
    fn test_set_catalog_title() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
        let title = "Test Catalog Title";

        builder.set_catalog_title(title);

        assert_eq!(builder.catalog.title, title);
    }

    #[test]
    fn test_add_catalog_item() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
        let nav_point = NavPoint::new("Chapter 1");

        builder.add_catalog_item(nav_point.clone());

        assert_eq!(builder.catalog.catalog.len(), 1);
        assert_eq!(builder.catalog.catalog[0].label, "Chapter 1");
    }

    #[test]
    fn test_clear_all() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        builder.add_rootfile("content.opf").unwrap();
        builder.add_metadata(MetadataItem::new("title", "Test Book"));
        builder.add_metadata(MetadataItem::new("language", "en"));
        builder.add_spine(SpineItem::new("chapter1"));
        builder.add_spine(SpineItem::new("chapter2"));
        builder.add_catalog_item(NavPoint::new("Chapter 1"));
        builder.add_catalog_item(NavPoint::new("Chapter 2"));
        builder.set_catalog_title("Table of Contents");

        assert_eq!(builder.metadata.metadata.len(), 2);
        assert_eq!(builder.spine.spine.len(), 2);
        assert_eq!(builder.catalog.catalog.len(), 2);
        assert_eq!(builder.catalog.title, "Table of Contents");

        builder.clear_all();

        assert!(builder.metadata.metadata.is_empty());
        assert!(builder.spine.spine.is_empty());
        assert!(builder.catalog.catalog.is_empty());
        assert!(builder.catalog.title.is_empty());
        assert!(builder.manifest.manifest.is_empty());

        builder.add_metadata(MetadataItem::new("title", "New Book"));
        builder.add_spine(SpineItem::new("new_chapter"));
        builder.add_catalog_item(NavPoint::new("New Chapter"));

        assert_eq!(builder.metadata.metadata.len(), 1);
        assert_eq!(builder.spine.spine.len(), 1);
        assert_eq!(builder.catalog.catalog.len(), 1);
    }

    #[test]
    fn test_make_container_file() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        let result = builder.make_container_xml();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            EpubBuilderError::MissingRootfile.into()
        );

        assert!(builder.add_rootfile("content.opf").is_ok());
        let result = builder.make_container_xml();
        assert!(result.is_ok());
    }

    #[test]
    fn test_make_navigation_document() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        let result = builder.make_navigation_document();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            EpubBuilderError::NavigationInfoUninitalized.into()
        );

        builder.add_catalog_item(NavPoint::new("test"));
        assert!(builder.make_navigation_document().is_ok());
    }

    #[test]
    fn test_validate_metadata_success() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        builder.add_metadata(MetadataItem::new("title", "Test Book"));
        builder.add_metadata(MetadataItem::new("language", "en"));
        builder.add_metadata(
            MetadataItem::new("identifier", "urn:isbn:1234567890")
                .with_id("pub-id")
                .build(),
        );

        assert!(builder.metadata.validate().is_ok());
    }

    #[test]
    fn test_validate_metadata_missing_required() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        builder.add_metadata(MetadataItem::new("title", "Test Book"));
        builder.add_metadata(MetadataItem::new("language", "en"));

        assert!(builder.metadata.validate().is_err());
    }

    #[test]
    fn test_validate_fallback_chain_valid() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        let item3 = ManifestItem::new("item3", "path3");
        assert!(item3.is_ok());

        let item3 = item3.unwrap();
        let item2 = ManifestItem::new("item2", "path2")
            .unwrap()
            .with_fallback("item3")
            .build();
        let item1 = ManifestItem::new("item1", "path1")
            .unwrap()
            .with_fallback("item2")
            .append_property("nav")
            .build();

        builder.manifest.insert("item3".to_string(), item3);
        builder.manifest.insert("item2".to_string(), item2);
        builder.manifest.insert("item1".to_string(), item1);

        assert!(builder.manifest.validate().is_ok());
    }

    #[test]
    fn test_validate_fallback_chain_circular_reference() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        let item2 = ManifestItem::new("item2", "path2")
            .unwrap()
            .with_fallback("item1")
            .build();
        let item1 = ManifestItem::new("item1", "path1")
            .unwrap()
            .with_fallback("item2")
            .build();

        builder.manifest.insert("item1".to_string(), item1);
        builder.manifest.insert("item2".to_string(), item2);

        let result = builder.manifest.validate();
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().starts_with(
                "Epub builder error: Circular reference detected in fallback chain for"
            ),
        );
    }

    #[test]
    fn test_validate_fallback_chain_not_found() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        let item1 = ManifestItem::new("item1", "path1")
            .unwrap()
            .with_fallback("nonexistent")
            .build();

        builder.manifest.insert("item1".to_string(), item1);

        let result = builder.manifest.validate();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Epub builder error: Fallback resource 'nonexistent' does not exist in manifest."
        );
    }

    #[test]
    fn test_validate_manifest_nav_single() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        let nav_item = ManifestItem::new("nav", "nav.xhtml")
            .unwrap()
            .append_property("nav")
            .build();
        builder
            .manifest
            .manifest
            .insert("nav".to_string(), nav_item);

        let result = builder.manifest.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_manifest_nav_multiple() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        let nav_item1 = ManifestItem::new("nav1", "nav1.xhtml")
            .unwrap()
            .append_property("nav")
            .build();
        let nav_item2 = ManifestItem::new("nav2", "nav2.xhtml")
            .unwrap()
            .append_property("nav")
            .build();

        builder
            .manifest
            .manifest
            .insert("nav1".to_string(), nav_item1);
        builder
            .manifest
            .manifest
            .insert("nav2".to_string(), nav_item2);

        let result = builder.manifest.validate();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Epub builder error: There are too many items with 'nav' property in the manifest."
        );
    }

    #[test]
    fn test_make_opf_file_success() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        assert!(builder.add_rootfile("content.opf").is_ok());
        builder.add_metadata(MetadataItem::new("title", "Test Book"));
        builder.add_metadata(MetadataItem::new("language", "en"));
        builder.add_metadata(
            MetadataItem::new("identifier", "urn:isbn:1234567890")
                .with_id("pub-id")
                .build(),
        );

        let temp_dir = env::temp_dir().join(local_time());
        fs::create_dir_all(&temp_dir).unwrap();

        let test_file = temp_dir.join("test.xhtml");
        fs::write(&test_file, "<html></html>").unwrap();

        let manifest_result = builder.add_manifest(
            test_file.to_str().unwrap(),
            ManifestItem::new("test", "test.xhtml").unwrap(),
        );
        assert!(manifest_result.is_ok());

        builder.add_catalog_item(NavPoint::new("Chapter"));
        builder.add_spine(SpineItem::new("test"));

        let result = builder.make_navigation_document();
        assert!(result.is_ok());

        let result = builder.make_opf_file();
        assert!(result.is_ok());

        let opf_path = builder.temp_dir.join("content.opf");
        assert!(opf_path.exists());

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_make_opf_file_missing_metadata() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
        assert!(builder.add_rootfile("content.opf").is_ok());

        let result = builder.make_opf_file();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Epub builder error: Requires at least one 'title', 'language', and 'identifier' with id 'pub-id'."
        );
    }

    #[test]
    fn test_make() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        assert!(builder.add_rootfile("content.opf").is_ok());
        builder.add_metadata(MetadataItem::new("title", "Test Book"));
        builder.add_metadata(MetadataItem::new("language", "en"));
        builder.add_metadata(
            MetadataItem::new("identifier", "test_identifier")
                .with_id("pub-id")
                .build(),
        );

        assert!(
            builder
                .add_manifest(
                    "./test_case/Overview.xhtml",
                    ManifestItem {
                        id: "test".to_string(),
                        path: PathBuf::from("test.xhtml"),
                        mime: String::new(),
                        properties: None,
                        fallback: None,
                    },
                )
                .is_ok()
        );

        builder.add_catalog_item(NavPoint::new("Chapter"));
        builder.add_spine(SpineItem::new("test"));

        let file = env::temp_dir()
            .join("temp_dir")
            .join(format!("{}.epub", local_time()));
        assert!(builder.make(&file).is_ok());
        assert!(EpubDoc::new(&file).is_ok());
    }

    #[test]
    fn test_build() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        assert!(builder.add_rootfile("content.opf").is_ok());
        builder.add_metadata(MetadataItem::new("title", "Test Book"));
        builder.add_metadata(MetadataItem::new("language", "en"));
        builder.add_metadata(
            MetadataItem::new("identifier", "test_identifier")
                .with_id("pub-id")
                .build(),
        );

        assert!(
            builder
                .add_manifest(
                    "./test_case/Overview.xhtml",
                    ManifestItem {
                        id: "test".to_string(),
                        path: PathBuf::from("test.xhtml"),
                        mime: String::new(),
                        properties: None,
                        fallback: None,
                    },
                )
                .is_ok()
        );

        builder.add_catalog_item(NavPoint::new("Chapter"));
        builder.add_spine(SpineItem::new("test"));

        let file = env::temp_dir().join(format!("{}.epub", local_time()));
        assert!(builder.build(&file).is_ok());
    }

    #[test]
    fn test_from() {
        let builder = EpubBuilder::<EpubVersion3>::new();
        assert!(builder.is_ok());

        let metadata = vec![
            MetadataItem {
                id: None,
                property: "title".to_string(),
                value: "Test Book".to_string(),
                lang: None,
                refined: vec![],
            },
            MetadataItem {
                id: None,
                property: "language".to_string(),
                value: "en".to_string(),
                lang: None,
                refined: vec![],
            },
            MetadataItem {
                id: Some("pub-id".to_string()),
                property: "identifier".to_string(),
                value: "test-book".to_string(),
                lang: None,
                refined: vec![],
            },
        ];
        let spine = vec![SpineItem {
            id: None,
            idref: "main".to_string(),
            linear: true,
            properties: None,
        }];
        let catalog = vec![
            NavPoint {
                label: "Nav".to_string(),
                content: None,
                children: vec![],
                play_order: None,
            },
            NavPoint {
                label: "Overview".to_string(),
                content: None,
                children: vec![],
                play_order: None,
            },
        ];

        let mut builder = builder.unwrap();
        assert!(builder.add_rootfile("content.opf").is_ok());
        builder.metadata.metadata = metadata.clone();
        builder.spine.spine = spine.clone();
        builder.catalog.catalog = catalog.clone();
        builder.set_catalog_title("catalog title");
        let result = builder.add_manifest(
            "./test_case/Overview.xhtml",
            ManifestItem {
                id: "main".to_string(),
                path: PathBuf::from("Overview.xhtml"),
                mime: String::new(),
                properties: None,
                fallback: None,
            },
        );
        assert!(result.is_ok());

        let epub_file = env::temp_dir().join(format!("{}.epub", local_time()));
        let result = builder.make(&epub_file);
        assert!(result.is_ok());

        let doc = EpubDoc::new(&epub_file);
        assert!(doc.is_ok());

        let mut doc = doc.unwrap();
        let builder = EpubBuilder::from(&mut doc);
        assert!(builder.is_ok());
        let builder = builder.unwrap();

        assert_eq!(builder.metadata.metadata.len(), metadata.len() + 1);
        assert_eq!(builder.manifest.manifest.len(), 1); // skip nav file
        assert_eq!(builder.spine.spine.len(), spine.len());
        assert_eq!(builder.catalog.catalog, catalog);
        assert_eq!(builder.catalog.title, "catalog title");

        fs::remove_file(epub_file).unwrap();
    }

    #[test]
    fn test_normalize_manifest_path() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        assert!(builder.add_rootfile("content.opf").is_ok());

        let result = normalize_manifest_path(
            &builder.temp_dir,
            &builder.rootfiles.first().expect("Unreachable"),
            "../../test.xhtml",
            "id",
        );
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            EpubError::RelativeLinkLeakage { path: "../../test.xhtml".to_string() }
        );

        let result = normalize_manifest_path(
            &builder.temp_dir,
            &builder.rootfiles.first().expect("Unreachable"),
            "/test.xhtml",
            "id",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), builder.temp_dir.join("test.xhtml"));

        let result = normalize_manifest_path(
            &builder.temp_dir,
            &builder.rootfiles.first().expect("Unreachable"),
            "./test.xhtml",
            "manifest_id",
        );
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            EpubBuilderError::IllegalManifestPath { manifest_id: "manifest_id".to_string() }.into(),
        );
    }

    #[test]
    fn test_refine_mime_type() {
        assert_eq!(
            refine_mime_type("text/xml", "xhtml"),
            "application/xhtml+xml"
        );
        assert_eq!(refine_mime_type("text/xml", "xht"), "application/xhtml+xml");
        assert_eq!(
            refine_mime_type("application/xml", "opf"),
            "application/oebps-package+xml"
        );
        assert_eq!(
            refine_mime_type("text/xml", "ncx"),
            "application/x-dtbncx+xml"
        );
        assert_eq!(refine_mime_type("text/plain", "css"), "text/css");
        assert_eq!(refine_mime_type("text/plain", "unknown"), "text/plain");
    }

    #[cfg(feature = "content-builder")]
    mod make_contents_tests {
        use std::path::PathBuf;

        use crate::builder::{EpubBuilder, EpubVersion3, content::ContentBuilder};

        #[test]
        fn test_make_contents_basic() {
            let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
            builder.add_rootfile("content.opf").unwrap();

            let mut content_builder = ContentBuilder::new("chapter1", "en").unwrap();
            content_builder
                .set_title("Test Chapter")
                .add_text_block("This is a test paragraph.", vec![])
                .unwrap();

            builder.add_content("OEBPS/chapter1.xhtml", content_builder);

            let result = builder.make_contents();
            assert!(result.is_ok());
            assert!(builder.temp_dir.join("OEBPS/chapter1.xhtml").exists());
        }

        #[test]
        fn test_make_contents_multiple_blocks() {
            let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
            builder.add_rootfile("content.opf").unwrap();

            let mut content_builder = ContentBuilder::new("chapter2", "zh-CN").unwrap();
            content_builder
                .set_title("多个区块章节")
                .add_text_block("第一段文本。", vec![])
                .unwrap()
                .add_quote_block("这是一个引用。", vec![])
                .unwrap()
                .add_title_block("子标题", 2, vec![])
                .unwrap()
                .add_text_block("最后的文本段落。", vec![])
                .unwrap();

            builder.add_content("OEBPS/chapter2.xhtml", content_builder);

            let result = builder.make_contents();
            assert!(result.is_ok());
            assert!(builder.temp_dir.join("OEBPS/chapter2.xhtml").exists());
        }

        #[test]
        fn test_make_contents_with_media() {
            let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
            builder.add_rootfile("content.opf").unwrap();

            let img_path = PathBuf::from("./test_case/image.jpg");

            let mut content_builder = ContentBuilder::new("chapter3", "en").unwrap();
            content_builder
                .set_title("Chapter with Media")
                .add_text_block("Text before image.", vec![])
                .unwrap()
                .add_image_block(
                    img_path,
                    Some("Test Image".to_string()),
                    Some("Figure 1: A test image".to_string()),
                    vec![],
                )
                .unwrap()
                .add_text_block("Text after image.", vec![])
                .unwrap();

            builder.add_content("OEBPS/chapter3.xhtml", content_builder);

            let result = builder.make_contents();
            assert!(result.is_ok());
            assert!(builder.temp_dir.join("OEBPS/chapter3.xhtml").exists());
            assert!(builder.temp_dir.join("OEBPS/img").exists());
            assert!(builder.temp_dir.join("OEBPS/img/image.jpg").exists());
        }

        #[test]
        fn test_make_contents_multiple_documents() {
            let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
            builder.add_rootfile("content.opf").unwrap();

            let mut content = ContentBuilder::new("ch1", "en").unwrap();
            content
                .set_title("Chapter 1")
                .add_text_block("Content of chapter 1", vec![])
                .unwrap();
            builder.add_content("OEBPS/chapter1.xhtml", content);

            let mut content = ContentBuilder::new("ch2", "en").unwrap();
            content
                .set_title("Chapter 2")
                .add_text_block("Content of chapter 2", vec![])
                .unwrap();
            builder.add_content("OEBPS/chapter2.xhtml", content);

            let mut content = ContentBuilder::new("ch3", "en").unwrap();
            content
                .set_title("Chapter 3")
                .add_text_block("Content of chapter 3", vec![])
                .unwrap();
            builder.add_content("OEBPS/chapter3.xhtml", content);

            let result = builder.make_contents();
            assert!(result.is_ok());
            assert!(builder.temp_dir.join("OEBPS/chapter1.xhtml").exists());
            assert!(builder.temp_dir.join("OEBPS/chapter2.xhtml").exists());
            assert!(builder.temp_dir.join("OEBPS/chapter3.xhtml").exists());
        }

        #[test]
        fn test_make_contents_different_languages() {
            let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
            builder.add_rootfile("content.opf").unwrap();

            let mut content = ContentBuilder::new("en_ch", "en").unwrap();
            content
                .set_title("English Chapter")
                .add_text_block("English text.", vec![])
                .unwrap();
            builder.add_content("OEBPS/en_chapter.xhtml", content);

            let mut content = ContentBuilder::new("zh_ch", "zh-CN").unwrap();
            content
                .set_title("中文章节")
                .add_text_block("中文文本。", vec![])
                .unwrap();
            builder.add_content("OEBPS/zh_chapter.xhtml", content);

            let mut content = ContentBuilder::new("ja_ch", "ja").unwrap();
            content
                .set_title("日本語の章")
                .add_text_block("日本語のテキスト。", vec![])
                .unwrap();
            builder.add_content("OEBPS/ja_chapter.xhtml", content);

            let result = builder.make_contents();
            assert!(result.is_ok());
            assert!(builder.temp_dir.join("OEBPS/en_chapter.xhtml").exists());
            assert!(builder.temp_dir.join("OEBPS/zh_chapter.xhtml").exists());
            assert!(builder.temp_dir.join("OEBPS/ja_chapter.xhtml").exists());
        }

        #[test]
        fn test_make_contents_unique_identifiers() {
            let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
            builder.add_rootfile("content.opf").unwrap();

            let mut content = ContentBuilder::new("unique_id_1", "en").unwrap();
            content.add_text_block("First content", vec![]).unwrap();
            builder.add_content("OEBPS/ch1.xhtml", content);

            let mut content = ContentBuilder::new("unique_id_2", "en").unwrap();
            content.add_text_block("Second content", vec![]).unwrap();
            builder.add_content("OEBPS/ch2.xhtml", content);

            let mut content = ContentBuilder::new("unique_id_1", "en").unwrap();
            content
                .add_text_block("Duplicate ID content", vec![])
                .unwrap();
            builder.add_content("OEBPS/ch3.xhtml", content);

            let result = builder.make_contents();
            assert!(result.is_ok());
            assert!(builder.temp_dir.join("OEBPS/ch1.xhtml").exists()); // recovered by ch3
            assert!(builder.temp_dir.join("OEBPS/ch2.xhtml").exists());
            assert!(builder.temp_dir.join("OEBPS/ch3.xhtml").exists());

            let manifest = builder.manifest.manifest.get("unique_id_1");
            assert!(manifest.is_some());

            let manifest = manifest.unwrap();
            assert_eq!(manifest.path, PathBuf::from("/OEBPS/ch3.xhtml"));
        }

        #[test]
        fn test_make_contents_complex_structure() {
            let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
            builder.add_rootfile("content.opf").unwrap();

            let mut content = ContentBuilder::new("complex_ch", "en").unwrap();
            content
                .set_title("Complex Chapter")
                .add_title_block("Section 1", 2, vec![])
                .unwrap()
                .add_text_block("Introduction text.", vec![])
                .unwrap()
                .add_quote_block("A wise quote here.", vec![])
                .unwrap()
                .add_title_block("Section 2", 2, vec![])
                .unwrap()
                .add_text_block("More content with multiple paragraphs.", vec![])
                .unwrap()
                .add_text_block("Another paragraph.", vec![])
                .unwrap()
                .add_title_block("Section 3", 2, vec![])
                .unwrap()
                .add_quote_block("Another quotation.", vec![])
                .unwrap();

            builder.add_content("OEBPS/complex_chapter.xhtml", content);

            let result = builder.make_contents();
            assert!(result.is_ok());
            assert!(
                builder
                    .temp_dir
                    .join("OEBPS/complex_chapter.xhtml")
                    .exists()
            );
        }

        #[test]
        fn test_make_contents_empty_document() {
            let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
            builder.add_rootfile("content.opf").unwrap();

            let content = ContentBuilder::new("empty_ch", "en").unwrap();
            builder.add_content("OEBPS/empty.xhtml", content);

            let result = builder.make_contents();
            assert!(result.is_ok());
            assert!(builder.temp_dir.join("OEBPS/empty.xhtml").exists());
        }

        #[test]
        fn test_make_contents_path_normalization() {
            let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
            builder.add_rootfile("OEBPS/content.opf").unwrap();

            let mut content = ContentBuilder::new("path_test", "en").unwrap();
            content.add_text_block("Path test content", vec![]).unwrap();

            builder.add_content("/OEBPS/text/chapter.xhtml", content);

            let result = builder.make_contents();
            assert!(result.is_ok());
            assert!(builder.temp_dir.join("OEBPS/text/chapter.xhtml").exists());
        }
    }
}
