//! Epub Builder
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
//!     .add_rootfile("OEBPS/content.opf")
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
//! - Files will be manipulated in a temporary directory during the build process;
//!   automatic cleanup will occur upon completion
//! - All resource files must exist on the local file system.

use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::{BufReader, Cursor, Read, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
};

use chrono::{SecondsFormat, Utc};
use infer::Infer;
use log::warn;
use quick_xml::{
    Writer,
    events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event},
};
use walkdir::WalkDir;
use zip::{CompressionMethod, ZipWriter, write::FileOptions};

use crate::{
    epub::EpubDoc,
    error::{EpubBuilderError, EpubError},
    types::{ManifestItem, MetadataItem, NavPoint, SpineItem},
    utils::{ELEMENT_IN_DC_NAMESPACE, local_time},
};

type XmlWriter = Writer<Cursor<Vec<u8>>>;

// struct EpubVersion2;
pub struct EpubVersion3;

/// EPUB Builder
///
/// The main structure used to create and build EPUB ebook files.
/// Supports the EPUB 3.0 specification and can build a complete EPUB file structure.
pub struct EpubBuilder<Version> {
    /// EPUB version placeholder
    epub_version: PhantomData<Version>,

    /// Temporary directory path for storing files during the build process
    temp_dir: PathBuf,

    /// List of root file paths
    rootfiles: Vec<String>,

    /// List of metadata items
    metadata: Vec<MetadataItem>,

    /// Manifest item mapping table, with ID as the key and manifest item as the value
    manifest: HashMap<String, ManifestItem>,

    /// List of spine items, defining the reading order
    spine: Vec<SpineItem>,

    catalog_title: String,

    /// List of catalog navigation points
    catalog: Vec<NavPoint>,
}

impl EpubBuilder<EpubVersion3> {
    /// Create a new `EpubBuilder` instance
    ///
    /// # Return
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
            temp_dir,

            rootfiles: vec![],
            metadata: vec![],
            manifest: HashMap::new(),
            spine: vec![],

