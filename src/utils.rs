use std::{
    cmp::min,
    collections::HashMap,
    io::{Read, Seek},
    path::{Path, PathBuf},
};

#[cfg(feature = "builder")]
use chrono::Local;
use quick_xml::{NsReader, events::Event};
use sha1::{Digest, Sha1};
use sha2::Sha256;
use zip::{CompressionMethod, ZipArchive};

use crate::error::EpubError;

#[cfg(feature = "builder")]
pub static ELEMENT_IN_DC_NAMESPACE: std::sync::LazyLock<Vec<&str>> =
    std::sync::LazyLock::new(|| {
        vec![
            "contributor",
            "coverage",
            "creator",
            "date",
            "description",
            "format",
            "identifier",
            "language",
            "publisher",
            "relation",
            "rights",
            "source",
            "subject",
            "title",
            "type",
        ]
    });

#[cfg(feature = "builder")]
/// Returns the current time with custom format
pub fn local_time() -> String {
    Local::now().format("%Y-%m-%dT%H-%M-%S.%fU%z").to_string()
}

/// Extracts the contents of a specified file from a ZIP archive
///
/// This function reads the raw byte data of a specified file from an EPUB file (which
/// is essentially a ZIP archive). This is a fundamental utility function for handling
/// files within an EPUB (such as OPF, NCX, container files, etc.).
///
/// ## Parameters
/// - `zip_file`: A mutable reference to a ZIP archive object
/// - `file_name`: The path to the file to extract (relative to the ZIP archive root directory)
///
/// ## Return
/// - `Ok(Vec<u8>)`: Returns a byte vector containing the file data
///   if the file content was successfully read
/// - `Err(EpubError)`: The file does not exist or an error occurred during the read operation
///
/// ## Notes
/// - The returned data is raw bytes; the caller needs to perform
///   appropriate decoding based on the file type.
/// - For text files, further decoding using the `DecodeBytes` trait is usually required.
pub fn get_file_in_zip_archive<R: Read + Seek>(
    zip_file: &mut ZipArchive<R>,
    file_name: &str,
) -> Result<Vec<u8>, EpubError> {
    let mut buffer = Vec::<u8>::new();
    match zip_file.by_name(file_name) {
        Ok(mut file) => {
            let _ = file.read_to_end(&mut buffer).map_err(EpubError::from)?;
            Ok(buffer)
        }
        Err(err) => Err(EpubError::from(err)),
    }
}

/// Checks if the compression method of all entries in the EPUB file
/// conforms to the specification requirements.
///
/// According to the OCF (Open Container Format) specification, EPUB files
/// can only use either Stored (uncompressed) or Deflated (deflate compression).
/// If any other compression method is found, an error will be returned.
///
/// ## Parameters
/// - `zip_archive`: The ZIP archive to check.
///
/// ## Return
/// - `Ok(())`: All files use the supported compression method
/// - `Err(EpubError)`: Unsupported compression method found
///
/// ## Specification Reference
/// According to the EPUB OCF 3.2 specification: "OCF ZIP containers
/// MUST only use compression techniques that are supported
/// by the ZIP format specification (ISO/IEC 21320-1)"
/// Currently only Stored and Deflated methods are supported.
pub fn compression_method_check<R: Read + Seek>(
    zip_archive: &mut ZipArchive<R>,
) -> Result<(), EpubError> {
    for index in 0..zip_archive.len() {
        let file = zip_archive.by_index(index)?;

        match file.compression() {
            CompressionMethod::Stored | CompressionMethod::Deflated => continue,
            _ => {
                return Err(EpubError::UnusableCompressionMethod {
                    file: file.name().to_string(),
                    method: file.compression().to_string(),
                });
            }
        };
    }

    Ok(())
}

