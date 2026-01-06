//! Error Type Definition Module
//!
//! This module defines the various error types that may be encountered during
//! EPUB file parsing and processing. All errors are uniformly wrapped in the
//! `EpubError` enumeration for convenient error handling by the caller.
//!
//! ## Main Error Types
//!
//! - [EpubError] - Enumeration of main errors during EPUB processing
//! - [EpubBuilderError] - Specific errors during EPUB build process (requires `builder` functionality enabled)

use thiserror::Error;

/// Types of errors that can occur during EPUB processing
///
/// This enumeration defines the various error cases that can be encountered
/// when parsing and processing EPUB files, including file format errors,
/// missing resources, compression issues, etc.
#[derive(Debug, Error)]
pub enum EpubError {
    /// ZIP archive related errors
    ///
    /// Errors occur when processing the ZIP structure of EPUB files,
    /// such as file corruption, unreadability, etc.
    #[error("Archive error: {source}")]
    ArchiveError { source: zip::result::ZipError },

    /// Data Decoding Error - Null dataw
    ///
    /// This error occurs when trying to decode an empty stream.
    #[error("Decode error: The data is empty.")]
    EmptyDataError,

    #[cfg(feature = "builder")]
    #[error("Epub builder error: {source}")]
    EpubBuilderError { source: EpubBuilderError },

    /// XML parsing failure error
    ///
    /// This error usually only occurs when there is an exception in the XML parsing process,
    /// the event listener ends abnormally, resulting in the root node not being initialized.
    /// This exception may be caused by an incorrect XML file.
    #[error(
        "Failed parsing XML error: Unknown problems occurred during XML parsing, causing parsing failure."
    )]
    FailedParsingXml,

    #[error("IO error: {source}")]
    IOError { source: std::io::Error },

    /// Missing required attribute error
    ///
    /// Triggered when an XML element in an EPUB file lacks the required
    /// attributes required by the EPUB specification.
    #[error(
        "Missing required attribute: The \"{attribute}\" attribute is a must attribute for the \"{tag}\" element."
    )]
    MissingRequiredAttribute { tag: String, attribute: String },

    /// Mutex error
    ///
    /// This error occurs when a mutex is poisoned, which means
    /// that a thread has panicked while holding a lock on the mutex.
    #[error("Mutex error: Mutex was poisoned.")]
    MutexError,

    /// Non-canonical EPUB structure error
    ///
    /// This error occurs when an EPUB file lacks some files or directory
    /// structure that is required in EPUB specification.
    #[error("Non-canonical epub: The \"{expected_file}\" file was not found.")]
    NonCanonicalEpub { expected_file: String },

    /// Non-canonical file structure error
    ///
    /// This error is triggered when the required XML elements in the
    /// specification are missing from the EPUB file.
    #[error("Non-canonical file: The \"{tag}\" elements was not found.")]
    NonCanonicalFile { tag: String },

    /// Missing supported file format error
    ///
    /// This error occurs when trying to get a resource but there isn't any supported file format.
    /// It usually happens when there are no supported formats available in the fallback chain.
    #[error(
        "No supported file format: The fallback resource does not contain the file format you support."
    )]
    NoSupportedFileFormat,

    /// Relative link leak error
    ///
    /// This error occurs when a relative path link is outside the scope
    /// of an EPUB container, which is a security protection mechanism.
    #[error("Relative link leakage: Path \"{path}\" is out of container range.")]
    RealtiveLinkLeakage { path: String },

    /// Unable to find the resource id error
    ///
    /// This error occurs when trying to get a resource by id but that id doesn't exist in the manifest.
    #[error("Resource Id Not Exist: There is no resource item with id \"{id}\".")]
    ResourceIdNotExist { id: String },

    /// Unable to find the resource error
    ///
    /// This error occurs when an attempt is made to get a resource
    /// but it does not exist in the EPUB container.
    #[error("Resource not found: Unable to find resource from \"{resource}\".")]
    ResourceNotFound { resource: String },

    /// Unrecognized EPUB version error
    ///
    /// This error occurs when parsing epub files, the library cannot
    /// directly or indirectly identify the epub version number.
    #[error(
        "Unrecognized EPUB version: Unable to identify version number and version characteristics from epub file"
    )]
    UnrecognizedEpubVersion,

    /// Unsupported encryption method error
    ///
    /// This error is triggered when attempting to decrypt a resource that uses
    /// an encryption method not supported by this library.
    ///
    /// Currently, this library only supports:
    /// - IDPF Font Obfuscation
    /// - Adobe Font Obfuscation
    #[error("Unsupported encryption method: The \"{method}\" encryption method is not supported.")]
    UnsupportedEncryptedMethod { method: String },

    /// Unusable compression method error
    ///
    /// This error occurs when an EPUB file uses an unsupported compression method.
    #[error(
        "Unusable compression method: The \"{file}\" file uses the unsupported \"{method}\" compression method."
    )]
    UnusableCompressionMethod { file: String, method: String },

    /// UTF-8 decoding error
    ///
    /// This error occurs when attempting to decode byte data into a UTF-8 string
    /// but the data is not formatted correctly.
    #[error("Decode error: {source}")]
    Utf8DecodeError { source: std::string::FromUtf8Error },

    /// UTF-16 decoding error
    ///
    /// This error occurs when attempting to decode byte data into a UTF-16 string
    /// but the data is not formatted correctly.
    #[error("Decode error: {source}")]
    Utf16DecodeError { source: std::string::FromUtf16Error },

    /// WalkDir error
    ///
    /// This error occurs when using the WalkDir library to traverse the directory.
    #[cfg(feature = "builder")]
    #[error("WalkDir error: {source}")]
    WalkDirError { source: walkdir::Error },

    /// QuickXml error
    ///
    /// This error occurs when parsing XML data using the QuickXml library.
    #[error("QuickXml error: {source}")]
    QuickXmlError { source: quick_xml::Error },
}

