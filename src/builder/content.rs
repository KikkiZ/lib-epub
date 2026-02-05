//! Content Builder
//!
//! This module provides functionality for creating EPUB content documents.
//!
//! ## Usage
//! ``` rust, no_run
//! # #[cfg(feature = "content_builder")] {
//! # fn main() -> Result<(), lib_epub::error::EpubError> {
//! use lib_epub::{
//!     builder::content::{Block, BlockBuilder, ContentBuilder},
//!     types::{BlockType, Footnote},
//! };
//!
//! let mut block_builder = BlockBuilder::new(BlockType::Title);
//! block_builder
//!     .set_content("This is a title")
//!     .add_footnote(Footnote {
//!         locate: 15,
//!         content: "This is a footnote.".to_string(),
//!     });
//! let block = block_builder.build()?;
//!
//! let mut builder = ContentBuilder::new("chapter1", "zh-CN")?;
//! builder.set_title("My Chapter")
//!     .add_block(block)?
//!     .add_text_block("This is my first chapter.", vec![])?;
//! let _ = builder.make("output.xhtml")?;
//! # Ok(())
//! # }
//! # }
//! ```
//!
//! ## Future Work
//! - Support more types of content `Block`

use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

use infer::{Infer, MatcherType};
use log::warn;
use quick_xml::{
    Reader, Writer,
    events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event},
};

use crate::{
    builder::XmlWriter,
    error::{EpubBuilderError, EpubError},
    types::{BlockType, Footnote},
    utils::local_time,
};

/// Content Block
///
/// The content block is the basic unit of content in a content document.
/// It can be one of the following types: Text, Quote, Title, Image, Audio, Video, MathML.
#[non_exhaustive]
#[derive(Debug)]
pub enum Block {
    /// Text paragraph
    ///
    /// This block represents a paragraph of text.
    #[non_exhaustive]
    Text {
        content: String,
        footnotes: Vec<Footnote>,
    },

    /// Quote paragraph
    ///
    /// This block represents a paragraph of quoted text.
    #[non_exhaustive]
    Quote {
        content: String,
        footnotes: Vec<Footnote>,
    },

    /// Heading
    #[non_exhaustive]
    Title {
        content: String,
        footnotes: Vec<Footnote>,

        /// Heading level
        ///
        /// The valid range is 1 to 6.
        level: usize,
    },

    /// Image block
    #[non_exhaustive]
    Image {
        /// Image file path
        url: PathBuf,

        /// Alternative text for the image
        alt: Option<String>,

        /// Caption for the image
        caption: Option<String>,

        footnotes: Vec<Footnote>,
    },

    /// Audio block
    #[non_exhaustive]
    Audio {
        /// Audio file path
        url: PathBuf,

        /// Fallback text for the audio
        ///
        /// This is used when the audio file cannot be played.
        fallback: String,

        /// Caption for the audio
        caption: Option<String>,

        footnotes: Vec<Footnote>,
    },

    /// Video block
    #[non_exhaustive]
    Video {
        /// Video file path
        url: PathBuf,

        /// Fallback text for the video
        ///
        /// This is used when the video file cannot be played.
        fallback: String,

        /// Caption for the video
        caption: Option<String>,

        footnotes: Vec<Footnote>,
    },

    /// MathML block
    #[non_exhaustive]
    MathML {
        /// MathML element raw data
        ///
        /// This field stores the raw data of the MathML markup, which we do not verify,
        /// and the user needs to make sure it is correct.
        element_str: String,

        /// Fallback image for the MathML block
        ///
        /// This field stores the path to the fallback image, which will be displayed
        /// when the MathML markup cannot be rendered.
        fallback_image: Option<PathBuf>,

        /// Caption for the MathML block
        caption: Option<String>,

        footnotes: Vec<Footnote>,
    },
}

