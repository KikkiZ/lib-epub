use std::{
    collections::HashMap,
    fs::{self, File},
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

use infer::{Infer, MatcherType};
use quick_xml::{
    Reader, Writer,
    events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event},
};

use crate::{
    builder::XmlWriter,
    error::{EpubBuilderError, EpubError},
    types::{BlockType, Footnote},
};

pub enum Block {
    /// 文本段落
    #[non_exhaustive]
    Text {
        content: String,
        footnotes: Vec<Footnote>,
    },

    /// 引用段落
    #[non_exhaustive]
    Quote {
        content: String,
        footnotes: Vec<Footnote>,
    },

    /// 标题段落
    #[non_exhaustive]
    Title {
        content: String,
        footnotes: Vec<Footnote>,
        level: usize,
    },

    /// 图片段落
    #[non_exhaustive]
    Image {
        url: PathBuf,
        alt: Option<String>,
        caption: Option<String>, // 图例
        footnotes: Vec<Footnote>,
    },

    /// 音频段落
    #[non_exhaustive]
    Audio {
        url: PathBuf,
        fallback: String, // 音频的替代文本
        caption: Option<String>,
        footnotes: Vec<Footnote>,
    },

    /// 视频段落
    #[non_exhaustive]
    Video {
        url: PathBuf,
        fallback: String, // 视频的替代文本
        caption: Option<String>,
        footnotes: Vec<Footnote>,
    },

    /// MathML 段落
    #[non_exhaustive]
    MathML {
        element_str: String,
        fallback_image: Option<PathBuf>,
        caption: Option<String>,
        footnotes: Vec<Footnote>,
    },
}

impl Block {
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