/// Check if relative link is outside the EPUB package scope
///
/// This function resolves relative path links and checks if they "leak"
/// outside the EPUB package structure. It determines the depth of upward
/// navigation by calculating the level of "../", and then verifies that
/// the final path is still within the EPUB package scope.
///
/// ## Parameters
/// - `epub_path`: The root path of the EPUB file
/// - `current_dir`: The directory path where the current file is located
/// - `check_file`: The relative path to check
///
/// ## Return
/// - `Some(String)`: The parsed normalized path string, if the link is within the EPUB package scope
/// - `None`: If the link is outside the EPUB package scope or an error occurs
pub fn check_realtive_link_leakage(
    epub_path: PathBuf,
    current_dir: PathBuf,
    check_file: &str,
) -> Option<String> {
    let mut folder_depth = 0;
    let mut remaining = check_file;

    // Count how many levels we need to go up
    while remaining.starts_with("../") {
        folder_depth += 1;
        remaining = &remaining[3..];
    }

    // Navigate up the directory tree according to folder_depth
    let mut current_path = epub_path.join(current_dir);
    for _ in 0..folder_depth {
        if !current_path.pop() {
            // failed to navigate up,
            // which means we're trying to escape the root directory
            return None;
        }
    }

    // verify that the resulting path is still within the EPUB package scope
    let prefix_path = match current_path.strip_prefix(&epub_path) {
        Ok(path) => path.to_str().unwrap(),
        Err(_) => return None, // path is outside the EPUB package scope
    };

    // construct the final path
    let path = match prefix_path {
        "" => remaining.to_string(),
        _ => format!("{}/{}", prefix_path, remaining),
    };
    Some(path)
}

/// Removes leading slash from a path
///
/// This function removes the leading slash from a path if it exists.
#[cfg(feature = "builder")]
pub fn remove_leading_slash<P: AsRef<Path>>(path: P) -> PathBuf {
    if let Ok(path) = path.as_ref().strip_prefix("/") {
        path.to_path_buf()
    } else {
        path.as_ref().to_path_buf()
    }
}

/// Encrypts the font file using the IDPF font obfuscation algorithm
///
/// The IDPF font obfuscation algorithm XORs the first 1040 bytes of the font file
/// with the publication's unique identifier. Due to the integrability of the XOR
/// operation (A XOR B XOR B = A), encryption and decryption use the same algorithm.
///
/// ## Parameters
/// - `data`: Original font data
/// - `key`: The unique identifier of the EPUB publication
///
/// ## Return
/// - `Vec<u8>`: Encrypted font data
///
/// ## Notes
/// - This function applies to the IDPF font obfuscation algorithm
///   (http://www.idpf.org/2008/embedding).
/// - Only processes the first 1040 bytes of the font file; the rest remains unchanged.
pub fn idpf_font_encryption(data: &[u8], key: &str) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }

    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    let hash = hasher.finalize();

    let mut key = vec![0u8; 1040];
    for index in 0..1040 {
        key[index] = hash[index % hash.len()];
    }

    let mut obfuscated_data = data.to_vec();
    for index in 0..min(1040, data.len()) {
        obfuscated_data[index] ^= key[index];
    }

    obfuscated_data
}

/// Decrypts a file encrypted using the IDPF obfuscation algorithm
///
/// The IDPF font obfuscation algorithm XORs the first 1040 bytes of the font file
/// with the publication's unique identifier. Due to the integrability of the XOR
/// operation (A XOR B XOR B = A), encryption and decryption use the same algorithm.
///
/// ## Parameters
/// - `data`: Original font data
/// - `key`: The unique identifier of the EPUB publication
///
/// ## Return
/// - `Vec<u8>`: Decrypted font data
pub fn idpf_font_dencryption(data: &[u8], key: &str) -> Vec<u8> {
    idpf_font_encryption(data, key)
}

/// Encrypts the font file using the Adobe font obfuscation algorithm
///
/// The Adobe font obfuscation algorithm XORs the first 1024 bytes of the font file
/// with a 16-byte key derived from the publication's unique identifier. Due to the
/// integrability of the XOR operation (A XOR B XOR B = A), encryption and decryption
/// use the same algorithm.
///
/// ## Parameters
/// - `data`: Original font data to be obfuscated
/// - `key`: The unique identifier of the EPUB publication
///
/// ## Return
/// - `Vec<u8>`: Obfuscated font data
///
/// ## Notes
/// - This function applies to the adobe font obfuscation algorithm
///   (http://ns.adobe.com/pdf/enc#RC).
/// - Only processes the first 1024 bytes of the font file; the rest remains unchanged.
pub fn adobe_font_encryption(data: &[u8], key: &str) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }

    let mut key_vec = key.as_bytes().to_vec();
    while key_vec.len() < 16 {
        key_vec.extend_from_slice(key.as_bytes());
    }

    let key = &key_vec[0..min(16, key_vec.len())];

    let mut obfuscated_data = data.to_vec();
    for index in 0..min(1024, data.len()) {
        obfuscated_data[index] ^= key[index % 16];
    }

    obfuscated_data
}

