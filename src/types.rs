//! Types and data structures for EPUB processing
//!
//! This module defines all the core data structures used throughout the EPUB library.
//! These structures represent the various components of an EPUB publication according to
//! the EPUB specification, including metadata, manifest items, spine items, navigation points,
//! and encryption information.
//!
//! The types in this module are designed to be compatible with both EPUB 2 and EPUB 3
//! specifications, providing a unified interface for working with different versions
//! of EPUB publications.
//!
//! ## Main Components
//!
//! - [MetadataItem] - Represents metadata entries in the publication
//! - [MetadataRefinement] - Additional details for metadata items (EPUB 3.x)
//! - [MetadataLinkItem] - Links to external metadata resources
//! - [ManifestItem] - Resources declared in the publication manifest
//! - [SpineItem] - Items defining the reading order
//! - [NavPoint] - Navigation points in the table of contents
//! - [EncryptionData] - Information about encrypted resources
//!
//! ## Builder Pattern
//!
//! Many of these types implement a builder pattern for easier construction when the
//! `builder` feature is enabled. See individual type documentation for details.

use std::path::PathBuf;

#[cfg(feature = "builder")]
use crate::{
    error::{EpubBuilderError, EpubError},
    utils::ELEMENT_IN_DC_NAMESPACE,
};

/// Represents the EPUB version
///
/// This enum is used to distinguish between different versions of the EPUB specification.
#[derive(Debug, PartialEq, Eq)]
pub enum EpubVersion {
    Version2_0,
    Version3_0,
}

/// Represents a metadata item in the EPUB publication
///
/// The `MetadataItem` structure represents a single piece of metadata from the EPUB publication.
/// Metadata items contain information about the publication such as title, author, identifier,
/// language, and other descriptive information.
///
/// In EPUB 3.0, metadata items can have refinements that provide additional details about
/// the main metadata item. For example, a title metadata item might have refinements that
/// specify it is the main title of the publication.
///
/// # Builder Methods
///
/// When the `builder` feature is enabled, this struct provides convenient builder methods:
///
/// ```rust
/// # #[cfg(feature = "builder")] {
/// use lib_epub::types::MetadataItem;
///
/// let metadata = MetadataItem::new("title", "Sample Book")
///     .with_id("title-1")
///     .with_lang("en")
///     .build();
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct MetadataItem {
    /// Optional unique identifier for this metadata item
    ///
    /// Used to reference this metadata item from other elements or refinements.
    /// In EPUB 3.0, this ID is particularly important for linking with metadata refinements.
    pub id: Option<String>,

    /// The metadata property name
    ///
    /// This field specifies the type of metadata this item represents. Common properties
    /// include "title", "creator", "identifier", "language", "publisher", etc.
    /// These typically correspond to Dublin Core metadata terms.
    pub property: String,

    /// The metadata value
    pub value: String,

    /// Optional language code for this metadata item
    pub lang: Option<String>,

    /// Refinements of this metadata item
    ///
    /// In EPUB 3.x, metadata items can have associated refinements that provide additional
    /// information about the main metadata item. For example, a creator metadata item might
    /// have refinements specifying the creator's role (author, illustrator, etc.) or file-as.
    ///
    /// In EPUB 2.x, metadata items may contain custom attributes, which will also be parsed as refinement.
    pub refined: Vec<MetadataRefinement>,
}

#[cfg(feature = "builder")]
impl MetadataItem {
    /// Creates a new metadata item with the given property and value
    ///
    /// Requires the `builder` feature.
    ///
    /// # Parameters
    /// - `property` - The metadata property name (e.g., "title", "creator")
    /// - `value` - The metadata value
    pub fn new(property: &str, value: &str) -> Self {
        Self {
            id: None,
            property: property.to_string(),
            value: value.to_string(),
            lang: None,
            refined: vec![],
        }
    }

    /// Sets the ID of the metadata item
    ///
    /// Requires the `builder` feature.
    ///
    /// # Parameters
    /// - `id` - The ID to assign to this metadata item
    pub fn with_id(&mut self, id: &str) -> &mut Self {
        self.id = Some(id.to_string());
        self
    }

    /// Sets the language of the metadata item
    ///
    /// Requires the `builder` feature.
    ///
    /// # Parameters
    /// - `lang` - The language code (e.g., "en", "fr", "zh-CN")
    pub fn with_lang(&mut self, lang: &str) -> &mut Self {
        self.lang = Some(lang.to_string());
        self
    }

    /// Adds a refinement to this metadata item
    ///
    /// Requires the `builder` feature.
    ///
    /// # Parameters
    /// - `refine` - The refinement to add
    ///
    /// # Notes
    /// - The metadata item must have an ID for refinements to be added.
    pub fn append_refinement(&mut self, refine: MetadataRefinement) -> &mut Self {
        if self.id.is_some() {
            self.refined.push(refine);
        } else {
            // TODO: alert warning
        }

        self
    }

    /// Builds the final metadata item
    ///
    /// Requires the `builder` feature.
    pub fn build(&self) -> Self {
        Self { ..self.clone() }
    }

    /// Gets the XML attributes for this metadata item
    pub(crate) fn attributes(&self) -> Vec<(&str, &str)> {
        let mut attributes = Vec::new();

        if !ELEMENT_IN_DC_NAMESPACE.contains(&self.property.as_str()) {
            attributes.push(("property", self.property.as_str()));
        }

        if let Some(id) = &self.id {
            attributes.push(("id", id.as_str()));
        };

        if let Some(lang) = &self.lang {
            attributes.push(("lang", lang.as_str()));
        };

        attributes
    }
}

/// Represents a refinement of a metadata item in an EPUB 3.0 publication
///
/// The `MetadataRefinement` structure provides additional details about a parent metadata item.
/// Refinements are used in EPUB 3.0 to add granular metadata information that would be difficult
/// to express with the basic metadata structure alone.
///
/// For example, a creator metadata item might have refinements specifying the creator's role
/// or the scheme used for an identifier.
///
/// # Builder Methods
///
/// When the `builder` feature is enabled, this struct provides convenient builder methods:
///
/// ```rust
/// # #[cfg(feature = "builder")] {
/// use lib_epub::types::MetadataRefinement;
///
/// let refinement = MetadataRefinement::new("creator-1", "role", "author")
///     .with_lang("en")
///     .with_scheme("marc:relators")
///     .build();
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct MetadataRefinement {
    pub refines: String,

    /// The refinement property name
    ///
    /// Specifies what aspect of the parent metadata item this refinement describes.
    /// Common refinement properties include "role", "file-as", "alternate-script", etc.
    pub property: String,

    /// The refinement value
    pub value: String,

    /// Optional language code for this refinement
    pub lang: Option<String>,

    /// Optional scheme identifier for this refinement
    ///
    /// Specifies the vocabulary or scheme used for the refinement value. For example,
    /// "marc:relators" for MARC relator codes, or "onix:codelist5" for ONIX roles.
    pub scheme: Option<String>,
}

