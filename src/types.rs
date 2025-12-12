use std::path::PathBuf;

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

/// Represents a refinement of a metadata item in an EPUB 3.0 publication
///
/// The `MetadataRefinement` structure provides additional details about a parent metadata item.
/// Refinements are used in EPUB 3.0 to add granular metadata information that would be difficult
/// to express with the basic metadata structure alone.
///
/// For example, a creator metadata item might have refinements specifying the creator's role
/// or the scheme used for an identifier.
#[derive(Debug, Clone)]
pub struct MetadataRefinement {
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
#[derive(Debug, Clone)]
pub struct ManifestItem {
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
#[derive(Debug)]
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
    /// - IDPF font obfuscation: "http://www.idpf.org/2008/embedding"
    /// - Adobe font obfuscation: "http://ns.adobe.com/pdf/enc#RC"
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
}