impl Block {
    /// Make the block
    ///
    /// Convert block data to xhtml markup.
    pub(crate) fn make(
        &mut self,
        writer: &mut XmlWriter,
        start_index: usize,
    ) -> Result<(), EpubError> {
        match self {
            Block::Text { content, footnotes } => {
                writer.write_event(Event::Start(
                    BytesStart::new("p").with_attributes([("class", "content-block")]),
                ))?;

                Self::make_text(writer, content, footnotes, start_index)?;

                writer.write_event(Event::End(BytesEnd::new("p")))?;
            }

            Block::Quote { content, footnotes } => {
                writer.write_event(Event::Start(BytesStart::new("blockquote").with_attributes(
                    [
                        ("class", "content-block"),
                        ("cite", "SOME ATTR NEED TO BE SET"),
                    ],
                )))?;
                writer.write_event(Event::Start(BytesStart::new("p")))?;

                Self::make_text(writer, content, footnotes, start_index)?;

                writer.write_event(Event::End(BytesEnd::new("p")))?;
                writer.write_event(Event::End(BytesEnd::new("blockquote")))?;
            }

            Block::Title { content, footnotes, level } => {
                let tag_name = format!("h{}", level);
                writer.write_event(Event::Start(
                    BytesStart::new(tag_name.as_str())
                        .with_attributes([("class", "content-block")]),
                ))?;

                Self::make_text(writer, content, footnotes, start_index)?;

                writer.write_event(Event::End(BytesEnd::new(tag_name)))?;
            }

            Block::Image { url, alt, caption, footnotes } => {
                let url = format!("./img/{}", url.file_name().unwrap().to_string_lossy());

                let mut attr = Vec::new();
                attr.push(("src", url.as_str()));
                attr.push(("class", "image-block"));
                if let Some(alt) = alt {
                    attr.push(("alt", alt.as_str()));
                }

                writer.write_event(Event::Start(
                    BytesStart::new("figure").with_attributes([("class", "content-block")]),
                ))?;
                writer.write_event(Event::Empty(BytesStart::new("img").with_attributes(attr)))?;

                if let Some(caption) = caption {
                    writer.write_event(Event::Start(BytesStart::new("figcaption")))?;

                    Self::make_text(writer, caption, footnotes, start_index)?;

                    writer.write_event(Event::End(BytesEnd::new("figcaption")))?;
                }

                writer.write_event(Event::End(BytesEnd::new("figure")))?;
            }

            Block::Audio { url, fallback, caption, footnotes } => {
                let url = format!("./audio/{}", url.file_name().unwrap().to_string_lossy());

                let attr = vec![
                    ("src", url.as_str()),
                    ("class", "audio-block"),
                    ("controls", "controls"), // attribute special spelling for xhtml
                ];

                writer.write_event(Event::Start(
                    BytesStart::new("figure").with_attributes([("class", "content-block")]),
                ))?;
                writer.write_event(Event::Start(BytesStart::new("audio").with_attributes(attr)))?;

                writer.write_event(Event::Start(BytesStart::new("p")))?;
                writer.write_event(Event::Text(BytesText::new(fallback.as_str())))?;
                writer.write_event(Event::End(BytesEnd::new("p")))?;

                writer.write_event(Event::End(BytesEnd::new("audio")))?;

                if let Some(caption) = caption {
                    writer.write_event(Event::Start(BytesStart::new("figcaption")))?;

                    Self::make_text(writer, caption, footnotes, start_index)?;

                    writer.write_event(Event::End(BytesEnd::new("figcaption")))?;
                }

                writer.write_event(Event::End(BytesEnd::new("figure")))?;
            }

            Block::Video { url, fallback, caption, footnotes } => {
                let url = format!("./video/{}", url.file_name().unwrap().to_string_lossy());

                let attr = vec![
                    ("src", url.as_str()),
                    ("class", "video-block"),
                    ("controls", "controls"), // attribute special spelling for xhtml
                ];

                writer.write_event(Event::Start(
                    BytesStart::new("figure").with_attributes([("class", "content-block")]),
                ))?;
                writer.write_event(Event::Start(BytesStart::new("video").with_attributes(attr)))?;

                writer.write_event(Event::Start(BytesStart::new("p")))?;
                writer.write_event(Event::Text(BytesText::new(fallback.as_str())))?;
                writer.write_event(Event::End(BytesEnd::new("p")))?;

                writer.write_event(Event::End(BytesEnd::new("video")))?;

                if let Some(caption) = caption {
                    writer.write_event(Event::Start(BytesStart::new("figcaption")))?;

                    Self::make_text(writer, caption, footnotes, start_index)?;

                    writer.write_event(Event::End(BytesEnd::new("figcaption")))?;
                }

                writer.write_event(Event::End(BytesEnd::new("figure")))?;
            }

            Block::MathML {
                element_str,
                fallback_image,
                caption,
                footnotes,
            } => {
                writer.write_event(Event::Start(
                    BytesStart::new("figure").with_attributes([("class", "content-block")]),
                ))?;

                Self::write_mathml_element(writer, element_str)?;

                if let Some(fallback_path) = fallback_image {
                    let img_url = format!(
                        "./img/{}",
                        fallback_path.file_name().unwrap().to_string_lossy()
                    );

                    writer.write_event(Event::Empty(BytesStart::new("img").with_attributes([
                        ("src", img_url.as_str()),
                        ("class", "mathml-fallback"),
                        ("alt", "Mathematical formula"),
                    ])))?;
                }

                if let Some(caption) = caption {
                    writer.write_event(Event::Start(BytesStart::new("figcaption")))?;

                    Self::make_text(writer, caption, footnotes, start_index)?;

                    writer.write_event(Event::End(BytesEnd::new("figcaption")))?;
                }

                writer.write_event(Event::End(BytesEnd::new("figure")))?;
            }
        }

        Ok(())
    }

    pub fn take_footnotes(&self) -> Vec<Footnote> {
        match self {
            Block::Text { footnotes, .. } => footnotes.to_vec(),
            Block::Quote { footnotes, .. } => footnotes.to_vec(),
            Block::Title { footnotes, .. } => footnotes.to_vec(),
            Block::Image { footnotes, .. } => footnotes.to_vec(),
            Block::Audio { footnotes, .. } => footnotes.to_vec(),
            Block::Video { footnotes, .. } => footnotes.to_vec(),
            Block::MathML { footnotes, .. } => footnotes.to_vec(),
        }
    }