#[cfg(feature = "builder")]
impl MetadataRefinement {
    /// Creates a new metadata refinement
    ///
    /// Requires the `builder` feature.
    ///
    /// # Parameters
    /// - `refines` - The ID of the metadata item being refined
    /// - `property` - The refinement property name
    /// - `value` - The refinement value
    pub fn new(refines: &str, property: &str, value: &str) -> Self {
        Self {
            refines: refines.to_string(),
            property: property.to_string(),
            value: value.to_string(),
            lang: None,
            scheme: None,
        }
    }

    /// Sets the language of the refinement
    ///
    /// Requires the `builder` feature.
    ///
    /// # Parameters
    /// - `lang` - The language code
    pub fn with_lang(&mut self, lang: &str) -> &mut Self {
        self.lang = Some(lang.to_string());
        self
    }

    /// Sets the scheme of the refinement
    ///
    /// Requires the `builder` feature.
    ///
    /// # Parameters
    /// - `scheme` - The scheme identifier
    pub fn with_scheme(&mut self, scheme: &str) -> &mut Self {
        self.scheme = Some(scheme.to_string());
        self
    }

    /// Builds the final metadata refinement
    ///
    /// Requires the `builder` feature.
    pub fn build(&self) -> Self {
        Self { ..self.clone() }
    }

    /// Gets the XML attributes for this refinement
    pub(crate) fn attributes(&self) -> Vec<(&str, &str)> {
        let mut attributes = Vec::new();

        attributes.push(("refines", self.refines.as_str()));
        attributes.push(("property", self.property.as_str()));

        if let Some(lang) = &self.lang {
            attributes.push(("lang", lang.as_str()));
        };

        if let Some(scheme) = &self.scheme {
            attributes.push(("scheme", scheme.as_str()));
        };

        attributes
    }
}

/// Represents a metadata link item in an EPUB publication
///
/// The `MetadataLinkItem` structure represents a link from the publication's metadata to
/// external resources. These links are typically used to associate the publication with
/// external records, alternate editions, or related resources.
///
/// Link metadata items are defined in the OPF file using `<link>` elements in the metadata
/// section and follow the EPUB 3.0 metadata link specification.
#[derive(Debug)]
pub struct MetadataLinkItem {
    /// The URI of the linked resource
    pub href: String,

    /// The relationship between this publication and the linked resource
    pub rel: String,

    /// Optional language of the linked resource
    pub hreflang: Option<String>,

    /// Optional unique identifier for this link item
    ///
    /// Provides an ID that can be used to reference this link from other elements.
    pub id: Option<String>,

    /// Optional MIME type of the linked resource
    pub mime: Option<String>,

    /// Optional properties of this link
    ///
    /// Contains space-separated property values that describe characteristics of the link
    /// or the linked resource. For example, "onix-3.0" to indicate an ONIX 3.0 record.
    pub properties: Option<String>,

    /// Optional reference to another metadata item
    ///
    /// In EPUB 3.0, links can refine other metadata items. This field contains the ID
    /// of the metadata item that this link refines, prefixed with "#".
    pub refines: Option<String>,
}

/// Represents a resource item declared in the EPUB manifest
///
/// The `ManifestItem` structure represents a single resource file declared in the EPUB
/// publication's manifest. Each manifest item describes a resource that is part of the
/// publication, including its location, media type, and optional properties or fallback
/// relationships.
///
/// The manifest serves as a comprehensive inventory of all resources in an EPUB publication.
/// Every resource that is part of the publication must be declared in the manifest, and
/// resources not listed in the manifest should not be accessed by reading systems.
///
/// Manifest items support the fallback mechanism, allowing alternative versions of a resource
/// to be specified. This is particularly important for foreign resources (resources with
/// non-core media types) that may not be supported by all reading systems.
///
/// # Builder Methods
///
/// When the `builder` feature is enabled, this struct provides convenient builder methods:
///
/// ```
/// # #[cfg(feature = "builder")] {
/// use lib_epub::types::ManifestItem;
///
/// let manifest_item = ManifestItem::new("cover", "images/cover.jpg")
///     .unwrap()
///     .append_property("cover-image")
///     .with_fallback("cover-fallback")
///     .build();
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct ManifestItem {
    /// The unique identifier for this resource item
    pub id: String,

    /// The path to the resource file within the EPUB container
    ///
    /// This field contains the normalized path to the resource file relative to the
    /// root of the EPUB container. The path is processed during parsing to handle
    /// various EPUB path conventions (absolute paths, relative paths, etc.).
    pub path: PathBuf,

    /// The media type of the resource
    pub mime: String,

    /// Optional properties associated with this resource
    ///
    /// This field contains a space-separated list of properties that apply to this
    /// resource. Properties provide additional information about how the resource
    /// should be treated.
    pub properties: Option<String>,

    /// Optional fallback resource identifier
    ///
    /// This field specifies the ID of another manifest item that serves as a fallback
    /// for this resource. Fallbacks are used when a reading system does not support
    /// the media type of the primary resource. The fallback chain allows publications
    /// to include foreign resources while maintaining compatibility with older or
    /// simpler reading systems.
    ///
    /// The value is the ID of another manifest item, which must exist in the manifest.
    /// If `None`, this resource has no fallback.
    pub fallback: Option<String>,
}

// TODO: 需要增加一个函数，用于处理绝对路径‘/’和相对opf路径，将相对路径转为绝对路径
#[cfg(feature = "builder")]
impl ManifestItem {
    /// Creates a new manifest item
    ///
    /// Requires the `builder` feature.
    ///
    /// # Parameters
    /// - `id` - The unique identifier for this resource
    /// - `path` - The path to the resource file
    ///
    /// # Errors
    /// Returns an error if the path starts with "../" which is not allowed.
    pub fn new(id: &str, path: &str) -> Result<Self, EpubError> {
        if path.starts_with("../") {
            return Err(EpubBuilderError::IllegalManifestPath {
                manifest_id: id.to_string(),
            }
            .into());
        }

        Ok(Self {
            id: id.to_string(),
            path: PathBuf::from(path),
            mime: String::new(),
            properties: None,
            fallback: None,
        })
    }