/// Decrypts a file encrypted using the Adobe font obfuscation algorithm
///
/// The Adobe font obfuscation algorithm XORs the first 1024 bytes of the font file
/// with a 16-byte key derived from the publication's unique identifier. Due to the
/// integrability of the XOR operation (A XOR B XOR B = A), encryption and decryption
/// use the same algorithm.
///
/// ## Parameters
/// - `data`: Obfuscated font data
/// - `key`: The unique identifier of the EPUB publication
///
/// ## Return
/// - `Vec<u8>`: Deobfuscated font data
pub fn adobe_font_dencryption(data: &[u8], key: &str) -> Vec<u8> {
    adobe_font_encryption(data, key)
}

mod unused_method {
    #![allow(dead_code)]

    use super::*;

    /// Encrypts data using the XML Encryption AES-128-CBC algorithm
    ///
    /// This function encrypts the provided data using the AES-128 algorithm
    /// in CBC mode, following the XML Encryption specification.
    ///
    /// ## Parameters
    /// - `data`: The raw byte data to encrypt
    /// - `key`: The encryption key string which will be processed to
    ///   generate the actual encryption key
    ///
    /// ## Return
    /// - `Vec<u8>`: The encrypted data
    ///
    /// ## Notes
    /// - Uses SHA-256 hashing to derive a 16-byte key from the provided key string
    /// - Implements http://www.w3.org/2001/04/xmlenc#aes128-cbc algorithm
    pub fn xml_encryption_aes128_cbc(data: &[u8], key: &str) -> Vec<u8> {
        xml_encryotion_algorithm(data, key, 16)
    }

    /// Decrypts data using the XML Encryption AES-128-CBC algorithm
    ///
    /// This function decrypts the provided data using the AES-128 algorithm
    /// in CBC mode, following the XML Encryption specification.
    ///
    /// ## Parameters
    /// - `data`: The encrypted byte data to decrypt
    /// - `key`: The decryption key string which will be processed to
    ///   generate the actual decryption key
    ///
    /// ## Return
    /// - `Vec<u8>`: The decrypted data
    pub fn xml_decryption_aes128_cbc(data: &[u8], key: &str) -> Vec<u8> {
        xml_encryotion_algorithm(data, key, 16)
    }

    /// Encrypts data using the XML Encryption AES-192-CBC algorithm
    ///
    /// This function encrypts the provided data using the AES-192 algorithm
    /// in CBC mode, following the XML Encryption specification.
    ///
    /// ## Parameters
    /// - `data`: The raw byte data to encrypt
    /// - `key`: The encryption key string which will be processed to
    ///   generate the actual encryption key
    ///
    /// ## Return
    /// - `Vec<u8>`: The encrypted data
    ///
    /// ## Notes
    /// - Uses SHA-256 hashing to derive a 24-byte key from the provided key string
    /// - Implements http://www.w3.org/2001/04/xmlenc#aes192-cbc algorithm
    pub fn xml_encryption_aes192_cbc(data: &[u8], key: &str) -> Vec<u8> {
        xml_encryotion_algorithm(data, key, 24)
    }

    /// Decrypts data using the XML Encryption AES-192-CBC algorithm
    ///
    /// This function decrypts the provided data using the AES-192 algorithm
    /// in CBC mode, following the XML Encryption specification.
    ///
    /// ## Parameters
    /// - `data`: The encrypted byte data to decrypt
    /// - `key`: The decryption key string which will be processed to
    ///   generate the actual decryption key
    ///
    /// ## Return
    /// - `Vec<u8>`: The decrypted data
    pub fn xml_decryption_aes192_cbc(data: &[u8], key: &str) -> Vec<u8> {
        xml_encryotion_algorithm(data, key, 24)
    }