    /// Split content by footnote locate
    ///
    /// ## Parameters
    /// - `content`: The content to split
    /// - `index_list`: The locations of footnotes
    fn split_content_by_index(content: &str, index_list: &[usize]) -> Vec<String> {
        if index_list.is_empty() {
            return vec![content.to_string()];
        }

        // index_list.len() footnote splits content into (index_list.len() + 1) parts.
        let mut result = Vec::with_capacity(index_list.len() + 1);
        let mut char_iter = content.chars().enumerate();

        let mut current_char_idx = 0;
        for &target_idx in index_list {
            let mut segment = String::new();

            // The starting range is the last location or 0,
            // and the ending range is the current location.
            while current_char_idx < target_idx {
                if let Some((_, ch)) = char_iter.next() {
                    segment.push(ch);
                    current_char_idx += 1;
                } else {
                    break;
                }
            }

            if !segment.is_empty() {
                result.push(segment);
            }
        }

        let remainder = char_iter.map(|(_, ch)| ch).collect::<String>();
        if !remainder.is_empty() {
            result.push(remainder);
        }

        result
    }

    /// Make text
    ///
    /// This function is used to format text content and footnote markup.
    ///
    /// ## Parameters
    /// - `writer`: The writer to write XML events
    /// - `content`: The text content to format
    /// - `footnotes`: The footnotes to format
    /// - `start_index`: The starting value of footnote number
    fn make_text(
        writer: &mut XmlWriter,
        content: &str,
        footnotes: &mut [Footnote],
        start_index: usize,
    ) -> Result<(), EpubError> {
        if footnotes.is_empty() {
            writer.write_event(Event::Text(BytesText::new(content)))?;
            return Ok(());
        }

        footnotes.sort_unstable();

        // statistical footnote locate and quantity
        let mut position_to_count = HashMap::new();
        for footnote in footnotes.iter() {
            *position_to_count.entry(footnote.locate).or_insert(0usize) += 1;
        }

        let mut positions = position_to_count.keys().copied().collect::<Vec<usize>>();
        positions.sort_unstable();

        let mut current_index = start_index;
        let content_list = Self::split_content_by_index(content, &positions);
        for (index, segment) in content_list.iter().enumerate() {
            writer.write_event(Event::Text(BytesText::new(segment)))?;

            // get the locate of the index-th footnote
            if let Some(&position) = positions.get(index) {
                // get the quantity of the index-th footnote
                if let Some(&count) = position_to_count.get(&position) {
                    for _ in 0..count {
                        Self::make_footnotes(writer, current_index)?;
                        current_index += 1;
                    }
                }
            }
        }

        Ok(())
    }

    /// Make footnote markup
    #[inline]
    fn make_footnotes(writer: &mut XmlWriter, index: usize) -> Result<(), EpubError> {
        writer.write_event(Event::Start(BytesStart::new("a").with_attributes([
            ("href", format!("#footnote-{}", index).as_str()),
            ("id", format!("ref-{}", index).as_str()),
            ("class", "footnote-ref"),
        ])))?;
        writer.write_event(Event::Text(BytesText::new(&format!("[{}]", index))))?;
        writer.write_event(Event::End(BytesEnd::new("a")))?;

        Ok(())
    }

    /// Write MathML element
    ///
    /// This function will parse the MathML element string and write it to the writer.
    fn write_mathml_element(writer: &mut XmlWriter, element_str: &str) -> Result<(), EpubError> {
        let mut reader = Reader::from_str(element_str);

        loop {
            match reader.read_event() {
                Ok(Event::Eof) => break,

                Ok(event) => writer.write_event(event)?,

                Err(err) => {
                    return Err(
                        EpubBuilderError::InvalidMathMLFormat { error: err.to_string() }.into(),
                    );
                }
            }
        }

        Ok(())
    }
}

/// Block Builder
///
/// A builder for constructing content blocks of various types.
///
/// ## Example
/// ```rust
/// # #[cfg(feature = "builder")]
/// # fn main() -> Result<(), lib_epub::error::EpubError> {
/// use lib_epub::{builder::content::BlockBuilder, types::{BlockType, Footnote}};
///
/// let mut builder = BlockBuilder::new(BlockType::Text);
/// builder.set_content("Hello, world!").add_footnote(Footnote {
///     content: "This is a footnote.".to_string(),
///     locate: 13,               
/// });
///
/// builder.build()?;
/// # Ok(())
/// # }
/// ```
///
/// ## Notes
/// - Not all fields are required for all block types. Required fields vary by block type.
/// - The `build()` method will validate that required fields are set for the specified block type.
pub struct BlockBuilder {
    /// The type of block to construct
    block_type: BlockType,

    /// Content text for Text, Quote, and Title blocks
    content: Option<String>,

    /// Heading level (1-6) for Title blocks
    level: Option<usize>,

    /// File path to media for Image, Audio, and Video blocks
    url: Option<PathBuf>,

    /// Alternative text for Image blocks
    alt: Option<String>,

    /// Caption text for Image, Audio, Video, and MathML blocks
    caption: Option<String>,

    /// Fallback text for Audio and Video blocks (displayed when media cannot be played)
    fallback: Option<String>,

    /// Raw MathML markup string for MathML blocks
    element_str: Option<String>,

    /// Fallback image path for MathML blocks (displayed when MathML cannot be rendered)
    fallback_image: Option<PathBuf>,

    /// Footnotes associated with the block content
    footnotes: Vec<Footnote>,
}

impl BlockBuilder {
    /// Creates a new BlockBuilder instance
    ///
    /// Initializes a BlockBuilder with the specified block type.
    ///
    /// ## Parameters
    /// - `block_type`: The type of block to construct
    pub fn new(block_type: BlockType) -> Self {
        Self {
            block_type,
            content: None,
            level: None,
            url: None,
            alt: None,
            caption: None,
            fallback: None,
            element_str: None,
            fallback_image: None,
            footnotes: vec![],
        }
    }