    /// Sets the MIME type of the manifest item
    pub(crate) fn set_mime(self, mime: &str) -> Self {
        Self {
            id: self.id,
            path: self.path,
            mime: mime.to_string(),
            properties: self.properties,
            fallback: self.fallback,
        }
    }

    /// Appends a property to the manifest item
    ///
    /// Requires the `builder` feature.
    ///
    /// # Parameters
    /// - `property` - The property to add
    pub fn append_property(&mut self, property: &str) -> &mut Self {
        let new_properties = if let Some(properties) = &self.properties {
            format!("{} {}", properties, property)
        } else {
            property.to_string()
        };

        self.properties = Some(new_properties);
        self
    }

    /// Sets the fallback for this manifest item
    ///
    /// Requires the `builder` feature.
    ///
    /// # Parameters
    /// - `fallback` - The ID of the fallback manifest item
    pub fn with_fallback(&mut self, fallback: &str) -> &mut Self {
        self.fallback = Some(fallback.to_string());
        self
    }

    /// Builds the final manifest item
    ///
    /// Requires the `builder` feature.
    pub fn build(&self) -> Self {
        Self { ..self.clone() }
    }

    /// Gets the XML attributes for this manifest item
    pub fn attributes(&self) -> Vec<(&str, &str)> {
        let mut attributes = Vec::new();

        attributes.push(("id", self.id.as_str()));
        attributes.push(("href", self.path.to_str().unwrap()));
        attributes.push(("media-type", self.mime.as_str()));

        if let Some(properties) = &self.properties {
            attributes.push(("properties", properties.as_str()));
        }

        if let Some(fallback) = &self.fallback {
            attributes.push(("fallback", fallback.as_str()));
        }

        attributes
    }
}

/// Represents an item in the EPUB spine, defining the reading order of the publication
///
/// The `SpineItem` structure represents a single item in the EPUB spine, which defines
/// the linear reading order of the publication's content documents. Each spine item
/// references a resource declared in the manifest and indicates whether it should be
/// included in the linear reading sequence.
///
/// The spine is a crucial component of an EPUB publication as it determines the recommended
/// reading order of content documents. Items can be marked as linear (part of the main reading
/// flow) or non-linear (supplementary content that may be accessed out of sequence).
///
/// # Builder Methods
///
/// When the `builder` feature is enabled, this struct provides convenient builder methods:
///
/// ```
/// # #[cfg(feature = "builder")] {
/// use lib_epub::types::SpineItem;
///
/// let spine_item = SpineItem::new("content-1")
///     .with_id("spine-1")
///     .append_property("page-spread-right")
///     .set_linear(false)
///     .build();
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct SpineItem {
    /// The ID reference to a manifest item
    ///
    /// This field contains the ID of the manifest item that this spine item references.
    /// It establishes the connection between the reading order (spine) and the actual
    /// content resources (manifest). The referenced ID must exist in the manifest.
    pub idref: String,

    /// Optional identifier for this spine item
    pub id: Option<String>,

    /// Optional properties associated with this spine item
    ///
    /// This field contains a space-separated list of properties that apply to this
    /// spine item. These properties can indicate special handling requirements,
    /// layout preferences, or other characteristics.
    pub properties: Option<String>,

    /// Indicates whether this item is part of the linear reading order
    ///
    /// When `true`, this spine item is part of the main linear reading sequence.
    /// When `false`, this item represents supplementary content that may be accessed
    /// out of the normal reading order (e.g., through hyperlinks).
    ///
    /// Non-linear items are typically used for content like footnotes, endnotes,
    /// appendices, or other supplementary materials that readers might access
    /// on-demand rather than sequentially.
    pub linear: bool,
}

#[cfg(feature = "builder")]
impl SpineItem {
    /// Creates a new spine item referencing a manifest item
    ///
    /// Requires the `builder` feature.
    ///
    /// By default, spine items are linear.
    ///
    /// # Parameters
    /// - `idref` - The ID of the manifest item this spine item references
    pub fn new(idref: &str) -> Self {
        Self {
            idref: idref.to_string(),
            id: None,
            properties: None,
            linear: true,
        }
    }

    /// Sets the ID of the spine item
    ///
    /// Requires the `builder` feature.
    ///
    /// # Parameters
    /// - `id` - The ID to assign to this spine item
    pub fn with_id(&mut self, id: &str) -> &mut Self {
        self.id = Some(id.to_string());
        self
    }

    /// Appends a property to the spine item
    ///
    /// Requires the `builder` feature.
    ///
    /// # Parameters
    /// - `property` - The property to add
    pub fn append_property(&mut self, property: &str) -> &mut Self {
        let new_properties = if let Some(properties) = &self.properties {
            format!("{} {}", properties, property)
        } else {
            property.to_string()
        };

        self.properties = Some(new_properties);
        self
    }

    /// Sets whether this spine item is part of the linear reading order
    ///
    /// Requires the `builder` feature.
    ///
    /// # Parameters
    /// - `linear` - `true` if the item is part of the linear reading order, `false` otherwise
    pub fn set_linear(&mut self, linear: bool) -> &mut Self {
        self.linear = linear;
        self
    }

    /// Builds the final spine item
    ///
    /// Requires the `builder` feature.
    pub fn build(&self) -> Self {
        Self { ..self.clone() }
    }

    /// Gets the XML attributes for this spine item
    pub(crate) fn attributes(&self) -> Vec<(&str, &str)> {
        let mut attributes = Vec::new();

        attributes.push(("idref", self.idref.as_str()));
        attributes.push(("linear", if self.linear { "yes" } else { "no" }));

        if let Some(id) = &self.id {
            attributes.push(("id", id.as_str()));
        }

        if let Some(properties) = &self.properties {
            attributes.push(("properties", properties.as_str()));
        }

        attributes
    }
}