            catalog_title: String::new(),
            catalog: vec![],
        })
    }

    /// Add a rootfile path
    ///
    /// The added path points to an OPF file that does not yet exist
    /// and will be created when building the Epub file.
    ///
    /// # Parameters
    /// - `rootfile`: Rootfile path
    pub fn add_rootfile(&mut self, rootfile: &str) -> &mut Self {
        self.rootfiles.push(rootfile.to_string());

        self
    }

    /// Add metadata item
    ///
    /// Required metadata includes title, language, and an identifier with 'pub-id'.
    /// Missing this data will result in an error when building the epub file.
    ///
    /// # Parameters
    /// - `item`: Metadata items to add
    pub fn add_metadata(&mut self, item: MetadataItem) -> &mut Self {
        self.metadata.push(item);
        self
    }

    /// Add manifest item and corresponding resource file
    ///
    /// The builder will automatically recognize the file type of
    /// the added resource and update it in `ManifestItem`.
    ///
    /// # Parameters
    /// - `manifest_source` - Local resource file path
    /// - `manifest_item` - Manifest item information
    ///
    /// # Return
    /// - `Ok(&mut Self)` - Successful addition, returns a reference to itself
    /// - `Err(EpubError)` - Error occurred during the addition process
    pub fn add_manifest(
        &mut self,
        manifest_source: &str,
        manifest_item: ManifestItem,
    ) -> Result<&mut Self, EpubError> {
        let source = PathBuf::from(manifest_source);
        if !source.is_file() {
            return Err(EpubBuilderError::TargetIsNotFile {
                target_path: manifest_source.to_string(),
            }
            .into());
        }

        let extension = match source.extension() {
            Some(ext) => ext.to_string_lossy().to_lowercase(),
            None => String::new(),
        };

        let buf = match fs::read(source) {
            Ok(buf) => buf,
            Err(err) => return Err(err.into()),
        };

        let real_mime = match Infer::new().get(&buf) {
            Some(infer_mime) => refine_mime_type(infer_mime.mime_type(), &extension),
            None => {
                return Err(EpubBuilderError::UnknowFileFormat {
                    file_path: manifest_source.to_string(),
                }
                .into());
            }
        };

        let target_path = self.temp_dir.join(&manifest_item.path);
        if let Some(parent_dir) = target_path.parent() {
            if !parent_dir.exists() {
                fs::create_dir_all(parent_dir)?
            }
        }

        match fs::write(target_path, buf) {
            Ok(_) => {
                self.manifest
                    .insert(manifest_item.id.clone(), manifest_item.set_mime(&real_mime));
                Ok(self)
            }
            Err(err) => Err(err.into()),
        }
    }

    /// Add spine item
    ///
    /// The spine item defines the reading order of the book.
    ///
    /// # Parameters
    /// - `item`: Spine item to add
    pub fn add_spine(&mut self, item: SpineItem) -> &mut Self {
        self.spine.push(item);
        self
    }

    /// Set catalog title
    ///
    /// # Parameters
    /// - `title`: Catalog title
    pub fn set_catalog_title(&mut self, title: &str) -> &mut Self {
        self.catalog_title = title.to_string();
        self
    }

    /// Add catalog item
    ///
    /// Added directory items will be added to the end of the existing list.
    ///
    /// # Parameters
    /// - `item`: Catalog item to add
    pub fn add_catalog_item(&mut self, item: NavPoint) -> &mut Self {
        self.catalog.push(item);
        self
    }

    /// Re-/ Set catalog
    ///
    /// The passed list will overwrite existing data.
    ///
    /// # Parameters
    /// - `catalog`: Catalog to set
    pub fn set_catalog(&mut self, catalog: Vec<NavPoint>) -> &mut Self {
        self.catalog = catalog;
        self
    }

    /// Builds an EPUB file and saves it to the specified path
    ///
    /// # Parameters
    /// - `output_path`: Output file path
    ///
    /// # Return
    /// - `Ok(())`: Build successful
    /// - `Err(EpubError)`: Error occurred during the build process
    pub fn make<P: AsRef<Path>>(mut self, output_path: P) -> Result<(), EpubError> {
        // Create the container.xml, navigation document, and OPF files in sequence.
        // The associated metadata will initialized when navigation document is created;
        // therefore, the navigation document must be created before the opf file is created.
        self.make_container_xml()?;
        self.make_navigation_document()?;
        self.make_opf_file()?;

        if let Some(parent) = output_path.as_ref().parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        let file = File::create(output_path)?;
        let mut zip = ZipWriter::new(file);
        let options = FileOptions::<()>::default().compression_method(CompressionMethod::Stored);

        for entry in WalkDir::new(&self.temp_dir) {
            let entry = entry.map_err(|_e| EpubError::FailedParsingXml)?;
            let path = entry.path();

            let relative_path = path
                .strip_prefix(&self.temp_dir)
                .map_err(|_e| EpubError::FailedParsingXml)?;
            let target_path = relative_path.to_string_lossy().replace("\\", "/");

            if path.is_file() {
                zip.start_file(target_path, options)?;
                let mut buf = Vec::new();
                File::open(path)?.read_to_end(&mut buf)?;
                zip.write(&buf)?;
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
    /// # Parameters
    /// - `output_path`: Output file path
    ///
    /// # Return
    /// - `Ok(EpubDoc)`: Build successful
    /// - `Err(EpubError)`: Error occurred during the build process
    pub fn build<P: AsRef<Path>>(
        self,
        output_path: P,
    ) -> Result<EpubDoc<BufReader<File>>, EpubError> {
        self.make(&output_path)?;

        EpubDoc::new(output_path)
    }

    /// Creates the `container.xml` file
    ///
    /// An error will occur if the `rootfile` path is not set
    fn make_container_xml(&self) -> Result<(), EpubError> {
        if self.rootfiles.is_empty() {
            return Err(EpubBuilderError::MissingRootfile.into());
        }

        let mut writer = Writer::new(Cursor::new(Vec::new()));

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

        let file_path = self.temp_dir.join("META-INF").join("container.xml");
        let file_data = writer.into_inner().into_inner();
        fs::write(file_path, file_data)?;

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

        writer.write_event(Event::Start(BytesStart::new("html").with_attributes([
            ("xmlns", "http://www.w3.org/1999/xhtml"),
            ("xmlns:epub", "http://www.idpf.org/2007/ops"),
        ])))?;

        // make head
        writer.write_event(Event::Start(BytesStart::new("head")))?;
        writer.write_event(Event::Start(BytesStart::new("title")))?;
        writer.write_event(Event::Text(BytesText::new(&self.catalog_title)))?;
        writer.write_event(Event::End(BytesEnd::new("title")))?;
        writer.write_event(Event::End(BytesEnd::new("head")))?;

        // make body
        writer.write_event(Event::Start(BytesStart::new("body")))?;
        writer.write_event(Event::Start(
            BytesStart::new("nav").with_attributes([("epub:type", "toc")]),
        ))?;

        if !self.catalog_title.is_empty() {
            writer.write_event(Event::Start(BytesStart::new("h1")))?;
            writer.write_event(Event::Text(BytesText::new(&self.catalog_title)))?;
            writer.write_event(Event::End(BytesEnd::new("h1")))?;
        }

        Self::make_nav(&mut writer, &self.catalog)?;

        writer.write_event(Event::End(BytesEnd::new("nav")))?;
        writer.write_event(Event::End(BytesEnd::new("body")))?;

        writer.write_event(Event::End(BytesEnd::new("html")))?;

        let file_path = self.temp_dir.join("nav.xhtml");
        let file_data = writer.into_inner().into_inner();
        fs::write(file_path, file_data)?;

        self.manifest.insert(
            "nav".to_string(),
            ManifestItem {
                id: "nav".to_string(),
                path: PathBuf::from("nav.xhtml"),
                mime: "application/xhtml+xml".to_string(),
                properties: Some("nav".to_string()),
                fallback: None,
            },
        );

        Ok(())
    }

    /// Creates the `OPF` file
    ///
    /// # Error conditions
    /// - Missing necessary metadata
    /// - Circular reference exists in the manifest backlink
    /// - Navigation information is not initialized
    fn make_opf_file(&mut self) -> Result<(), EpubError> {
        if !self.validate_metadata() {
            return Err(EpubBuilderError::MissingNecessaryMetadata.into());
        }
        self.validate_manifest_fallback_chains()?;
        self.validate_manifest_nav()?;

        let mut writer = Writer::new(Cursor::new(Vec::new()));

        writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;

        writer.write_event(Event::Start(BytesStart::new("package").with_attributes([
            ("xmlns", "http://www.idpf.org/2007/opf"),
            ("xmlns:dc", "http://purl.org/dc/elements/1.1/"),
            ("unique-identifier", "pub-id"),
            ("version", "3.0"),
        ])))?;

        self.make_opf_metadata(&mut writer)?;
        self.make_opf_manifest(&mut writer)?;
        self.make_opf_spine(&mut writer)?;

        writer.write_event(Event::End(BytesEnd::new("package")))?;

        let file_path = self.temp_dir.join(&self.rootfiles[0]);
        let file_data = writer.into_inner().into_inner();
        fs::write(file_path, file_data)?;

        Ok(())
    }

    fn make_opf_metadata(&mut self, writer: &mut XmlWriter) -> Result<(), EpubError> {
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

    fn make_opf_manifest(&self, writer: &mut XmlWriter) -> Result<(), EpubError> {
        writer.write_event(Event::Start(BytesStart::new("manifest")))?;

        for (_, manifest) in &self.manifest {
            writer.write_event(Event::Empty(
                BytesStart::new("item").with_attributes(manifest.attributes()),
            ))?;
        }

        writer.write_event(Event::End(BytesEnd::new("manifest")))?;

        Ok(())
    }

    fn make_opf_spine(&self, writer: &mut XmlWriter) -> Result<(), EpubError> {
        writer.write_event(Event::Start(BytesStart::new("spine")))?;

        for spine in &self.spine {
            writer.write_event(Event::Empty(
                BytesStart::new("itemref").with_attributes(spine.attributes()),
            ))?;
        }

        writer.write_event(Event::End(BytesEnd::new("spine")))?;

        Ok(())
    }

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

    /// Verify metadata integrity
    ///
    /// Check if the required metadata items are included: title, language, and identifier with pub-id.
    fn validate_metadata(&self) -> bool {
        let has_title = self.metadata.iter().any(|item| item.property == "title");
        let has_language = self.metadata.iter().any(|item| item.property == "language");
        let has_identifier = self.metadata.iter().any(|item| {
            item.property == "identifier" && item.id.as_ref().is_some_and(|id| id == "pub-id")
        });

        has_title && has_identifier && has_language
    }

    fn validate_manifest_fallback_chains(&self) -> Result<(), EpubError> {
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
    fn validate_manifest_nav(&self) -> Result<(), EpubError> {
        if self
            .manifest
            .values()
            .filter(|&item| {
                if let Some(properties) = &item.properties {
                    properties
                        .clone()
                        .split(" ")
                        .collect::<Vec<&str>>()
                        .contains(&"nav")
                } else {
                    return false;
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

impl<Version> Drop for EpubBuilder<Version> {
    /// Remove temporary directory when dropped
    fn drop(&mut self) {
        if let Err(err) = fs::remove_dir_all(&self.temp_dir) {
            warn!("{}", err);
        };
    }
}

/// Refine the mime type
///
/// Optimize mime types inferred from file content based on file extensions
fn refine_mime_type(infer_mime: &str, extension: &str) -> String {
    match (infer_mime, extension) {
        ("text/xml", "xhtml")
        | ("application/xml", "xhtml")
        | ("text/xml", "xht")
        | ("application/xml", "xht") => "application/xhtml+xml".to_string(),

        ("text/xml", "opf") | ("application/xml", "opf") => {
            "application/oebps-package+xml".to_string()
        }

        ("text/xml", "ncx") | ("application/xml", "ncx") => "application/x-dtbncx+xml".to_string(),

        ("application/zip", "epub") => "application/epub+zip".to_string(),

        ("text/plain", "css") => "text/css".to_string(),
        ("text/plain", "js") => "application/javascript".to_string(),
        ("text/plain", "json") => "application/json".to_string(),
        ("text/plain", "svg") => "image/svg+xml".to_string(),

        _ => infer_mime.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use crate::{
        builder::{EpubBuilder, EpubVersion3, refine_mime_type},
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
        assert!(builder.metadata.is_empty());
        assert!(builder.manifest.is_empty());
        assert!(builder.spine.is_empty());
        assert!(builder.catalog_title.is_empty());
        assert!(builder.catalog.is_empty());
    }

    #[test]
    fn test_add_rootfile() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
        builder.add_rootfile("content.opf");

        assert_eq!(builder.rootfiles.len(), 1);
        assert_eq!(builder.rootfiles[0], "content.opf");

        // Test chaining
        builder
            .add_rootfile("another.opf")
            .add_rootfile("third.opf");
        assert_eq!(builder.rootfiles.len(), 3);
        assert_eq!(
            builder.rootfiles,
            vec!["content.opf", "another.opf", "third.opf"]
        );
    }

    #[test]
    fn test_add_metadata() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
        let metadata_item = MetadataItem::new("title", "Test Book");

        builder.add_metadata(metadata_item);

        assert_eq!(builder.metadata.len(), 1);
        assert_eq!(builder.metadata[0].property, "title");
        assert_eq!(builder.metadata[0].value, "Test Book");
    }

    #[test]
    fn test_add_manifest_success() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        // Create a temporary file for testing
        let temp_dir = env::temp_dir().join(local_time());
        fs::create_dir_all(&temp_dir).unwrap();
        let test_file = temp_dir.join("test.xhtml");
        fs::write(&test_file, "<html><body>Hello World</body></html>").unwrap();

        let manifest_item = ManifestItem::new("test", "test.xhtml").unwrap();
        let result = builder.add_manifest(test_file.to_str().unwrap(), manifest_item);

        assert!(result.is_ok());
        assert_eq!(builder.manifest.len(), 1);
        assert!(builder.manifest.contains_key("test"));

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_add_manifest_nonexistent_file() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        let manifest_item = ManifestItem::new("test", "nonexistent.xhtml").unwrap();
        let result = builder.add_manifest("nonexistent.xhtml", manifest_item);

        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(
                err.to_string(),
                "Epub builder error: Expect a file, but 'nonexistent.xhtml' is not a file."
            );
        }
    }

    #[test]
    fn test_add_spine() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
        let spine_item = SpineItem::new("test_item");

        builder.add_spine(spine_item.clone());

        assert_eq!(builder.spine.len(), 1);
        assert_eq!(builder.spine[0].idref, "test_item");
    }

    #[test]
    fn test_set_catalog_title() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
        let title = "Test Catalog Title";

        builder.set_catalog_title(title);

        assert_eq!(builder.catalog_title, title);
    }

    #[test]
    fn test_add_catalog_item() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
        let nav_point = NavPoint::new("Chapter 1");

        builder.add_catalog_item(nav_point.clone());

        assert_eq!(builder.catalog.len(), 1);
        assert_eq!(builder.catalog[0].label, "Chapter 1");
    }

    #[test]
    fn test_set_catalog() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();
        let nav_points = vec![NavPoint::new("Chapter 1"), NavPoint::new("Chapter 2")];

        builder.set_catalog(nav_points.clone());

        assert_eq!(builder.catalog.len(), 2);
        assert_eq!(builder.catalog[0].label, "Chapter 1");
        assert_eq!(builder.catalog[1].label, "Chapter 2");
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

        assert!(builder.validate_metadata());
    }

    #[test]
    fn test_validate_metadata_missing_required() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        builder.add_metadata(MetadataItem::new("title", "Test Book"));
        builder.add_metadata(MetadataItem::new("language", "en"));

        assert!(!builder.validate_metadata());
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
            .build();

        builder.manifest.insert("item3".to_string(), item3);
        builder.manifest.insert("item2".to_string(), item2);
        builder.manifest.insert("item1".to_string(), item1);

        let result = builder.validate_manifest_fallback_chains();
        assert!(result.is_ok());
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

        let result = builder.validate_manifest_fallback_chains();
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

        let result = builder.validate_manifest_fallback_chains();
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
        builder.manifest.insert("nav".to_string(), nav_item);

        let result = builder.validate_manifest_nav();
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

        builder.manifest.insert("nav1".to_string(), nav_item1);
        builder.manifest.insert("nav2".to_string(), nav_item2);

        let result = builder.validate_manifest_nav();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Epub builder error: There are too many items with 'nav' property in the manifest."
        );
    }

    #[test]
    fn test_make_opf_file_success() {
        let mut builder = EpubBuilder::<EpubVersion3>::new().unwrap();

        builder.add_rootfile("content.opf");
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
        builder.add_rootfile("content.opf");

        let result = builder.make_opf_file();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Epub builder error: Requires at least one 'title', 'language', and 'identifier' with id 'pub-id'."
        );
    }

    #[test]
    fn test_refine_mime_type() {
        assert_eq!(
            refine_mime_type("text/xml", "xhtml"),
            "application/xhtml+xml"
        );
        assert_eq!(
            refine_mime_type("application/xml", "opf"),
            "application/oebps-package+xml"
        );
        assert_eq!(refine_mime_type("text/plain", "css"), "text/css");
        assert_eq!(refine_mime_type("text/plain", "unknown"), "text/plain");
    }
}