    /// Encrypts data using the XML Encryption AES-256-CBC algorithm
    ///
    /// This function encrypts the provided data using the AES-256 algorithm
    /// in CBC mode, following the XML Encryption specification.
    ///
    /// ## Parameters
    /// - `data`: The raw byte data to encrypt
    /// - `key`: The encryption key string which will be processed to
    ///   generate the actual encryption key
    ///
    /// ## Return
    /// - `Vec<u8>`: The encrypted data
    ///
    /// ## Notes
    /// - Uses SHA-256 hashing to derive a 32-byte key from the provided key string
    /// - Implements http://www.w3.org/2001/04/xmlenc#aes256-cbc algorithm
    pub fn xml_encryption_aes256_cbc(data: &[u8], key: &str) -> Vec<u8> {
        xml_encryotion_algorithm(data, key, 32)
    }

    /// Decrypts data using the XML Encryption AES-256-CBC algorithm
    ///
    /// This function decrypts the provided data using the AES-256 algorithm
    /// in CBC mode, following the XML Encryption specification.
    ///
    /// ## Parameters
    /// - `data`: The encrypted byte data to decrypt
    /// - `key`: The decryption key string which will be processed to
    ///   generate the actual decryption key
    ///
    /// ## Return
    /// - `Vec<u8>`: The decrypted data
    pub fn xml_decryption_aes256_cbc(data: &[u8], key: &str) -> Vec<u8> {
        xml_encryotion_algorithm(data, key, 32)
    }

    /// Internal helper function for XML encryption/decryption operations
    ///
    /// This function performs XOR-based encryption/decryption on the provided data
    /// using a key derived from the provided key string via SHA-256 hashing.
    ///
    /// ## Parameters
    /// - `data`: The raw byte data to process
    /// - `key`: The key string which will be processed to generate the actual encryption/decryption key
    /// - `key_size`: The desired size of the key in bytes (16 for AES-128, 24 for AES-192, 32 for AES-256)
    ///
    /// ## Return
    /// - `Vec<u8>`: The processed data (encrypted or decrypted)
    fn xml_encryotion_algorithm(data: &[u8], key: &str, key_size: usize) -> Vec<u8> {
        if data.is_empty() {
            return Vec::new();
        }

        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        let hash = hasher.finalize();

        let ecryption_key = &hash[..min(key_size, hash.len())];

        data.iter()
            .enumerate()
            .map(|(index, &byte)| byte ^ ecryption_key[index % key_size])
            .collect()
    }
}

/// Provides functionality to decode byte data into strings
///
/// This trait is primarily used to decode raw byte data (such as
/// text files read from EPUB files) into a suitable string representation.
/// It supports automatic detection of multiple encoding formats,
/// including UTF-8 (with or without BOM), UTF-16 BE, and UTF-16 LE.
///
/// ## Implementation
/// Currently, this trait is implemented for the `Vec<u8>` type,
/// primarily used for processing text content in EPUB files.
///
/// ## Notes
/// - When attempting to parse a byte stream lacking a BOM (Byte Order Mark), the parsing
///   results may be unreadable; caution should be exercised when using such streams.
pub trait DecodeBytes {
    fn decode(&self) -> Result<String, EpubError>;
}

impl DecodeBytes for Vec<u8> {
    fn decode(&self) -> Result<String, EpubError> {
        if self.is_empty() || self.len() < 4 {
            return Err(EpubError::EmptyDataError);
        }

        match self[0..3] {
            // Check UTF-8 BOM (0xEF, 0xBB, 0xBF)
            [0xEF, 0xBB, 0xBF, ..] => {
                String::from_utf8(self[3..].to_vec()).map_err(EpubError::from)
            }

            // Check UTF-16 BE BOM (0xFE, 0xFF)
            [0xFE, 0xFF, ..] => {
                let utf16_bytes = &self[2..];
                let utf16_units: Vec<u16> = utf16_bytes
                    .chunks_exact(2)
                    .map(|b| u16::from_be_bytes([b[0], b[1]]))
                    .collect();

                String::from_utf16(&utf16_units).map_err(EpubError::from)
            }

            // Check UTF-16 LE BOM (0xFF, 0xFE)
            [0xFF, 0xFE, ..] => {
                let utf16_bytes = &self[2..];
                let utf16_units: Vec<u16> = utf16_bytes
                    .chunks_exact(2)
                    .map(|b| u16::from_le_bytes([b[0], b[1]]))
                    .collect();

                String::from_utf16(&utf16_units).map_err(EpubError::from)
            }

            // Try without BOM
            // The analytical results for this branch are unpredictable,
            // making it difficult to cover all possibilities when testing it.
            _ => {
                if let Ok(utf8_str) = String::from_utf8(self.to_vec()) {
                    return Ok(utf8_str);
                }

                if self.len() % 2 == 0 {
                    let utf16_units: Vec<u16> = self
                        .chunks_exact(2)
                        .map(|b| u16::from_be_bytes([b[0], b[1]]))
                        .collect();

                    if let Ok(utf16_str) = String::from_utf16(&utf16_units) {
                        return Ok(utf16_str);
                    }
                }

                if self.len() % 2 == 0 {
                    let utf16_units: Vec<u16> = self
                        .chunks_exact(2)
                        .map(|b| u16::from_le_bytes([b[0], b[1]]))
                        .collect();

                    if let Ok(utf16_str) = String::from_utf16(&utf16_units) {
                        return Ok(utf16_str);
                    }
                }

                // Final fallback
                Ok(String::from_utf8_lossy(self).to_string())
            }
        }
    }
}