/// Represents encryption information for EPUB resources
///
/// This structure holds information about encrypted resources in an EPUB publication,
/// as defined in the META-INF/encryption.xml file according to the EPUB specification.
/// It describes which resources are encrypted and what encryption method was used.
#[derive(Debug, Clone)]
pub struct EncryptionData {
    /// The encryption algorithm URI
    ///
    /// This field specifies the encryption method used for the resource.
    /// Supported encryption methods:
    /// - IDPF font obfuscation: <http://www.idpf.org/2008/embedding>
    /// - Adobe font obfuscation: <http://ns.adobe.com/pdf/enc#RC>
    pub method: String,

    /// The URI of the encrypted resource
    ///
    /// This field contains the path/URI to the encrypted resource within the EPUB container.
    /// The path is relative to the root of the EPUB container.
    pub data: String,
}

/// Represents a navigation point in an EPUB document's table of contents
///
/// The `NavPoint` structure represents a single entry in the hierarchical table of contents
/// of an EPUB publication. Each navigation point corresponds to a section or chapter in
/// the publication and may contain nested child navigation points to represent sub-sections.
///
/// # Builder Methods
///
/// When the `builder` feature is enabled, this struct provides convenient builder methods:
///
/// ```
/// # #[cfg(feature = "builder")] {
/// use lib_epub::types::NavPoint;
///
/// let nav_point = NavPoint::new("Chapter 1")
///     .with_content("chapter1.xhtml")
///     .append_child(
///         NavPoint::new("Section 1.1")
///             .with_content("section1_1.xhtml")
///             .build()
///     )
///     .build();
/// # }
/// ```
#[derive(Debug, Eq, Clone)]
pub struct NavPoint {
    /// The display label/title of this navigation point
    ///
    /// This is the text that should be displayed to users in the table of contents.
    pub label: String,

    /// The content document path this navigation point references
    ///
    /// Can be `None` for navigation points that no relevant information was
    /// provided in the original data.
    pub content: Option<PathBuf>,

    /// Child navigation points (sub-sections)
    pub children: Vec<NavPoint>,

    /// The reading order position of this navigation point
    ///
    /// It can be `None` for navigation points that no relevant information was
    /// provided in the original data.
    pub play_order: Option<usize>,
}

#[cfg(feature = "builder")]
impl NavPoint {
    /// Creates a new navigation point with the given label
    ///
    /// Requires the `builder` feature.
    ///
    /// # Parameters
    /// - `label` - The display label for this navigation point
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            content: None,
            children: vec![],
            play_order: None,
        }
    }

    /// Sets the content path for this navigation point
    ///
    /// Requires the `builder` feature.
    ///
    /// # Parameters
    /// - `content` - The path to the content document
    pub fn with_content(&mut self, content: &str) -> &mut Self {
        self.content = Some(PathBuf::from(content));
        self
    }

    /// Appends a child navigation point
    ///
    /// Requires the `builder` feature.
    ///
    /// # Parameters
    /// - `child` - The child navigation point to add
    pub fn append_child(&mut self, child: NavPoint) -> &mut Self {
        self.children.push(child);
        self
    }

    /// Sets all child navigation points
    ///
    /// Requires the `builder` feature.
    ///
    /// # Parameters
    /// - `children` - Vector of child navigation points
    pub fn set_children(&mut self, children: Vec<NavPoint>) -> &mut Self {
        self.children = children;
        self
    }

    /// Builds the final navigation point
    ///
    /// Requires the `builder` feature.
    pub fn build(&self) -> Self {
        Self { ..self.clone() }
    }
}

impl Ord for NavPoint {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.play_order.cmp(&other.play_order)
    }
}

impl PartialOrd for NavPoint {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for NavPoint {
    fn eq(&self, other: &Self) -> bool {
        self.play_order == other.play_order
    }
}

#[cfg(test)]
mod tests {
    mod navpoint_tests {
        use std::path::PathBuf;

        use crate::types::NavPoint;

        /// Testing the equality comparison of NavPoint
        #[test]
        fn test_navpoint_partial_eq() {
            let nav1 = NavPoint {
                label: "Chapter 1".to_string(),
                content: Some(PathBuf::from("chapter1.html")),
                children: vec![],
                play_order: Some(1),
            };

            let nav2 = NavPoint {
                label: "Chapter 1".to_string(),
                content: Some(PathBuf::from("chapter2.html")),
                children: vec![],
                play_order: Some(1),
            };

            let nav3 = NavPoint {
                label: "Chapter 2".to_string(),
                content: Some(PathBuf::from("chapter1.html")),
                children: vec![],
                play_order: Some(2),
            };

            assert_eq!(nav1, nav2); // Same play_order, different contents, should be equal
            assert_ne!(nav1, nav3); // Different play_order, Same contents, should be unequal
        }

        /// Test NavPoint sorting comparison
        #[test]
        fn test_navpoint_ord() {
            let nav1 = NavPoint {
                label: "Chapter 1".to_string(),
                content: Some(PathBuf::from("chapter1.html")),
                children: vec![],
                play_order: Some(1),
            };

            let nav2 = NavPoint {
                label: "Chapter 2".to_string(),
                content: Some(PathBuf::from("chapter2.html")),
                children: vec![],
                play_order: Some(2),
            };

            let nav3 = NavPoint {
                label: "Chapter 3".to_string(),
                content: Some(PathBuf::from("chapter3.html")),
                children: vec![],
                play_order: Some(3),
            };

            // Test function cmp
            assert!(nav1 < nav2);
            assert!(nav2 > nav1);
            assert!(nav1 == nav1);

            // Test function partial_cmp
            assert_eq!(nav1.partial_cmp(&nav2), Some(std::cmp::Ordering::Less));
            assert_eq!(nav2.partial_cmp(&nav1), Some(std::cmp::Ordering::Greater));
            assert_eq!(nav1.partial_cmp(&nav1), Some(std::cmp::Ordering::Equal));

            // Test function sort
            let mut nav_points = vec![nav2.clone(), nav3.clone(), nav1.clone()];
            nav_points.sort();
            assert_eq!(nav_points, vec![nav1, nav2, nav3]);
        }