impl From<zip::result::ZipError> for EpubError {
    fn from(value: zip::result::ZipError) -> Self {
        EpubError::ArchiveError { source: value }
    }
}

impl From<quick_xml::Error> for EpubError {
    fn from(value: quick_xml::Error) -> Self {
        EpubError::QuickXmlError { source: value }
    }
}

impl From<std::io::Error> for EpubError {
    fn from(value: std::io::Error) -> Self {
        EpubError::IOError { source: value }
    }
}

impl From<std::string::FromUtf8Error> for EpubError {
    fn from(value: std::string::FromUtf8Error) -> Self {
        EpubError::Utf8DecodeError { source: value }
    }
}

impl From<std::string::FromUtf16Error> for EpubError {
    fn from(value: std::string::FromUtf16Error) -> Self {
        EpubError::Utf16DecodeError { source: value }
    }
}

impl<T> From<std::sync::PoisonError<T>> for EpubError {
    fn from(_value: std::sync::PoisonError<T>) -> Self {
        EpubError::MutexError
    }
}

#[cfg(feature = "builder")]
impl From<EpubBuilderError> for EpubError {
    fn from(value: EpubBuilderError) -> Self {
        EpubError::EpubBuilderError { source: value }
    }
}

#[cfg(feature = "builder")]
impl From<walkdir::Error> for EpubError {
    fn from(value: walkdir::Error) -> Self {
        EpubError::WalkDirError { source: value }
    }
}