    /// Sets the text content of the block
    ///
    /// Used for Text, Quote, and Title block types.
    ///
    /// ## Parameters
    /// - `content`: The text content to set
    pub fn set_content(&mut self, content: &str) -> &mut Self {
        self.content = Some(content.to_string());
        self
    }

    /// Sets the heading level for a Title block
    ///
    /// Only applicable to Title block types. Valid range is 1 to 6.
    /// If the level is outside the valid range, this method silently ignores the setting
    /// and returns self unchanged.
    ///
    /// ## Parameters
    /// - `level`: The heading level (1-6), corresponding to h1-h6 HTML tags
    pub fn set_title_level(&mut self, level: usize) -> &mut Self {
        if !(1..=6).contains(&level) {
            return self;
        }

        self.level = Some(level);
        self
    }

    /// Sets the media file path
    ///
    /// Used for Image, Audio, and Video block types. This method validates that
    /// the file is a recognized image, audio, or video type.
    ///
    /// ## Parameters
    /// - `url`: The path to the media file
    ///
    /// ## Return
    /// - `Ok(&mut self)`: If the file type is valid
    /// - `Err(EpubError)`: The file does not exist or the file format is not image, audio, or video
    pub fn set_url(&mut self, url: &PathBuf) -> Result<&mut Self, EpubError> {
        match Self::is_target_type(
            url,
            vec![MatcherType::Image, MatcherType::Audio, MatcherType::Video],
        ) {
            Ok(_) => {
                self.url = Some(url.to_path_buf());
                Ok(self)
            }
            Err(err) => Err(err),
        }
    }

    /// Sets the alternative text for an image
    ///
    /// Only applicable to Image block types.
    /// Alternative text is displayed when the image cannot be loaded.
    ///
    /// ## Parameters
    /// - `alt`: The alternative text for the image
    pub fn set_alt(&mut self, alt: &str) -> &mut Self {
        self.alt = Some(alt.to_string());
        self
    }

    /// Sets the caption for the block
    ///
    /// Used for Image, Audio, Video, and MathML block types.
    /// The caption is displayed below the media or element.
    ///
    /// ## Parameters
    /// - `caption`: The caption text to display
    pub fn set_caption(&mut self, caption: &str) -> &mut Self {
        self.caption = Some(caption.to_string());
        self
    }

    /// Sets the fallback text for audio or video content
    ///
    /// Used for Audio and Video block types.
    /// The fallback text is displayed when the media file cannot be played.
    ///
    /// ## Parameters
    /// - `fallback`: The fallback text content
    pub fn set_fallback(&mut self, fallback: &str) -> &mut Self {
        self.fallback = Some(fallback.to_string());
        self
    }

    /// Sets the raw MathML element string
    ///
    /// Only applicable to MathML block types.
    /// This method accepts the raw MathML markup data without validation.
    /// The user is responsible for ensuring the MathML is well-formed.
    ///
    /// ## Parameters
    /// - `element_str`: The raw MathML markup string
    pub fn set_mathml_element(&mut self, element_str: &str) -> &mut Self {
        self.element_str = Some(element_str.to_string());
        self
    }

    /// Sets the fallback image for MathML content
    ///
    /// Only applicable to MathML block types.
    /// The fallback image is displayed when the MathML markup cannot be rendered.
    /// This method validates that the file is a recognized image type.
    ///
    /// ## Parameters
    /// - `fallback_image`: The path to the fallback image file
    ///
    /// ## Return
    /// - `Ok(self)`: If the file type is valid
    /// - `Err(EpubError)`: If validation fails
    pub fn set_fallback_image(&mut self, fallback_image: PathBuf) -> Result<&mut Self, EpubError> {
        match Self::is_target_type(&fallback_image, vec![MatcherType::Image]) {
            Ok(_) => {
                self.fallback_image = Some(fallback_image);
                Ok(self)
            }
            Err(err) => Err(err),
        }
    }

    /// Adds a footnote to the block
    ///
    /// Adds a single footnote to the block's footnotes collection.
    /// The footnote must reference a valid position within the content.
    ///
    /// ## Parameters
    /// - `footnote`: The footnote to add
    pub fn add_footnote(&mut self, footnote: Footnote) -> &mut Self {
        self.footnotes.push(footnote);
        self
    }

    /// Sets all footnotes for the block
    ///
    /// Replaces the current footnotes collection with the provided one.
    /// All footnotes must reference valid positions within the content.
    ///
    /// ## Parameters
    /// - `footnotes`: The vector of footnotes to set
    pub fn set_footnotes(&mut self, footnotes: Vec<Footnote>) -> &mut Self {
        self.footnotes = footnotes;
        self
    }

    /// Removes the last footnote
    ///
    /// Removes and discards the last footnote from the footnotes collection.
    /// If the collection is empty, this method has no effect.
    pub fn remove_last_footnote(&mut self) -> &mut Self {
        self.footnotes.pop();
        self
    }

    /// Clears all footnotes
    ///
    /// Removes all footnotes from the block's footnotes collection.
    pub fn clear_footnotes(&mut self) -> &mut Self {
        self.footnotes.clear();
        self
    }