        /// Test the case of None play_order
        #[test]
        fn test_navpoint_ord_with_none_play_order() {
            let nav_with_order = NavPoint {
                label: "Chapter 1".to_string(),
                content: Some(PathBuf::from("chapter1.html")),
                children: vec![],
                play_order: Some(1),
            };

            let nav_without_order = NavPoint {
                label: "Preface".to_string(),
                content: Some(PathBuf::from("preface.html")),
                children: vec![],
                play_order: None,
            };

            assert!(nav_without_order < nav_with_order);
            assert!(nav_with_order > nav_without_order);

            let nav_without_order2 = NavPoint {
                label: "Introduction".to_string(),
                content: Some(PathBuf::from("intro.html")),
                children: vec![],
                play_order: None,
            };

            assert!(nav_without_order == nav_without_order2);
        }

        /// Test NavPoint containing child nodes
        #[test]
        fn test_navpoint_with_children() {
            let child1 = NavPoint {
                label: "Section 1.1".to_string(),
                content: Some(PathBuf::from("section1_1.html")),
                children: vec![],
                play_order: Some(1),
            };

            let child2 = NavPoint {
                label: "Section 1.2".to_string(),
                content: Some(PathBuf::from("section1_2.html")),
                children: vec![],
                play_order: Some(2),
            };

            let parent1 = NavPoint {
                label: "Chapter 1".to_string(),
                content: Some(PathBuf::from("chapter1.html")),
                children: vec![child1.clone(), child2.clone()],
                play_order: Some(1),
            };

            let parent2 = NavPoint {
                label: "Chapter 1".to_string(),
                content: Some(PathBuf::from("chapter1.html")),
                children: vec![child1.clone(), child2.clone()],
                play_order: Some(1),
            };

            assert!(parent1 == parent2);

            let parent3 = NavPoint {
                label: "Chapter 2".to_string(),
                content: Some(PathBuf::from("chapter2.html")),
                children: vec![child1.clone(), child2.clone()],
                play_order: Some(2),
            };

            assert!(parent1 != parent3);
            assert!(parent1 < parent3);
        }

        /// Test the case where content is None
        #[test]
        fn test_navpoint_with_none_content() {
            let nav1 = NavPoint {
                label: "Chapter 1".to_string(),
                content: None,
                children: vec![],
                play_order: Some(1),
            };

            let nav2 = NavPoint {
                label: "Chapter 1".to_string(),
                content: None,
                children: vec![],
                play_order: Some(1),
            };

            assert!(nav1 == nav2);
        }
    }

    #[cfg(feature = "builder")]
    mod builder_tests {
        mod metadata_item {
            use crate::types::{MetadataItem, MetadataRefinement};

            #[test]
            fn test_metadata_item_new() {
                let metadata_item = MetadataItem::new("title", "EPUB Test Book");

                assert_eq!(metadata_item.property, "title");
                assert_eq!(metadata_item.value, "EPUB Test Book");
                assert_eq!(metadata_item.id, None);
                assert_eq!(metadata_item.lang, None);
                assert_eq!(metadata_item.refined.len(), 0);
            }

            #[test]
            fn test_metadata_item_with_id() {
                let mut metadata_item = MetadataItem::new("creator", "John Doe");
                metadata_item.with_id("creator-1");

                assert_eq!(metadata_item.property, "creator");
                assert_eq!(metadata_item.value, "John Doe");
                assert_eq!(metadata_item.id, Some("creator-1".to_string()));
                assert_eq!(metadata_item.lang, None);
                assert_eq!(metadata_item.refined.len(), 0);
            }

            #[test]
            fn test_metadata_item_with_lang() {
                let mut metadata_item = MetadataItem::new("title", "测试书籍");
                metadata_item.with_lang("zh-CN");

                assert_eq!(metadata_item.property, "title");
                assert_eq!(metadata_item.value, "测试书籍");
                assert_eq!(metadata_item.id, None);
                assert_eq!(metadata_item.lang, Some("zh-CN".to_string()));
                assert_eq!(metadata_item.refined.len(), 0);
            }

            #[test]
            fn test_metadata_item_append_refinement() {
                let mut metadata_item = MetadataItem::new("creator", "John Doe");
                metadata_item.with_id("creator-1"); // ID is required for refinements

                let refinement = MetadataRefinement::new("creator-1", "role", "author");
                metadata_item.append_refinement(refinement);

                assert_eq!(metadata_item.refined.len(), 1);
                assert_eq!(metadata_item.refined[0].refines, "creator-1");
                assert_eq!(metadata_item.refined[0].property, "role");
                assert_eq!(metadata_item.refined[0].value, "author");
            }

            #[test]
            fn test_metadata_item_append_refinement_without_id() {
                let mut metadata_item = MetadataItem::new("title", "Test Book");
                // No ID set

                let refinement = MetadataRefinement::new("title", "title-type", "main");
                metadata_item.append_refinement(refinement);

                // Refinement should not be added because metadata item has no ID
                assert_eq!(metadata_item.refined.len(), 0);
            }

            #[test]
            fn test_metadata_item_build() {
                let mut metadata_item = MetadataItem::new("identifier", "urn:isbn:1234567890");
                metadata_item.with_id("pub-id").with_lang("en");

                let built = metadata_item.build();

                assert_eq!(built.property, "identifier");
                assert_eq!(built.value, "urn:isbn:1234567890");
                assert_eq!(built.id, Some("pub-id".to_string()));
                assert_eq!(built.lang, Some("en".to_string()));
                assert_eq!(built.refined.len(), 0);
            }

            #[test]
            fn test_metadata_item_builder_chaining() {
                let mut metadata_item = MetadataItem::new("title", "EPUB 3.3 Guide");
                metadata_item.with_id("title").with_lang("en");

                let refinement = MetadataRefinement::new("title", "title-type", "main");
                metadata_item.append_refinement(refinement);

                let built = metadata_item.build();

                assert_eq!(built.property, "title");
                assert_eq!(built.value, "EPUB 3.3 Guide");
                assert_eq!(built.id, Some("title".to_string()));
                assert_eq!(built.lang, Some("en".to_string()));
                assert_eq!(built.refined.len(), 1);
            }

            #[test]
            fn test_metadata_item_attributes_dc_namespace() {
                let mut metadata_item = MetadataItem::new("title", "Test Book");
                metadata_item.with_id("title-id");

                let attributes = metadata_item.attributes();

                // For DC namespace properties, no "property" attribute should be added
                assert!(!attributes.iter().any(|(k, _)| k == &"property"));
                assert!(
                    attributes
                        .iter()
                        .any(|(k, v)| k == &"id" && v == &"title-id")
                );
            }

