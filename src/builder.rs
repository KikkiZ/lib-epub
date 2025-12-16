use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::{BufReader, Cursor, Read, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
};

use infer::Infer;
use log::warn;
use quick_xml::{
    Writer,
    events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event},
};
use thiserror::Error;
use walkdir::WalkDir;
use zip::{CompressionMethod, ZipWriter, write::FileOptions};

use crate::{
    epub::EpubDoc,
    error::EpubError,
    types::{ManifestItem, MetadataItem, SpineItem},
    utils::{ELEMENT_IN_DC_NAMESPACE, local_time},
};

type XmlWriter = Writer<Cursor<Vec<u8>>>;

#[derive(Debug, Error)]
pub enum EpubBuilderError {
    #[error("Expect a file, but \"{target_path}\" is not a file.")]
    ExpectFile { target_path: String },

    #[error("Need at least one rootfile.")]
    MissingRootfile,

    #[error("Unable to analyze the file \"{file_path}\" type.")]
    UnknowFileFormat { file_path: String },
}

// struct EpubVersion2;
struct EpubVersion3;

pub struct EpubBuilder<Version> {
    epub_version: PhantomData<Version>,
    temp_dir: PathBuf,

    rootfiles: Vec<String>,
    metadata: Vec<MetadataItem>,
    manifest: HashMap<String, ManifestItem>,
    spine: Vec<SpineItem>,
}

impl EpubBuilder<EpubVersion3> {
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
        })
    }

    pub fn add_rootfile(&mut self, rootfile: &str) -> &mut Self {
        self.rootfiles.push(rootfile.to_string());

        self
    }

    pub fn add_metadata(&mut self, item: MetadataItem) -> &mut Self {
        self.metadata.push(item);
        self
    }

    pub fn add_manifest(
        &mut self,
        manifest_source: &str,
        manifest_item: ManifestItem,
    ) -> Result<&mut Self, EpubError> {
        let source = PathBuf::from(manifest_source);
        if !source.is_file() {
            return Err(EpubBuilderError::ExpectFile {
                target_path: manifest_source.to_string(),
            }
            .into());
        }

        let extension = match source.extension() {
            Some(ext) => ext.to_str().unwrap().to_lowercase(),
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

    pub fn add_spine(&mut self, item: SpineItem) -> &mut Self {
        self.spine.push(item);
        self
    }

    pub fn make<P: AsRef<Path>>(self, output_path: P) -> Result<(), EpubError> {
        self.make_container_xml()?;
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

    pub fn build<P: AsRef<Path>>(
        self,
        output_path: P,
    ) -> Result<EpubDoc<BufReader<File>>, EpubError> {
        self.make(&output_path)?;

        EpubDoc::new(output_path)
    }

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

    fn make_opf_file(&self) -> Result<(), EpubError> {
        let mut writer = Writer::new(Cursor::new(Vec::new()));

        writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;

        writer.write_event(Event::Start(BytesStart::new("package").with_attributes([
            ("xmlns", "http://www.idpf.org/2007/opf"),
            ("xmlns:dc", "http://purl.org/dc/elements/1.1/"),
            ("unique-identifier", "pub-id"),
            ("version", "3.0"),
        ])))?;

        self.opf_metadata(&mut writer)?;
        self.opf_manifest(&mut writer)?;
        self.opf_spine(&mut writer)?;

        writer.write_event(Event::End(BytesEnd::new("package")))?;

        let file_path = self.temp_dir.join(&self.rootfiles[0]);
        let file_data = writer.into_inner().into_inner();
        fs::write(file_path, file_data)?;

        Ok(())
    }

    // TODO: 构建metadata时添加modified时间相关的元数据
    fn opf_metadata(&self, writer: &mut XmlWriter) -> Result<(), EpubError> {
        writer.write_event(Event::Start(BytesStart::new("metadata")))?;

        for metadata in &self.metadata {
            let tag_name = if ELEMENT_IN_DC_NAMESPACE.contains(&metadata.property.as_str()) {
                format!("dc:{}", metadata.property)
            } else {
                metadata.property.clone()
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

    fn opf_manifest(&self, writer: &mut XmlWriter) -> Result<(), EpubError> {
        writer.write_event(Event::Start(BytesStart::new("manifest")))?;

        for (_, manifest) in &self.manifest {
            writer.write_event(Event::Empty(
                BytesStart::new("item").with_attributes(manifest.attributes()),
            ))?;
        }

        writer.write_event(Event::End(BytesEnd::new("manifest")))?;

        Ok(())
    }

    fn opf_spine(&self, writer: &mut XmlWriter) -> Result<(), EpubError> {
        writer.write_event(Event::Start(BytesStart::new("spine")))?;

        for spine in &self.spine {
            writer.write_event(Event::Empty(
                BytesStart::new("itemref").with_attributes(spine.attributes()),
            ))?;
        }

        writer.write_event(Event::End(BytesEnd::new("spine")))?;

        Ok(())
    }
}

impl<Version> Drop for EpubBuilder<Version> {
    fn drop(&mut self) {
        if let Err(err) = fs::remove_dir_all(&self.temp_dir) {
            warn!("{}", err);
        };
    }
}

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
    use std::env;

    use crate::{
        builder::{EpubBuilder, EpubVersion3},
        types::{ManifestItem, MetadataItem, MetadataRefinement, SpineItem},
    };

    #[test]
    fn it_works() {
        let builder = EpubBuilder::<EpubVersion3>::new();
        assert!(builder.is_ok());

        let mut builder = builder.unwrap();
        builder.add_rootfile("package.opf");

        builder.add_metadata(
            MetadataItem::new("identifier", "https://www.w3.org/TR/epub-33/")
                .with_id("pub-id")
                .build(),
        );
        builder.add_metadata(
            MetadataItem::new("title", "EPUB 3.3")
                .with_id("title")
                .append_refinement(MetadataRefinement::new("title", "title-type", "main"))
                .build(),
        );
        builder.add_metadata(MetadataItem::new("language", "en-us"));
        builder.add_metadata(
            MetadataItem::new("subject", "Information systems~World Wide Web")
                .with_id("acm1")
                .append_refinement(MetadataRefinement::new(
                    "acm1",
                    "authority",
                    "https://dl.acm.org/ccs",
                ))
                .append_refinement(MetadataRefinement::new("acm1", "term", "10002951.10003260"))
                .build(),
        );
        builder.add_metadata(
            MetadataItem::new(
                "subject",
                "General and reference~Computing standards, RFCs and guidelines",
            )
            .with_id("acm2")
            .build(),
        );

        let _ = builder.add_manifest(
            "./test_case/nav.xhtml",
            ManifestItem::new("nav", "epub/nav.xhtml")
                .append_property("nav")
                .with_fallback("main")
                .build(),
        );
        let _ = builder.add_manifest(
            "./test_case/Overview.xhtml",
            ManifestItem::new("main", "epub/Overview.xhtml")
                .append_property("scripted")
                .append_property("svg")
                .build(),
        );

        builder.add_spine(SpineItem::new("nav").set_linear(false).build());
        builder.add_spine(SpineItem::new("main"));

        let target_path = env::temp_dir().join("target.epub");
        let result = builder.build(target_path);
        assert!(result.is_ok());

        let doc = result.unwrap();
        assert!(doc.get_title().is_ok());
        assert_eq!(doc.get_title().unwrap(), vec!["EPUB 3.3"]);
    }
}