    fn split_content_by_index(content: &str, index_list: &[usize]) -> Vec<String> {
        if index_list.is_empty() {
            return vec![content.to_string()];
        }

        let mut result = Vec::with_capacity(index_list.len() + 1);
        let mut char_iter = content.chars().enumerate();

        // 优化：单次迭代，无需完整字符数组
        let mut current_char_idx = 0;
        for &target_idx in index_list {
            let mut segment = String::new();

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

            // 当前的locate，也即当前切片的结尾位置
            if let Some(&position) = positions.get(index) {
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

pub struct BlockBuilder {
    block_type: BlockType,
    content: Option<String>,
    level: Option<usize>,
    url: Option<PathBuf>,
    alt: Option<String>,
    caption: Option<String>,
    fallback: Option<String>,
    element_str: Option<String>,
    fallback_image: Option<PathBuf>,
    footnotes: Vec<Footnote>,
}

impl BlockBuilder {
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

    pub fn set_content(&mut self, content: &str) -> &mut Self {
        self.content = Some(content.to_string());
        self
    }

    pub fn set_title_level(&mut self, level: usize) -> &mut Self {
        if !(1..=6).contains(&level) {
            return self;
        }

        self.level = Some(level);
        self
    }

    pub fn set_url(&mut self, url: PathBuf) -> Result<&mut Self, EpubError> {
        match Self::is_target_type(
            &url,
            vec![MatcherType::Image, MatcherType::Audio, MatcherType::Video],
        ) {
            Ok(_) => {
                self.url = Some(url);
                Ok(self)
            }
            Err(err) => Err(err),
        }
    }

    pub fn set_alt(&mut self, alt: &str) -> &mut Self {
        self.alt = Some(alt.to_string());
        self
    }

    pub fn set_caption(&mut self, caption: &str) -> &mut Self {
        self.caption = Some(caption.to_string());
        self
    }

    pub fn set_fallback(&mut self, fallback: &str) -> &mut Self {
        self.fallback = Some(fallback.to_string());
        self
    }

    pub fn set_mathml_element(&mut self, element_str: &str) -> &mut Self {
        self.element_str = Some(element_str.to_string());
        self
    }

    pub fn set_fallback_image(&mut self, fallback_image: PathBuf) -> Result<&mut Self, EpubError> {
        match Self::is_target_type(&fallback_image, vec![MatcherType::Image]) {
            Ok(_) => {
                self.fallback_image = Some(fallback_image);
                Ok(self)
            }
            Err(err) => Err(err),
        }
    }

    pub fn add_footnote(&mut self, footnote: Footnote) -> &mut Self {
        self.footnotes.push(footnote);
        self
    }

    pub fn set_footnotes(&mut self, footnotes: Vec<Footnote>) -> &mut Self {
        self.footnotes = footnotes;
        self
    }

    pub fn remove_last_footnote(&mut self) -> &mut Self {
        self.footnotes.pop();
        self
    }

    pub fn clear_footnotes(&mut self) -> &mut Self {
        self.footnotes.clear();
        self
    }

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
            Some(file_type) if types.contains(&file_type.matcher_type()) => {
                Err(EpubBuilderError::NotExpectedFileFormat.into())
            }

            None => Err(EpubBuilderError::UnknownFileFormat {
                file_path: path.to_string_lossy().to_string(),
            }
            .into()),

            _ => Ok(()),
        }
    }

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

pub struct ContentBuilder {
    pub blocks: Vec<Block>,
    language: String,
    title: String,
}

impl ContentBuilder {
    pub fn new(language: &str) -> Self {
        Self {
            blocks: vec![],
            language: language.to_string(),
            title: String::new(),
        }
    }

    pub fn set_title(&mut self, title: &str) -> &mut Self {
        self.title = title.to_string();
        self
    }

    pub fn add_block(&mut self, block: Block) -> &mut Self {
        self.blocks.push(block);
        self
    }

    // TODO: 应该使用 BlockBuilder 来创建 Block
    pub fn add_text_block(&mut self, content: &str, footnotes: Vec<Footnote>) -> &mut Self {
        self.add_block(Block::Text { content: content.to_string(), footnotes })
    }

    pub fn add_quote_block(&mut self, content: &str, footnotes: Vec<Footnote>) -> &mut Self {
        self.add_block(Block::Quote { content: content.to_string(), footnotes })
    }

    pub fn add_title_block(
        &mut self,
        content: &str,
        level: usize,
        footnotes: Vec<Footnote>,
    ) -> &mut Self {
        self.add_block(Block::Title {
            content: content.to_string(),
            level,
            footnotes,
        })
    }

    pub fn add_image_block(
        &mut self,
        url: PathBuf,
        alt: Option<String>,
        caption: Option<String>,
        footnotes: Vec<Footnote>,
    ) -> &mut Self {
        self.add_block(Block::Image { url, alt, caption, footnotes })
    }

    pub fn add_audio_block(
        &mut self,
        url: PathBuf,
        fallback: String,
        caption: Option<String>,
        footnotes: Vec<Footnote>,
    ) -> &mut Self {
        self.add_block(Block::Audio { url, fallback, caption, footnotes })
    }

    pub fn add_video_block(
        &mut self,
        url: PathBuf,
        fallback: String,
        caption: Option<String>,
        footnotes: Vec<Footnote>,
    ) -> &mut Self {
        self.add_block(Block::Video { url, fallback, caption, footnotes })
    }

    pub fn add_mathml_block(
        &mut self,
        element_str: String,
        fallback_image: Option<PathBuf>,
        caption: Option<String>,
        footnotes: Vec<Footnote>,
    ) -> &mut Self {
        self.add_block(Block::MathML {
            element_str,
            fallback_image,
            caption,
            footnotes,
        })
    }

    pub fn make<P: AsRef<Path>>(&mut self, output_path: P) -> Result<(), EpubError> {
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

        let file_path = PathBuf::from(output_path.as_ref());
        let file_data = writer.into_inner().into_inner();
        fs::write(file_path, file_data)?;

        Ok(())
    }

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
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, vec};

    use crate::builder::content::{ContentBuilder, Footnote};

    #[test]
    fn test() {
        let ele_string = r#"
        <math xmlns="http://www.w3.org/1998/Math/MathML">
          <mrow>
            <munderover>
              <mo>∑</mo>
              <mrow>
                <mi>n</mi>
                <mo>=</mo>
                <mn>1</mn>
              </mrow>
              <mrow>
                <mo>+</mo>
                <mn>∞</mn>
              </mrow>
            </munderover>
            <mfrac>
              <mn>1</mn>
              <msup>
                <mi>n</mi>
                <mn>2</mn>
              </msup>
            </mfrac>
          </mrow>
        </math>"#;

        let content = ContentBuilder::new("zh-CN")
            .set_title("Test")
            .add_title_block(
                "This is a title",
                2,
                vec![
                    Footnote {
                        content: "This is a footnote for title.".to_string(),
                        locate: 15,
                    },
                    Footnote {
                        content: "This is another footnote for title.".to_string(),
                        locate: 4,
                    },
                ],
            )
            .add_text_block(
                "This is a paragraph.",
                vec![
                    Footnote {
                        content: "This is a footnote.".to_string(),
                        locate: 4,
                    },
                    Footnote {
                        content: "This is another footnote.".to_string(),
                        locate: 20,
                    },
                    Footnote {
                        content: "This is a third footnote.".to_string(),
                        locate: 4,
                    },
                ],
            )
            .add_image_block(
                PathBuf::from("C:\\Users\\Kikki\\Desktop\\background.jpg"),
                None,
                Some("this is an image".to_string()),
                vec![Footnote {
                    content: "This is a footnote for image.".to_string(),
                    locate: 16,
                }],
            )
            .add_quote_block(
                "Quote a text.",
                vec![Footnote {
                    content: "This is a footnote for quote.".to_string(),
                    locate: 13,
                }],
            )
            .add_audio_block(
                PathBuf::from("C:\\Users\\Kikki\\Desktop\\audio.mp3"),
                "This a fallback string".to_string(),
                Some("this is an audio".to_string()),
                vec![Footnote {
                    content: "This is a footnote for audio.".to_string(),
                    locate: 4,
                }],
            )
            .add_video_block(
                PathBuf::from("C:\\Users\\Kikki\\Desktop\\秋日何时来2024BD1080P.mp4"),
                "This a fallback string".to_string(),
                Some("this a video".to_string()),
                vec![Footnote {
                    content: "This is a footnote for video.".to_string(),
                    locate: 12,
                }],
            )
            .add_mathml_block(
                ele_string.to_owned(),
                None,
                Some("this is a formula".to_string()),
                vec![Footnote {
                    content: "This is a footnote for formula.".to_string(),
                    locate: 17,
                }],
            )
            .make("C:\\Users\\Kikki\\Desktop\\test.xhtml");
        assert!(content.is_ok());
    }
}