            #[test]
            fn test_metadata_item_attributes_non_dc_namespace() {
                let mut metadata_item = MetadataItem::new("meta", "value");
                metadata_item.with_id("meta-id");

                let attributes = metadata_item.attributes();

                // For non-DC namespace properties, "property" attribute should be added
                assert!(attributes.iter().any(|(k, _)| k == &"property"));
                assert!(
                    attributes
                        .iter()
                        .any(|(k, v)| k == &"id" && v == &"meta-id")
                );
            }

            #[test]
            fn test_metadata_item_attributes_with_lang() {
                let mut metadata_item = MetadataItem::new("title", "Test Book");
                metadata_item.with_id("title-id").with_lang("en");

                let attributes = metadata_item.attributes();

                assert!(
                    attributes
                        .iter()
                        .any(|(k, v)| k == &"id" && v == &"title-id")
                );
                assert!(attributes.iter().any(|(k, v)| k == &"lang" && v == &"en"));
            }
        }

        mod metadata_refinement {
            use crate::types::MetadataRefinement;

            #[test]
            fn test_metadata_refinement_new() {
                let refinement = MetadataRefinement::new("title", "title-type", "main");

                assert_eq!(refinement.refines, "title");
                assert_eq!(refinement.property, "title-type");
                assert_eq!(refinement.value, "main");
                assert_eq!(refinement.lang, None);
                assert_eq!(refinement.scheme, None);
            }

            #[test]
            fn test_metadata_refinement_with_lang() {
                let mut refinement = MetadataRefinement::new("creator", "role", "author");
                refinement.with_lang("en");

                assert_eq!(refinement.refines, "creator");
                assert_eq!(refinement.property, "role");
                assert_eq!(refinement.value, "author");
                assert_eq!(refinement.lang, Some("en".to_string()));
                assert_eq!(refinement.scheme, None);
            }

            #[test]
            fn test_metadata_refinement_with_scheme() {
                let mut refinement = MetadataRefinement::new("creator", "role", "author");
                refinement.with_scheme("marc:relators");

                assert_eq!(refinement.refines, "creator");
                assert_eq!(refinement.property, "role");
                assert_eq!(refinement.value, "author");
                assert_eq!(refinement.lang, None);
                assert_eq!(refinement.scheme, Some("marc:relators".to_string()));
            }

            #[test]
            fn test_metadata_refinement_build() {
                let mut refinement = MetadataRefinement::new("title", "alternate-script", "テスト");
                refinement.with_lang("ja").with_scheme("iso-15924");

                let built = refinement.build();

                assert_eq!(built.refines, "title");
                assert_eq!(built.property, "alternate-script");
                assert_eq!(built.value, "テスト");
                assert_eq!(built.lang, Some("ja".to_string()));
                assert_eq!(built.scheme, Some("iso-15924".to_string()));
            }

            #[test]
            fn test_metadata_refinement_builder_chaining() {
                let mut refinement = MetadataRefinement::new("creator", "file-as", "Doe, John");
                refinement.with_lang("en").with_scheme("dcterms");

                let built = refinement.build();

                assert_eq!(built.refines, "creator");
                assert_eq!(built.property, "file-as");
                assert_eq!(built.value, "Doe, John");
                assert_eq!(built.lang, Some("en".to_string()));
                assert_eq!(built.scheme, Some("dcterms".to_string()));
            }

            #[test]
            fn test_metadata_refinement_attributes() {
                let mut refinement = MetadataRefinement::new("title", "title-type", "main");
                refinement.with_lang("en").with_scheme("onix:codelist5");

                let attributes = refinement.attributes();

                assert!(
                    attributes
                        .iter()
                        .any(|(k, v)| k == &"refines" && v == &"title")
                );
                assert!(
                    attributes
                        .iter()
                        .any(|(k, v)| k == &"property" && v == &"title-type")
                );
                assert!(attributes.iter().any(|(k, v)| k == &"lang" && v == &"en"));
                assert!(
                    attributes
                        .iter()
                        .any(|(k, v)| k == &"scheme" && v == &"onix:codelist5")
                );
            }

            #[test]
            fn test_metadata_refinement_attributes_optional_fields() {
                let refinement = MetadataRefinement::new("creator", "role", "author");
                let attributes = refinement.attributes();

                assert!(
                    attributes
                        .iter()
                        .any(|(k, v)| k == &"refines" && v == &"creator")
                );
                assert!(
                    attributes
                        .iter()
                        .any(|(k, v)| k == &"property" && v == &"role")
                );

                // Should not contain optional attributes when they are None
                assert!(!attributes.iter().any(|(k, _)| k == &"lang"));
                assert!(!attributes.iter().any(|(k, _)| k == &"scheme"));
            }
        }

        mod manifest_item {
            use std::path::PathBuf;

            use crate::types::ManifestItem;

            #[test]
            fn test_manifest_item_new() {
                let manifest_item = ManifestItem::new("cover", "images/cover.jpg");
                assert!(manifest_item.is_ok());

                let manifest_item = manifest_item.unwrap();
                assert_eq!(manifest_item.id, "cover");
                assert_eq!(manifest_item.path, PathBuf::from("images/cover.jpg"));
                assert_eq!(manifest_item.mime, "");
                assert_eq!(manifest_item.properties, None);
                assert_eq!(manifest_item.fallback, None);
            }

            #[test]
            fn test_manifest_item_append_property() {
                let manifest_item = ManifestItem::new("nav", "nav.xhtml");
                assert!(manifest_item.is_ok());

                let mut manifest_item = manifest_item.unwrap();
                manifest_item.append_property("nav");

                assert_eq!(manifest_item.id, "nav");
                assert_eq!(manifest_item.path, PathBuf::from("nav.xhtml"));
                assert_eq!(manifest_item.mime, "");
                assert_eq!(manifest_item.properties, Some("nav".to_string()));
                assert_eq!(manifest_item.fallback, None);
            }

            #[test]
            fn test_manifest_item_append_multiple_properties() {
                let manifest_item = ManifestItem::new("content", "content.xhtml");
                assert!(manifest_item.is_ok());

                let mut manifest_item = manifest_item.unwrap();
                manifest_item
                    .append_property("nav")
                    .append_property("scripted")
                    .append_property("svg");

                assert_eq!(
                    manifest_item.properties,
                    Some("nav scripted svg".to_string())
                );
            }