    /// Builds the block
    ///
    /// Constructs a Block instance based on the configured parameters and block type.
    /// This method validates that all required fields are set for the specified block type
    /// and validates the footnotes to ensure they reference valid content positions.
    ///
    /// ## Return
    /// - `Ok(Block)`: Build successful
    /// - `Err(EpubError)`: Error occurred during the build process
    pub fn build(self) -> Result<Block, EpubError> {
        let block = match self.block_type {
            BlockType::Text => {
                if let Some(content) = self.content {
                    Block::Text { content, footnotes: self.footnotes }
                } else {
                    return Err(EpubBuilderError::MissingNecessaryBlockData {
                        block_type: "Text".to_string(),
                        missing_data: "'content'".to_string(),
                    }
                    .into());
                }
            }

            BlockType::Quote => {
                if let Some(content) = self.content {
                    Block::Quote { content, footnotes: self.footnotes }
                } else {
                    return Err(EpubBuilderError::MissingNecessaryBlockData {
                        block_type: "Quote".to_string(),
                        missing_data: "'content'".to_string(),
                    }
                    .into());
                }
            }

            BlockType::Title => match (self.content, self.level) {
                (Some(content), Some(level)) => Block::Title {
                    content,
                    level,
                    footnotes: self.footnotes,
                },
                _ => {
                    return Err(EpubBuilderError::MissingNecessaryBlockData {
                        block_type: "Title".to_string(),
                        missing_data: "'content' or 'level'".to_string(),
                    }
                    .into());
                }
            },

            BlockType::Image => {
                if let Some(url) = self.url {
                    Block::Image {
                        url,
                        alt: self.alt,
                        caption: self.caption,
                        footnotes: self.footnotes,
                    }
                } else {
                    return Err(EpubBuilderError::MissingNecessaryBlockData {
                        block_type: "Image".to_string(),
                        missing_data: "'url'".to_string(),
                    }
                    .into());
                }
            }

            BlockType::Audio => match (self.url, self.fallback) {
                (Some(url), Some(fallback)) => Block::Audio {
                    url,
                    fallback,
                    caption: self.caption,
                    footnotes: self.footnotes,
                },
                _ => {
                    return Err(EpubBuilderError::MissingNecessaryBlockData {
                        block_type: "Audio".to_string(),
                        missing_data: "'url' or 'fallback'".to_string(),
                    }
                    .into());
                }
            },

            BlockType::Video => match (self.url, self.fallback) {
                (Some(url), Some(fallback)) => Block::Video {
                    url,
                    fallback,
                    caption: self.caption,
                    footnotes: self.footnotes,
                },
                _ => {
                    return Err(EpubBuilderError::MissingNecessaryBlockData {
                        block_type: "Video".to_string(),
                        missing_data: "'url' or 'fallback'".to_string(),
                    }
                    .into());
                }
            },

            BlockType::MathML => {
                if let Some(element_str) = self.element_str {
                    Block::MathML {
                        element_str,
                        fallback_image: self.fallback_image,
                        caption: self.caption,
                        footnotes: self.footnotes,
                    }
                } else {
                    return Err(EpubBuilderError::MissingNecessaryBlockData {
                        block_type: "MathML".to_string(),
                        missing_data: "'element_str'".to_string(),
                    }
                    .into());
                }
            }
        };

        Self::validate_footnotes(&block)?;
        Ok(block)
    }

    /// Validates that the file type matches expected types
    ///
    /// Identifies the file type by reading the file header and validates whether
    /// it belongs to one of the expected types. Uses file magic numbers for
    /// reliable type detection.
    ///
    /// ## Parameters
    /// - `path`: The path to the file to check
    /// - `types`: The vector of expected file types
    fn is_target_type(path: &PathBuf, types: Vec<MatcherType>) -> Result<(), EpubError> {
        if !path.is_file() {
            return Err(EpubBuilderError::TargetIsNotFile {
                target_path: path.to_string_lossy().to_string(),
            }
            .into());
        }

        let mut file = File::open(path)?;
        let mut buf = [0; 512];
        let read_size = file.read(&mut buf)?;
        let header_bytes = &buf[..read_size];

        match Infer::new().get(header_bytes) {
            Some(file_type) if !types.contains(&file_type.matcher_type()) => {
                Err(EpubBuilderError::NotExpectedFileFormat.into())
            }

            None => Err(EpubBuilderError::UnknownFileFormat {
                file_path: path.to_string_lossy().to_string(),
            }
            .into()),

            _ => Ok(()),
        }
    }

    /// Validates the footnotes in a block
    ///
    /// Ensures all footnotes reference valid positions within the content.
    /// For Text, Quote, and Title blocks, footnotes must be within the character count of the content.
    /// For Image, Audio, Video, and MathML blocks, footnotes must be within the character count
    /// of the caption (if a caption is set). Blocks with media but no caption cannot have footnotes.
    fn validate_footnotes(block: &Block) -> Result<(), EpubError> {
        match block {
            Block::Text { content, footnotes }
            | Block::Quote { content, footnotes }
            | Block::Title { content, footnotes, .. } => {
                let max_locate = content.chars().count();
                for footnote in footnotes.iter() {
                    if footnote.locate == 0 || footnote.locate > content.chars().count() {
                        return Err(EpubBuilderError::InvalidFootnoteLocate { max_locate }.into());
                    }
                }

                Ok(())
            }

            Block::Image { caption, footnotes, .. }
            | Block::MathML { caption, footnotes, .. }
            | Block::Video { caption, footnotes, .. }
            | Block::Audio { caption, footnotes, .. } => {
                if let Some(caption) = caption {
                    let max_locate = caption.chars().count();
                    for footnote in footnotes.iter() {
                        if footnote.locate == 0 || footnote.locate > caption.chars().count() {
                            return Err(
                                EpubBuilderError::InvalidFootnoteLocate { max_locate }.into()
                            );
                        }
                    }
                } else if !footnotes.is_empty() {
                    return Err(EpubBuilderError::InvalidFootnoteLocate { max_locate: 0 }.into());
                }

                Ok(())
            }
        }
    }
}