/// Provides functionality for normalizing whitespace characters
///
/// This trait normalizes various sequences of whitespace characters
/// (including spaces, tabs, newlines, etc.) in a string into a single
/// whitespace character, removing leading and trailing whitespace characters.
///
/// ## Implementation
/// This trait is implemented for both `&str` and `String` types.
pub trait NormalizeWhitespace {
    fn normalize_whitespace(&self) -> String;
}

impl NormalizeWhitespace for &str {
    fn normalize_whitespace(&self) -> String {
        self.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

impl NormalizeWhitespace for String {
    fn normalize_whitespace(&self) -> String {
        self.as_str().normalize_whitespace()
    }
}

/// Represents an element node in an XML document
#[derive(Debug)]
pub struct XmlElement {
    /// The local name of the element(excluding namespace prefix)
    pub name: String,

    /// The namespace prefix of the element
    pub prefix: Option<String>,

    /// The namespace of the element
    pub namespace: Option<String>,

    /// The attributes of the element
    ///
    /// The key is the attribute name, the value is the attribute value
    pub attributes: HashMap<String, String>,

    /// The text content of the element
    pub text: Option<String>,

    /// The CDATA content of the element
    pub cdata: Option<String>,

    /// The children of the element
    pub children: Vec<XmlElement>,
}

impl XmlElement {
    /// Create a new element
    pub fn new(name: String) -> Self {
        Self {
            name,
            prefix: None,
            namespace: None,
            attributes: HashMap::new(),
            text: None,
            cdata: None,
            children: Vec::new(),
        }
    }

    /// Get the full tag name of the element
    ///
    /// If the element has a namespace prefix, return "prefix:name" format;
    /// otherwise, return only the element name.
    pub fn tag_name(&self) -> String {
        if let Some(prefix) = &self.prefix {
            format!("{}:{}", prefix, self.name)
        } else {
            self.name.clone()
        }
    }

    /// Gets the text content of the element and all its child elements
    ///
    /// Collects the text content of the current element and the text content of
    /// all its child elements, removing leading and trailing whitespace.
    pub fn text(&self) -> String {
        let mut result = String::new();

        if let Some(text_value) = &self.text {
            result.push_str(text_value);
        }

        for child in &self.children {
            result.push_str(&child.text());
        }

        result.trim().to_string()
    }

    /// Returns the value of the specified attribute
    pub fn get_attr(&self, name: &str) -> Option<String> {
        self.attributes.get(name).cloned()
    }

    /// Find all elements with the specified name
    pub fn find_elements_by_name(&self, name: &str) -> impl Iterator<Item = &XmlElement> {
        SearchElementsByNameIter::new(self, name)
    }

    /// Find all elements with the specified name among the child elements of the current element
    pub fn find_children_by_name(&self, name: &str) -> impl Iterator<Item = &XmlElement> {
        self.children.iter().filter(move |child| child.name == name)
    }

    /// Find all elements with the specified name list among the child elements of the current element
    pub fn find_children_by_names(&self, names: &[&str]) -> impl Iterator<Item = &XmlElement> {
        self.children
            .iter()
            .filter(move |child| names.contains(&child.name.as_str()))
    }

    /// Get children elements
    pub fn children(&self) -> impl Iterator<Item = &XmlElement> {
        self.children.iter()
    }
}

struct SearchElementsByNameIter<'a> {
    elements: Vec<&'a XmlElement>,
    current_index: usize,
    target_name: String,
}

impl<'a> SearchElementsByNameIter<'a> {
    fn new(root: &'a XmlElement, name: &str) -> Self {
        let mut elements = Vec::new();
        Self::collect_elements(root, &mut elements);
        Self {
            elements,
            current_index: 0,
            target_name: name.to_string(),
        }
    }