#[cfg(test)]
impl PartialEq for EpubError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::MissingRequiredAttribute {
                    tag: l_tag,
                    attribute: l_attribute,
                },
                Self::MissingRequiredAttribute {
                    tag: r_tag,
                    attribute: r_attribute,
                },
            ) => l_tag == r_tag && l_attribute == r_attribute,
            (
                Self::NonCanonicalEpub {
                    expected_file: l_expected_file,
                },
                Self::NonCanonicalEpub {
                    expected_file: r_expected_file,
                },
            ) => l_expected_file == r_expected_file,
            (Self::NonCanonicalFile { tag: l_tag }, Self::NonCanonicalFile { tag: r_tag }) => {
                l_tag == r_tag
            }
            (
                Self::RealtiveLinkLeakage { path: l_path },
                Self::RealtiveLinkLeakage { path: r_path },
            ) => l_path == r_path,
            (Self::ResourceIdNotExist { id: l_id }, Self::ResourceIdNotExist { id: r_id }) => {
                l_id == r_id
            }
            (
                Self::ResourceNotFound {
                    resource: l_resource,
                },
                Self::ResourceNotFound {
                    resource: r_resource,
                },
            ) => l_resource == r_resource,
            (
                Self::UnsupportedEncryptedMethod { method: l_method },
                Self::UnsupportedEncryptedMethod { method: r_method },
            ) => l_method == r_method,
            (
                Self::UnusableCompressionMethod {
                    file: l_file,
                    method: l_method,
                },
                Self::UnusableCompressionMethod {
                    file: r_file,
                    method: r_method,
                },
            ) => l_file == r_file && l_method == r_method,
            (
                Self::Utf8DecodeError { source: l_source },
                Self::Utf8DecodeError { source: r_source },
            ) => l_source == r_source,

            (
                Self::EpubBuilderError { source: l_source },
                Self::EpubBuilderError { source: r_source },
            ) => l_source == r_source,

            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

/// Types of errors that can occur during EPUB build
///
/// This enumeration defines various error conditions that may occur
/// when creating EPUB files using the `builder` function. These errors
/// are typically related to EPUB specification requirements or validation
/// rules during the build process.
#[cfg(feature = "builder")]
#[derive(Debug, Error)]
#[cfg_attr(test, derive(PartialEq))]
pub enum EpubBuilderError {
    /// Illegal manifest path error
    ///
    /// This error is triggered when the path corresponding to a resource ID
    /// in the manifest begins with "../". Using relative paths starting with "../"
    /// when building the manifest fails to determine the 'current location',
    /// making it impossible to pinpoint the resource.
    #[error(
        "A manifest with id '{manifest_id}' should not use a relative path starting with '../'."
    )]
    IllegalManifestPath { manifest_id: String },

    /// Invalid rootfile path error
    ///
    /// This error is triggered when the rootfile path in the container.xml is invalid.
    /// According to the EPUB specification, rootfile paths must be relative paths
    /// that do not start with "../" to prevent directory traversal outside the EPUB container.
    #[error("A rootfile path should be a relative path and not start with '../'.")]
    IllegalRootfilePath,

    /// Manifest Circular Reference error
    ///
    /// This error is triggered when a fallback relationship between manifest items forms a cycle.
    #[error("Circular reference detected in fallback chain for '{fallback_chain}'.")]
    ManifestCircularReference { fallback_chain: String },

    /// Manifest resource not found error
    ///
    /// This error is triggered when a manifest item specifies a fallback resource ID that does not exist.
    #[error("Fallback resource '{manifest_id}' does not exist in manifest.")]
    ManifestNotFound { manifest_id: String },

    /// Missing necessary metadata error
    ///
    /// This error is triggered when the basic metadata required to build a valid EPUB is missing.
    /// The following must be included: title, language, and an identifier with a 'pub-id' ID.
    #[error("Requires at least one 'title', 'language', and 'identifier' with id 'pub-id'.")]
    MissingNecessaryMetadata,

    /// Navigation information uninitialized error
    ///
    /// This error is triggered when attempting to build an EPUB but without setting navigation information.
    #[error("Navigation information is not set.")]
    NavigationInfoUninitalized,

    /// Missing rootfile error
    ///
    /// This error is triggered when attempting to build an EPUB without adding any 'rootfile'.
    #[error("Need at least one rootfile.")]
    MissingRootfile,

    /// Target is not a file error
    ///
    /// This error is triggered when the specified target path is not a file.
    #[error("Expect a file, but '{target_path}' is not a file.")]
    TargetIsNotFile { target_path: String },

    /// Too many nav flags error
    ///
    /// This error is triggered when the manifest contains multiple items with
    /// the `nav` attribute. The EPUB specification requires that each EPUB have
    /// **only one** navigation document.
    #[error("There are too many items with 'nav' property in the manifest.")]
    TooManyNavFlags,

    /// Unknown file format error
    ///
    /// This error is triggered when the format type of the specified file cannot be analyzed.
    #[error("Unable to analyze the file '{file_path}' type.")]
    UnknownFileFormat { file_path: String },
}