/// Content Builder
///
/// A builder for constructing EPUB content documents with various block types.
/// This builder manages the creation and organization of content blocks including
/// text, quotes, headings, images, audio, video, and MathML content.
#[derive(Debug)]
pub struct ContentBuilder {
    /// The unique identifier for the content document
    ///
    /// This identifier is used to uniquely identify the content document within the EPUB container.
    /// If the identifier is not unique, only one content document will be included in the EPUB container;
    /// and the other content document will be ignored.  
    pub id: String,

    blocks: Vec<Block>,
    language: String,
    title: String,

    pub(crate) temp_dir: PathBuf,
}

impl ContentBuilder {
    /// Creates a new ContentBuilder instance
    ///
    /// Initializes a ContentBuilder with the specified language code.
    /// A temporary directory is automatically created to store media files during construction.
    ///
    /// ## Parameters
    /// - `language`: The language code for the document
    pub fn new(id: &str, language: &str) -> Result<Self, EpubError> {
        let temp_dir = env::temp_dir().join(local_time());
        fs::create_dir(&temp_dir)?;

        Ok(Self {
            id: id.to_string(),
            blocks: vec![],
            language: language.to_string(),
            title: String::new(),
            temp_dir,
        })
    }

    /// Sets the title of the document
    ///
    /// Sets the title that will be displayed in the document's head section.
    ///
    /// ## Parameters
    /// - `title`: The title text for the document
    pub fn set_title(&mut self, title: &str) -> &mut Self {
        self.title = title.to_string();
        self
    }

    /// Adds a block to the document
    ///
    /// Adds a constructed Block to the document.
    ///
    /// ## Parameters
    /// - `block`: The Block to add to the document
    pub fn add_block(&mut self, block: Block) -> Result<&mut Self, EpubError> {
        self.blocks.push(block);

        match self.blocks.last() {
            Some(Block::Image { .. }) | Some(Block::Audio { .. }) | Some(Block::Video { .. }) => {
                self.handle_resource()?
            }

            Some(Block::MathML { fallback_image, .. }) if fallback_image.is_some() => {
                self.handle_resource()?;
            }

            _ => {}
        }

        Ok(self)
    }

    /// Adds a text block to the document
    ///
    /// Convenience method that creates and adds a Text block using the provided content and footnotes.
    ///
    /// ## Parameters
    /// - `content`: The text content of the paragraph
    /// - `footnotes`: A vector of footnotes associated with the text
    pub fn add_text_block(
        &mut self,
        content: &str,
        footnotes: Vec<Footnote>,
    ) -> Result<&mut Self, EpubError> {
        let mut builder = BlockBuilder::new(BlockType::Text);
        builder.set_content(content).set_footnotes(footnotes);

        self.blocks.push(builder.build()?);
        Ok(self)
    }

    /// Adds a quote block to the document
    ///
    /// Convenience method that creates and adds a Quote block using the provided content and footnotes.
    ///
    /// ## Parameters
    /// - `content`: The quoted text
    /// - `footnotes`: A vector of footnotes associated with the quote
    pub fn add_quote_block(
        &mut self,
        content: &str,
        footnotes: Vec<Footnote>,
    ) -> Result<&mut Self, EpubError> {
        let mut builder = BlockBuilder::new(BlockType::Quote);
        builder.set_content(content).set_footnotes(footnotes);

        self.blocks.push(builder.build()?);
        Ok(self)
    }

    /// Adds a heading block to the document
    ///
    /// Convenience method that creates and adds a Title block with the specified level.
    ///
    /// ## Parameters
    /// - `content`: The heading text
    /// - `level`: The heading level (1-6), corresponding to h1-h6 HTML tags
    /// - `footnotes`: A vector of footnotes associated with the heading
    pub fn add_title_block(
        &mut self,
        content: &str,
        level: usize,
        footnotes: Vec<Footnote>,
    ) -> Result<&mut Self, EpubError> {
        let mut builder = BlockBuilder::new(BlockType::Title);
        builder
            .set_content(content)
            .set_title_level(level)
            .set_footnotes(footnotes);

        self.blocks.push(builder.build()?);
        Ok(self)
    }

    /// Adds an image block to the document
    ///
    /// Convenience method that creates and adds an Image block with optional alt text,
    /// caption, and footnotes.
    ///
    /// ## Parameters
    /// - `url`: The path to the image file
    /// - `alt`: Optional alternative text for the image (displayed when image cannot load)
    /// - `caption`: Optional caption text to display below the image
    /// - `footnotes`: A vector of footnotes associated with the caption or image
    pub fn add_image_block(
        &mut self,
        url: PathBuf,
        alt: Option<String>,
        caption: Option<String>,
        footnotes: Vec<Footnote>,
    ) -> Result<&mut Self, EpubError> {
        let mut builder = BlockBuilder::new(BlockType::Image);
        builder.set_url(&url)?.set_footnotes(footnotes);

        if let Some(alt) = &alt {
            builder.set_alt(alt);
        }

        if let Some(caption) = &caption {
            builder.set_caption(caption);
        }

        self.blocks.push(builder.build()?);
        self.handle_resource()?;
        Ok(self)
    }