    fn collect_elements(element: &'a XmlElement, collection: &mut Vec<&'a XmlElement>) {
        collection.push(element);
        for child in &element.children {
            Self::collect_elements(child, collection);
        }
    }
}

impl<'a> Iterator for SearchElementsByNameIter<'a> {
    type Item = &'a XmlElement;

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_index < self.elements.len() {
            let element = self.elements[self.current_index];
            self.current_index += 1;
            if element.name == self.target_name {
                return Some(element);
            }
        }
        None
    }
}

/// XML parser used to parse XML content and build an XML element tree
pub struct XmlReader {}

#[allow(unused)]
impl XmlReader {
    /// Parses an XML from string and builds the root element
    ///
    /// This function takes an XML string, parses its content using the `quick_xml` library,
    /// and builds an `XmlElement` tree representing the structure of the entire XML document.
    ///
    /// ## Parameters
    /// - `content`: The XML string to be parsed
    ///
    /// ## Return
    /// - `Ok(XmlElement)`: The root element of the XML element tree
    /// - `Err(EpubError)`: An error occurred during parsing
    pub fn parse(content: &str) -> Result<XmlElement, EpubError> {
        if content.is_empty() {
            return Err(EpubError::EmptyDataError);
        }

        // Create a XML reader with namespace support
        let mut reader = NsReader::from_str(content);
        reader.config_mut().trim_text(true);

        let mut buf = Vec::new();
        let mut stack = Vec::<XmlElement>::new();
        let mut root = None;
        let mut namespace_map = HashMap::new();

        // Read XML events
        loop {
            match reader.read_event_into(&mut buf) {
                // End of file, stop the loop
                Ok(Event::Eof) => break,

                // Start of an element
                Ok(Event::Start(e)) => {
                    let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                    let mut element = XmlElement::new(name);

                    if let Some(prefix) = e.name().prefix() {
                        element.prefix = Some(String::from_utf8_lossy(prefix.as_ref()).to_string());
                    }

                    for attr in e.attributes().flatten() {
                        let attr_key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                        let attr_value = String::from_utf8_lossy(&attr.value).to_string();

                        // Handle namespace attributes
                        if attr_key.contains("xmlns") {
                            let attr_keys = attr_key.split(":").collect::<Vec<&str>>();
                            if attr_keys.len() >= 2 {
                                namespace_map.insert(attr_keys[1].to_string(), attr_value);
                            } else {
                                namespace_map.insert(attr_key, attr_value);
                            }

                            continue;
                        }

                        element.attributes.insert(attr_key, attr_value);
                    }

                    stack.push(element);
                }

                // End of an element
                Ok(Event::End(_)) => {
                    if let Some(element) = stack.pop() {
                        // If the stack is empty,
                        // the current element is the root element
                        if stack.is_empty() {
                            root = Some(element);
                        } else if let Some(parent) = stack.last_mut() {
                            // If the stack is not empty,
                            // the current element is a child element of the last element in the stack
                            parent.children.push(element);
                        }
                    }
                }

                // Self-closing element
                Ok(Event::Empty(e)) => {
                    let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                    let mut element = XmlElement::new(name);

                    if let Some(prefix) = e.name().prefix() {
                        element.prefix = Some(String::from_utf8_lossy(prefix.as_ref()).to_string());
                    }

                    for attr in e.attributes().flatten() {
                        let attr_key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                        let attr_value = String::from_utf8_lossy(&attr.value).to_string();

                        if attr_key.contains("xmlns") {
                            let attr_keys = attr_key.split(":").collect::<Vec<&str>>();
                            if attr_keys.len() >= 2 {
                                namespace_map.insert(attr_keys[1].to_string(), attr_value);
                            } else {
                                namespace_map.insert(attr_key, attr_value);
                            }

                            continue;
                        }

                        element.attributes.insert(attr_key, attr_value);
                    }

                    // We can almost certainly assert that a self-closing element cannot be
                    // the root node of an XML file, so this will definitely be executed.
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(element);
                    }
                }

                // Text node
                Ok(Event::Text(e)) => {
                    if let Some(element) = stack.last_mut() {
                        let text = String::from_utf8_lossy(e.as_ref()).to_string();
                        if !text.trim().is_empty() {
                            element.text = Some(text);
                        }
                    }
                }

                // CDATA node
                Ok(Event::CData(e)) => {
                    if let Some(element) = stack.last_mut() {
                        element.cdata = Some(String::from_utf8_lossy(e.as_ref()).to_string());
                    }
                }

                Err(err) => return Err(err.into()),

                // Ignore the following events (elements):
                // Comment, PI, Declaration, Doctype, GeneralRef
                _ => continue,
            }
        }