            #[test]
            fn test_manifest_item_with_fallback() {
                let manifest_item = ManifestItem::new("image", "image.tiff");
                assert!(manifest_item.is_ok());

                let mut manifest_item = manifest_item.unwrap();
                manifest_item.with_fallback("image-fallback");

                assert_eq!(manifest_item.id, "image");
                assert_eq!(manifest_item.path, PathBuf::from("image.tiff"));
                assert_eq!(manifest_item.mime, "");
                assert_eq!(manifest_item.properties, None);
                assert_eq!(manifest_item.fallback, Some("image-fallback".to_string()));
            }

            #[test]
            fn test_manifest_item_set_mime() {
                let manifest_item = ManifestItem::new("style", "style.css");
                assert!(manifest_item.is_ok());

                let manifest_item = manifest_item.unwrap();
                let updated_item = manifest_item.set_mime("text/css");

                assert_eq!(updated_item.id, "style");
                assert_eq!(updated_item.path, PathBuf::from("style.css"));
                assert_eq!(updated_item.mime, "text/css");
                assert_eq!(updated_item.properties, None);
                assert_eq!(updated_item.fallback, None);
            }

            #[test]
            fn test_manifest_item_build() {
                let manifest_item = ManifestItem::new("cover", "images/cover.jpg");
                assert!(manifest_item.is_ok());

                let mut manifest_item = manifest_item.unwrap();
                manifest_item
                    .append_property("cover-image")
                    .with_fallback("cover-fallback");

                let built = manifest_item.build();

                assert_eq!(built.id, "cover");
                assert_eq!(built.path, PathBuf::from("images/cover.jpg"));
                assert_eq!(built.mime, "");
                assert_eq!(built.properties, Some("cover-image".to_string()));
                assert_eq!(built.fallback, Some("cover-fallback".to_string()));
            }

            #[test]
            fn test_manifest_item_builder_chaining() {
                let manifest_item = ManifestItem::new("content", "content.xhtml");
                assert!(manifest_item.is_ok());

                let mut manifest_item = manifest_item.unwrap();
                manifest_item
                    .append_property("scripted")
                    .append_property("svg")
                    .with_fallback("fallback-content");

                let built = manifest_item.build();

                assert_eq!(built.id, "content");
                assert_eq!(built.path, PathBuf::from("content.xhtml"));
                assert_eq!(built.mime, "");
                assert_eq!(built.properties, Some("scripted svg".to_string()));
                assert_eq!(built.fallback, Some("fallback-content".to_string()));
            }

            #[test]
            fn test_manifest_item_attributes() {
                let manifest_item = ManifestItem::new("nav", "nav.xhtml");
                assert!(manifest_item.is_ok());

                let mut manifest_item = manifest_item.unwrap();
                manifest_item
                    .append_property("nav")
                    .with_fallback("fallback-nav");

                // Manually set mime type for testing
                let manifest_item = manifest_item.set_mime("application/xhtml+xml");
                let attributes = manifest_item.attributes();

                // Check that all expected attributes are present
                assert!(attributes.contains(&("id", "nav")));
                assert!(attributes.contains(&("href", "nav.xhtml")));
                assert!(attributes.contains(&("media-type", "application/xhtml+xml")));
                assert!(attributes.contains(&("properties", "nav")));
                assert!(attributes.contains(&("fallback", "fallback-nav")));
            }

            #[test]
            fn test_manifest_item_attributes_optional_fields() {
                let manifest_item = ManifestItem::new("simple", "simple.xhtml");
                assert!(manifest_item.is_ok());

                let manifest_item = manifest_item.unwrap();
                let manifest_item = manifest_item.set_mime("application/xhtml+xml");
                let attributes = manifest_item.attributes();

                // Should contain required attributes
                assert!(attributes.contains(&("id", "simple")));
                assert!(attributes.contains(&("href", "simple.xhtml")));
                assert!(attributes.contains(&("media-type", "application/xhtml+xml")));

                // Should not contain optional attributes when they are None
                assert!(!attributes.iter().any(|(k, _)| k == &"properties"));
                assert!(!attributes.iter().any(|(k, _)| k == &"fallback"));
            }

            #[test]
            fn test_manifest_item_path_handling() {
                let manifest_item = ManifestItem::new("test", "../images/test.png");
                assert!(manifest_item.is_err());

                let err = manifest_item.unwrap_err();
                assert_eq!(
                    err.to_string(),
                    "Epub builder error: A manifest with id 'test' should not use a relative path starting with '../'."
                );
            }
        }

        mod spine_item {
            use crate::types::SpineItem;

            #[test]
            fn test_spine_item_new() {
                let spine_item = SpineItem::new("content_001");

                assert_eq!(spine_item.idref, "content_001");
                assert_eq!(spine_item.id, None);
                assert_eq!(spine_item.properties, None);
                assert_eq!(spine_item.linear, true);
            }

            #[test]
            fn test_spine_item_with_id() {
                let mut spine_item = SpineItem::new("content_001");
                spine_item.with_id("spine1");

                assert_eq!(spine_item.idref, "content_001");
                assert_eq!(spine_item.id, Some("spine1".to_string()));
                assert_eq!(spine_item.properties, None);
                assert_eq!(spine_item.linear, true);
            }

            #[test]
            fn test_spine_item_append_property() {
                let mut spine_item = SpineItem::new("content_001");
                spine_item.append_property("page-spread-left");

                assert_eq!(spine_item.idref, "content_001");
                assert_eq!(spine_item.id, None);
                assert_eq!(spine_item.properties, Some("page-spread-left".to_string()));
                assert_eq!(spine_item.linear, true);
            }

            #[test]
            fn test_spine_item_append_multiple_properties() {
                let mut spine_item = SpineItem::new("content_001");
                spine_item
                    .append_property("page-spread-left")
                    .append_property("rendition:layout-pre-paginated");

                assert_eq!(
                    spine_item.properties,
                    Some("page-spread-left rendition:layout-pre-paginated".to_string())
                );
            }

            #[test]
            fn test_spine_item_set_linear() {
                let mut spine_item = SpineItem::new("content_001");
                spine_item.set_linear(false);

                assert_eq!(spine_item.idref, "content_001");
                assert_eq!(spine_item.id, None);
                assert_eq!(spine_item.properties, None);
                assert_eq!(spine_item.linear, false);
            }