    /// Adds an audio block to the document
    ///
    /// Convenience method that creates and adds an Audio block with fallback text,
    /// optional caption, and footnotes.
    ///
    /// ## Parameters
    /// - `url`: The path to the audio file
    /// - `fallback`: Fallback text displayed when the audio cannot be played
    /// - `caption`: Optional caption text to display below the audio player
    /// - `footnotes`: A vector of footnotes associated with the caption or audio
    pub fn add_audio_block(
        &mut self,
        url: PathBuf,
        fallback: String,
        caption: Option<String>,
        footnotes: Vec<Footnote>,
    ) -> Result<&mut Self, EpubError> {
        let mut builder = BlockBuilder::new(BlockType::Audio);
        builder
            .set_url(&url)?
            .set_fallback(&fallback)
            .set_footnotes(footnotes);

        if let Some(caption) = &caption {
            builder.set_caption(caption);
        }

        self.blocks.push(builder.build()?);
        self.handle_resource()?;
        Ok(self)
    }

    /// Adds a video block to the document
    ///
    /// Convenience method that creates and adds a Video block with fallback text,
    /// optional caption, and footnotes.
    ///
    /// ## Parameters
    /// - `url`: The path to the video file
    /// - `fallback`: Fallback text displayed when the video cannot be played
    /// - `caption`: Optional caption text to display below the video player
    /// - `footnotes`: A vector of footnotes associated with the caption or video
    pub fn add_video_block(
        &mut self,
        url: PathBuf,
        fallback: String,
        caption: Option<String>,
        footnotes: Vec<Footnote>,
    ) -> Result<&mut Self, EpubError> {
        let mut builder = BlockBuilder::new(BlockType::Video);
        builder
            .set_url(&url)?
            .set_fallback(&fallback)
            .set_footnotes(footnotes);

        if let Some(caption) = &caption {
            builder.set_caption(caption);
        }

        self.blocks.push(builder.build()?);
        self.handle_resource()?;
        Ok(self)
    }

    /// Adds a MathML block to the document
    ///
    /// Convenience method that creates and adds a MathML block with optional fallback image,
    /// caption, and footnotes.
    ///
    /// ## Parameters
    /// - `element_str`: The raw MathML markup string
    /// - `fallback_image`: Optional path to a fallback image displayed when MathML cannot render
    /// - `caption`: Optional caption text to display below the MathML element
    /// - `footnotes`: A vector of footnotes associated with the caption or equation
    pub fn add_mathml_block(
        &mut self,
        element_str: String,
        fallback_image: Option<PathBuf>,
        caption: Option<String>,
        footnotes: Vec<Footnote>,
    ) -> Result<&mut Self, EpubError> {
        let mut builder = BlockBuilder::new(BlockType::MathML);
        builder
            .set_mathml_element(&element_str)
            .set_footnotes(footnotes);

        if let Some(fallback_image) = fallback_image {
            builder.set_fallback_image(fallback_image)?;
        }

        if let Some(caption) = &caption {
            builder.set_caption(caption);
        }

        self.blocks.push(builder.build()?);
        self.handle_resource()?;
        Ok(self)
    }

    /// Removes the last block from the document
    ///
    /// Discards the most recently added block. If no blocks exist, this method has no effect.
    pub fn remove_last_block(&mut self) -> &mut Self {
        self.blocks.pop();
        self
    }

    /// Takes ownership of the last block
    ///
    /// Removes and returns the most recently added block without consuming the builder.
    /// This allows you to extract a block while keeping the builder alive.
    ///
    /// ## Return
    /// - `Some(Block)`: If a block exists
    /// - `None`: If the blocks collection is empty
    pub fn take_last_block(&mut self) -> Option<Block> {
        self.blocks.pop()
    }

    /// Clears all blocks from the document
    ///
    /// Removes all blocks from the document while keeping the language and title settings intact.
    pub fn clear_blocks(&mut self) -> &mut Self {
        self.blocks.clear();
        self
    }

    /// Builds content document
    ///
    /// ## Parameters
    /// - `target`: The file path where the document should be written
    ///
    /// ## Return
    /// - `Ok(Vec<PathBuf>)`: A vector of paths to all resources used in the document
    /// - `Err(EpubError)`: Error occurred during the making process
    pub fn make<P: AsRef<Path>>(&mut self, target: P) -> Result<Vec<PathBuf>, EpubError> {
        let mut result = Vec::new();

        // Handle target directory, create if it doesn't exist
        let target_dir = match target.as_ref().parent() {
            Some(path) => {
                fs::create_dir_all(path)?;
                path.to_path_buf()
            }
            None => {
                return Err(EpubBuilderError::InvalidTargetPath {
                    target_path: target.as_ref().to_string_lossy().to_string(),
                }
                .into());
            }
        };

        self.make_content(&target)?;
        result.push(target.as_ref().to_path_buf());

        // Copy all resource files (images, audio, video) from temp directory to target directory
        for resource_type in ["img", "audio", "video"] {
            let source = self.temp_dir.join(resource_type);
            if source.exists() && source.is_dir() {
                let target = target_dir.join(resource_type);
                fs::create_dir_all(&target)?;

                for entry in fs::read_dir(&source)? {
                    let entry = entry?;
                    if entry.file_type()?.is_file() {
                        let file_name = entry.file_name();
                        let target = target.join(&file_name);

                        fs::copy(source.join(&file_name), &target)?;
                        result.push(target);
                    }
                }
            }
        }

        Ok(result)
    }