        if let Some(element) = root.as_mut() {
            Self::assign_namespace(element, &namespace_map);
        }

        // TODO: handle this error with a proper error
        root.ok_or(EpubError::EmptyDataError)
    }

    /// Parse XML from bytes and builds the root element
    pub fn parse_bytes(bytes: Vec<u8>) -> Result<XmlElement, EpubError> {
        let content = bytes.decode()?;
        Self::parse(&content)
    }

    /// Assign namespace to element recursively
    ///
    /// ## Parameters
    /// - `element`: The element to assign namespace
    /// - `namespace_map`: The prefix-namespace map
    fn assign_namespace(element: &mut XmlElement, namespace_map: &HashMap<String, String>) {
        if let Some(prefix) = &element.prefix {
            if let Some(namespace) = namespace_map.get(prefix) {
                element.namespace = Some(namespace.clone());
            }
        } else if let Some(namespace) = namespace_map.get("xmlns") {
            element.namespace = Some(namespace.clone());
        }

        for chiled in element.children.iter_mut() {
            Self::assign_namespace(chiled, namespace_map);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        error::EpubError,
        utils::{DecodeBytes, NormalizeWhitespace},
    };

    /// Test with empty data
    #[test]
    fn test_decode_empty_data() {
        let data = vec![];
        let result = data.decode();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), EpubError::EmptyDataError);
    }

    /// Test data with a length of less than 4 bytes
    #[test]
    fn test_decode_short_data() {
        let data = vec![0xEF, 0xBB];
        let result = data.decode();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), EpubError::EmptyDataError);
    }

    /// Testing text decoding with UTF-8 BOM
    #[test]
    fn test_decode_utf8_with_bom() {
        let data: Vec<u8> = vec![0xEF, 0xBB, 0xBF, b'H', b'e', b'l', b'l', b'o'];
        let result = data.decode();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello");
    }

    /// Test text decoding with UTF-16 BE BOM
    #[test]
    fn test_decode_utf16_be_with_bom() {
        let data = vec![
            0xFE, 0xFF, // BOM
            0x00, b'H', // H
            0x00, b'e', // e
            0x00, b'l', // l
            0x00, b'l', // l
            0x00, b'o', // o
        ];
        let result = data.decode();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello");
    }

    /// Testing text decoding with UTF-16 LE BOM
    #[test]
    fn test_decode_utf16_le_with_bom() {
        let data = vec![
            0xFF, 0xFE, // BOM
            b'H', 0x00, // H
            b'e', 0x00, // e
            b'l', 0x00, // l
            b'l', 0x00, // l
            b'o', 0x00, // o
        ];
        let result = data.decode();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello");
    }

    /// Testing ordinary UTF-8 text (without BOM)
    #[test]
    fn test_decode_plain_utf8() {
        let data = b"Hello, World!".to_vec();
        let result = data.decode();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello, World!");
    }

    /// Test text standardization containing various whitespace characters
    #[test]
    fn test_normalize_whitespace_trait() {
        // Test for &str
        let text = "  Hello,\tWorld!\n\nRust  ";
        let normalized = text.normalize_whitespace();
        assert_eq!(normalized, "Hello, World! Rust");

        // Test for String
        let text_string = String::from("  Hello,\tWorld!\n\nRust  ");
        let normalized = text_string.normalize_whitespace();
        assert_eq!(normalized, "Hello, World! Rust");
    }
}