            #[test]
            fn test_spine_item_build() {
                let mut spine_item = SpineItem::new("content_001");
                spine_item
                    .with_id("spine1")
                    .append_property("page-spread-left")
                    .set_linear(false);

                let built = spine_item.build();

                assert_eq!(built.idref, "content_001");
                assert_eq!(built.id, Some("spine1".to_string()));
                assert_eq!(built.properties, Some("page-spread-left".to_string()));
                assert_eq!(built.linear, false);
            }

            #[test]
            fn test_spine_item_builder_chaining() {
                let mut spine_item = SpineItem::new("content_001");
                spine_item
                    .with_id("spine1")
                    .append_property("page-spread-left")
                    .set_linear(false);

                let built = spine_item.build();

                assert_eq!(built.idref, "content_001");
                assert_eq!(built.id, Some("spine1".to_string()));
                assert_eq!(built.properties, Some("page-spread-left".to_string()));
                assert_eq!(built.linear, false);
            }

            #[test]
            fn test_spine_item_attributes() {
                let mut spine_item = SpineItem::new("content_001");
                spine_item
                    .with_id("spine1")
                    .append_property("page-spread-left")
                    .set_linear(false);

                let attributes = spine_item.attributes();

                // Check that all expected attributes are present
                assert!(attributes.contains(&("idref", "content_001")));
                assert!(attributes.contains(&("id", "spine1")));
                assert!(attributes.contains(&("properties", "page-spread-left")));
                assert!(attributes.contains(&("linear", "no"))); // false should become "no"
            }

            #[test]
            fn test_spine_item_attributes_linear_yes() {
                let spine_item = SpineItem::new("content_001");
                let attributes = spine_item.attributes();

                // Linear true should become "yes"
                assert!(attributes.contains(&("linear", "yes")));
            }

            #[test]
            fn test_spine_item_attributes_optional_fields() {
                let spine_item = SpineItem::new("content_001");
                let attributes = spine_item.attributes();

                // Should only contain required attributes when optional fields are None
                assert!(attributes.contains(&("idref", "content_001")));
                assert!(attributes.contains(&("linear", "yes")));

                // Should not contain optional attributes when they are None
                assert!(!attributes.iter().any(|(k, _)| k == &"id"));
                assert!(!attributes.iter().any(|(k, _)| k == &"properties"));
            }
        }

        mod navpoint {

            use std::path::PathBuf;

            use crate::types::NavPoint;

            #[test]
            fn test_navpoint_new() {
                let navpoint = NavPoint::new("Test Chapter");

                assert_eq!(navpoint.label, "Test Chapter");
                assert_eq!(navpoint.content, None);
                assert_eq!(navpoint.children.len(), 0);
            }

            #[test]
            fn test_navpoint_with_content() {
                let mut navpoint = NavPoint::new("Test Chapter");
                navpoint.with_content("chapter1.html");

                assert_eq!(navpoint.label, "Test Chapter");
                assert_eq!(navpoint.content, Some(PathBuf::from("chapter1.html")));
                assert_eq!(navpoint.children.len(), 0);
            }

            #[test]
            fn test_navpoint_append_child() {
                let mut parent = NavPoint::new("Parent Chapter");

                let mut child1 = NavPoint::new("Child Section 1");
                child1.with_content("section1.html");

                let mut child2 = NavPoint::new("Child Section 2");
                child2.with_content("section2.html");

                parent.append_child(child1.build());
                parent.append_child(child2.build());

                assert_eq!(parent.children.len(), 2);
                assert_eq!(parent.children[0].label, "Child Section 1");
                assert_eq!(parent.children[1].label, "Child Section 2");
            }

            #[test]
            fn test_navpoint_set_children() {
                let mut navpoint = NavPoint::new("Main Chapter");
                let children = vec![NavPoint::new("Section 1"), NavPoint::new("Section 2")];

                navpoint.set_children(children);

                assert_eq!(navpoint.children.len(), 2);
                assert_eq!(navpoint.children[0].label, "Section 1");
                assert_eq!(navpoint.children[1].label, "Section 2");
            }

            #[test]
            fn test_navpoint_build() {
                let mut navpoint = NavPoint::new("Complete Chapter");
                navpoint.with_content("complete.html");

                let child = NavPoint::new("Sub Section");
                navpoint.append_child(child.build());

                let built = navpoint.build();

                assert_eq!(built.label, "Complete Chapter");
                assert_eq!(built.content, Some(PathBuf::from("complete.html")));
                assert_eq!(built.children.len(), 1);
                assert_eq!(built.children[0].label, "Sub Section");
            }

            #[test]
            fn test_navpoint_builder_chaining() {
                let mut navpoint = NavPoint::new("Chained Chapter");

                navpoint
                    .with_content("chained.html")
                    .append_child(NavPoint::new("Child 1").build())
                    .append_child(NavPoint::new("Child 2").build());

                let built = navpoint.build();

                assert_eq!(built.label, "Chained Chapter");
                assert_eq!(built.content, Some(PathBuf::from("chained.html")));
                assert_eq!(built.children.len(), 2);
            }

            #[test]
            fn test_navpoint_empty_children() {
                let navpoint = NavPoint::new("No Children Chapter");
                let built = navpoint.build();

                assert_eq!(built.children.len(), 0);
            }

            #[test]
            fn test_navpoint_complex_hierarchy() {
                let mut root = NavPoint::new("Book");

                let mut chapter1 = NavPoint::new("Chapter 1");
                chapter1
                    .with_content("chapter1.html")
                    .append_child(
                        NavPoint::new("Section 1.1")
                            .with_content("sec1_1.html")
                            .build(),
                    )
                    .append_child(
                        NavPoint::new("Section 1.2")
                            .with_content("sec1_2.html")
                            .build(),
                    );

                let mut chapter2 = NavPoint::new("Chapter 2");
                chapter2.with_content("chapter2.html").append_child(
                    NavPoint::new("Section 2.1")
                        .with_content("sec2_1.html")
                        .build(),
                );

                root.append_child(chapter1.build())
                    .append_child(chapter2.build());

                let book = root.build();

                assert_eq!(book.label, "Book");
                assert_eq!(book.children.len(), 2);

                let ch1 = &book.children[0];
                assert_eq!(ch1.label, "Chapter 1");
                assert_eq!(ch1.children.len(), 2);

                let ch2 = &book.children[1];
                assert_eq!(ch2.label, "Chapter 2");
                assert_eq!(ch2.children.len(), 1);
            }
        }
    }
}