    /// Write the document to a file
    ///
    /// Constructs the final XHTML document from all added blocks and writes it to the specified output path.
    ///
    /// ## Parameters
    /// - `target_path`: The file path where the XHTML document should be written
    fn make_content<P: AsRef<Path>>(&mut self, target_path: P) -> Result<(), EpubError> {
        let mut writer = Writer::new(Cursor::new(Vec::new()));

        writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;
        writer.write_event(Event::Start(BytesStart::new("html").with_attributes([
            ("xmlns", "http://www.w3.org/1999/xhtml"),
            ("xml:lang", self.language.as_str()),
        ])))?;

        // make head
        writer.write_event(Event::Start(BytesStart::new("head")))?;
        writer.write_event(Event::Start(BytesStart::new("title")))?;
        writer.write_event(Event::Text(BytesText::new(&self.title)))?;
        writer.write_event(Event::End(BytesEnd::new("title")))?;
        writer.write_event(Event::End(BytesEnd::new("head")))?;

        // make body
        writer.write_event(Event::Start(BytesStart::new("body")))?;

        let mut footnote_index = 1;
        let mut footnotes = Vec::new();
        for block in self.blocks.iter_mut() {
            block.make(&mut writer, footnote_index)?;

            footnotes.append(&mut block.take_footnotes());
            footnote_index = footnotes.len() + 1;
        }

        Self::make_footnotes(&mut writer, footnotes)?;
        writer.write_event(Event::End(BytesEnd::new("body")))?;
        writer.write_event(Event::End(BytesEnd::new("html")))?;

        let file_path = PathBuf::from(target_path.as_ref());
        let file_data = writer.into_inner().into_inner();
        fs::write(file_path, file_data)?;

        Ok(())
    }

    /// Generates the footnotes section in the document
    ///
    /// Creates an aside element containing an unordered list of all footnotes.
    /// Each footnote is rendered as a list item with a backlink to its reference in the text.
    fn make_footnotes(writer: &mut XmlWriter, footnotes: Vec<Footnote>) -> Result<(), EpubError> {
        writer.write_event(Event::Start(BytesStart::new("aside")))?;
        writer.write_event(Event::Start(BytesStart::new("ul")))?;

        let mut index = 1;
        for footnote in footnotes.into_iter() {
            writer.write_event(Event::Start(
                BytesStart::new("li")
                    .with_attributes([("id", format!("footnote-{}", index).as_str())]),
            ))?;
            writer.write_event(Event::Start(BytesStart::new("p")))?;

            writer.write_event(Event::Start(
                BytesStart::new("a")
                    .with_attributes([("href", format!("#ref-{}", index).as_str())]),
            ))?;
            writer.write_event(Event::Text(BytesText::new(&format!("[{}]", index,))))?;
            writer.write_event(Event::End(BytesEnd::new("a")))?;
            writer.write_event(Event::Text(BytesText::new(&footnote.content)))?;

            writer.write_event(Event::End(BytesEnd::new("p")))?;
            writer.write_event(Event::End(BytesEnd::new("li")))?;

            index += 1;
        }

        writer.write_event(Event::End(BytesEnd::new("ul")))?;
        writer.write_event(Event::End(BytesEnd::new("aside")))?;

        Ok(())
    }

    /// Automatically handles media resources
    fn handle_resource(&mut self) -> Result<(), EpubError> {
        match self.blocks.last() {
            Some(Block::Image { url, .. }) => {
                let target_dir = self.temp_dir.join("img");
                fs::create_dir_all(&target_dir)?;

                let target_path = target_dir.join(url.file_name().unwrap());
                fs::copy(url, &target_path)?;
            }

            Some(Block::Video { url, .. }) => {
                let target_dir = self.temp_dir.join("video");
                fs::create_dir_all(&target_dir)?;

                let target_path = target_dir.join(url.file_name().unwrap());
                fs::copy(url, &target_path)?;
            }

            Some(Block::Audio { url, .. }) => {
                let target_dir = self.temp_dir.join("audio");
                fs::create_dir_all(&target_dir)?;

                let target_path = target_dir.join(url.file_name().unwrap());
                fs::copy(url, &target_path)?;
            }

            Some(Block::MathML { fallback_image, .. }) if fallback_image.is_some() => {
                let target_dir = self.temp_dir.join("img");
                fs::create_dir_all(&target_dir)?;

                let target_path =
                    target_dir.join(fallback_image.as_ref().unwrap().file_name().unwrap());

                fs::copy(fallback_image.as_ref().unwrap(), &target_path)?;
            }

            Some(_) => {}
            None => {}
        }

        Ok(())
    }
}

impl Drop for ContentBuilder {
    fn drop(&mut self) {
        if let Err(err) = fs::remove_dir_all(&self.temp_dir) {
            warn!("{}", err);
        };
    }
}
