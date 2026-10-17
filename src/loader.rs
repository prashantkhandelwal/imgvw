use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread;

use image::{DynamicImage, GenericImageView};

pub const THUMBNAIL_SIZE: u32 = 160;
const PREVIEW_SIZE: u32 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Thumbnail,
    Preview,
}

#[derive(Clone, Debug)]
pub struct Request {
    pub generation: u64,
    pub index: usize,
    pub path: PathBuf,
    pub kind: Kind,
}

#[derive(Debug)]
pub struct Metadata {
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub fields: Vec<(String, String)>,
    pub exif_error: Option<String>,
}

pub struct Decoded {
    pub pixels: image::RgbaImage,
    pub metadata: Option<Metadata>,
}

pub enum Event {
    Folder {
        generation: u64,
        path: PathBuf,
        result: Result<Vec<PathBuf>, String>,
    },
    Image {
        request: Request,
        result: Result<Decoded, String>,
    },
}

#[derive(Default)]
struct Queue {
    requests: VecDeque<Request>,
    stopped: bool,
}

pub struct Loader {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    generation: Arc<AtomicU64>,
    sender: mpsc::Sender<Event>,
    pub events: mpsc::Receiver<Event>,
}

impl Loader {
    pub fn new(context: eframe::egui::Context) -> Self {
        let queue = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
        let generation = Arc::new(AtomicU64::new(0));
        let (sender, events) = mpsc::channel();
        let workers = thread::available_parallelism()
            .map_or(2, usize::from)
            .clamp(2, 4);

        for _ in 0..workers {
            let queue = Arc::clone(&queue);
            let generation = Arc::clone(&generation);
            let sender = sender.clone();
            let context = context.clone();
            thread::spawn(move || {
                loop {
                    let request = {
                        let (lock, ready) = &*queue;
                        let mut state = lock.lock().expect("image queue poisoned");
                        while state.requests.is_empty() && !state.stopped {
                            state = ready.wait(state).expect("image queue poisoned");
                        }
                        if state.stopped {
                            return;
                        }
                        state.requests.pop_front().expect("queued request")
                    };
                    if request.generation != generation.load(Ordering::Relaxed) {
                        continue;
                    }
                    let result = decode(&request.path, request.kind);
                    if request.generation == generation.load(Ordering::Relaxed) {
                        if sender.send(Event::Image { request, result }).is_err() {
                            return;
                        }
                        context.request_repaint();
                    }
                }
            });
        }
        Self {
            queue,
            generation,
            sender,
            events,
        }
    }

    pub fn open_folder(&self, path: PathBuf, context: eframe::egui::Context) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        self.queue
            .0
            .lock()
            .expect("image queue poisoned")
            .requests
            .clear();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = scan_folder(&path);
            let _ = sender.send(Event::Folder {
                generation,
                path,
                result,
            });
            context.request_repaint();
        });
        generation
    }

    pub fn request(&self, request: Request) {
        let (lock, ready) = &*self.queue;
        let mut queue = lock.lock().expect("image queue poisoned");
        if request.kind == Kind::Preview {
            queue.requests.retain(|item| item.kind != Kind::Preview);
            queue.requests.push_front(request);
        } else {
            queue.requests.push_back(request);
        }
        ready.notify_one();
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        let (lock, ready) = &*self.queue;
        let mut queue = lock.lock().expect("image queue poisoned");
        queue.stopped = true;
        queue.requests.clear();
        ready.notify_all();
    }
}

pub fn supported(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "jpg"
                    | "jpeg"
                    | "png"
                    | "gif"
                    | "webp"
                    | "bmp"
                    | "tif"
                    | "tiff"
                    | "ico"
                    | "pnm"
                    | "pbm"
                    | "pgm"
                    | "ppm"
                    | "tga"
            )
        })
}

pub fn scan_folder(path: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(path).map_err(|error| format!("Cannot read folder: {error}"))?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("Cannot read folder entry: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Cannot inspect {}: {error}", entry.path().display()))?;
        if file_type.is_file() && supported(&entry.path()) {
            paths.push(entry.path());
        }
    }
    paths.sort_by_cached_key(|path| path.file_name().unwrap_or_default().to_ascii_lowercase());
    Ok(paths)
}

fn read_exif(
    path: &Path,
    format: Option<image::ImageFormat>,
) -> Result<Option<exif::Exif>, String> {
    if !matches!(
        format,
        Some(
            image::ImageFormat::Jpeg
                | image::ImageFormat::Tiff
                | image::ImageFormat::Png
                | image::ImageFormat::WebP
        )
    ) {
        return Ok(None);
    }
    let file = File::open(path).map_err(|error| format!("Cannot read EXIF: {error}"))?;
    match exif::Reader::new().read_from_container(&mut BufReader::new(file)) {
        Ok(exif) => Ok(Some(exif)),
        Err(exif::Error::NotFound(_)) => Ok(None),
        Err(error) => Err(format!("EXIF unavailable: {error}")),
    }
}

fn orient(image: DynamicImage, orientation: u32) -> DynamicImage {
    match orientation {
        2 => image.fliph(),
        3 => image.rotate180(),
        4 => image.flipv(),
        5 => image.rotate90().fliph(),
        6 => image.rotate90(),
        7 => image.rotate270().fliph(),
        8 => image.rotate270(),
        _ => image,
    }
}

pub fn decode(path: &Path, kind: Kind) -> Result<Decoded, String> {
    let reader = image::ImageReader::open(path)
        .map_err(|error| format!("Cannot open image: {error}"))?
        .with_guessed_format()
        .map_err(|error| format!("Cannot identify image: {error}"))?;
    let format = reader.format();
    let image = reader
        .decode()
        .map_err(|error| format!("Cannot decode image: {error}"))?;
    let exif = read_exif(path, format);
    let orientation = exif
        .as_ref()
        .ok()
        .and_then(|value| value.as_ref())
        .and_then(|value| value.get_field(exif::Tag::Orientation, exif::In::PRIMARY))
        .and_then(|field| field.value.get_uint(0))
        .unwrap_or(1);
    let image = orient(image, orientation);
    let (width, height) = image.dimensions();
    let metadata = if kind == Kind::Preview {
        let bytes = fs::metadata(path)
            .map_err(|error| format!("Cannot read file properties: {error}"))?
            .len();
        let (fields, exif_error) = match exif {
            Ok(Some(exif)) => (
                exif.fields()
                    .map(|field| {
                        (
                            field.tag.to_string(),
                            field.display_value().with_unit(&exif).to_string(),
                        )
                    })
                    .collect(),
                None,
            ),
            Ok(None) => (Vec::new(), None),
            Err(error) => (Vec::new(), Some(error)),
        };
        Some(Metadata {
            width,
            height,
            bytes,
            fields,
            exif_error,
        })
    } else {
        None
    };
    let size = match kind {
        Kind::Thumbnail => THUMBNAIL_SIZE,
        Kind::Preview => PREVIEW_SIZE,
    };
    let pixels = if width > size || height > size {
        image.thumbnail(size, size).to_rgba8()
    } else {
        image.to_rgba8()
    };
    Ok(Decoded { pixels, metadata })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_supported_files_only_and_sorts_case_insensitively() {
        let folder = tempfile::tempdir().unwrap();
        for name in ["z.PNG", "A.jpg", "notes.txt", "no-extension"] {
            fs::write(folder.path().join(name), []).unwrap();
        }
        fs::create_dir(folder.path().join("nested.jpg")).unwrap();
        let names: Vec<_> = scan_folder(folder.path())
            .unwrap()
            .into_iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["A.jpg", "z.PNG"]);
        assert!(scan_folder(&folder.path().join("missing")).is_err());
    }

    #[test]
    fn decoding_scales_thumbnails_and_preserves_properties() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("test.png");
        image::RgbaImage::new(640, 320).save(&path).unwrap();
        let thumbnail = decode(&path, Kind::Thumbnail).unwrap();
        assert_eq!(thumbnail.pixels.dimensions(), (160, 80));
        assert!(thumbnail.metadata.is_none());
        let preview = decode(&path, Kind::Preview).unwrap();
        assert_eq!(preview.pixels.dimensions(), (640, 320));
        let metadata = preview.metadata.unwrap();
        assert_eq!((metadata.width, metadata.height), (640, 320));
        assert!(metadata.bytes > 0);
        assert!(metadata.fields.is_empty());
        fs::write(&path, b"not an image").unwrap();
        assert!(decode(&path, Kind::Preview).is_err());
    }

    #[test]
    fn formats_without_exif_do_not_report_metadata_errors() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("test.bmp");
        image::RgbImage::new(4, 2).save(&path).unwrap();
        let decoded = decode(&path, Kind::Preview).unwrap();
        let metadata = decoded.metadata.unwrap();
        assert!(metadata.fields.is_empty());
        assert!(metadata.exif_error.is_none());
    }

    #[test]
    fn reads_exif_fields_and_applies_orientation_to_both_sizes() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("test.jpg");
        image::RgbImage::new(200, 300).save(&path).unwrap();
        let jpeg = fs::read(&path).unwrap();
        // A little-endian TIFF IFD with Orientation=6 and Make="Canon".
        let exif = [
            b'E', b'x', b'i', b'f', 0, 0, b'I', b'I', 42, 0, 8, 0, 0, 0, 2, 0, 0x12, 0x01, 3, 0, 1,
            0, 0, 0, 6, 0, 0, 0, 0x0f, 0x01, 2, 0, 6, 0, 0, 0, 38, 0, 0, 0, 0, 0, 0, 0, b'C', b'a',
            b'n', b'o', b'n', 0,
        ];
        let mut with_exif = jpeg[..2].to_vec();
        with_exif.extend_from_slice(&[0xff, 0xe1]);
        with_exif.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        with_exif.extend_from_slice(&exif);
        with_exif.extend_from_slice(&jpeg[2..]);
        fs::write(&path, with_exif).unwrap();
        let preview = decode(&path, Kind::Preview).unwrap();
        assert_eq!(preview.pixels.dimensions(), (300, 200));
        let metadata = preview.metadata.unwrap();
        assert_eq!((metadata.width, metadata.height), (300, 200));
        assert!(metadata.exif_error.is_none());
        assert!(
            metadata
                .fields
                .iter()
                .any(|(name, value)| name == "Make" && value.contains("Canon"))
        );
        let thumbnail = decode(&path, Kind::Thumbnail).unwrap();
        assert_eq!(thumbnail.pixels.width(), THUMBNAIL_SIZE);
        assert!(thumbnail.pixels.width() > thumbnail.pixels.height());
    }

    #[test]
    fn orientation_handles_all_exif_transforms() {
        let image = image::RgbImage::from_fn(2, 3, |x, y| image::Rgb([(y * 2 + x) as u8, 0, 0]));
        let expected = [
            vec![0, 1, 2, 3, 4, 5],
            vec![1, 0, 3, 2, 5, 4],
            vec![5, 4, 3, 2, 1, 0],
            vec![4, 5, 2, 3, 0, 1],
            vec![0, 2, 4, 1, 3, 5],
            vec![4, 2, 0, 5, 3, 1],
            vec![5, 3, 1, 4, 2, 0],
            vec![1, 3, 5, 0, 2, 4],
        ];
        for (index, expected) in expected.iter().enumerate() {
            let result = orient(DynamicImage::ImageRgb8(image.clone()), index as u32 + 1);
            let actual: Vec<_> = result.to_rgb8().pixels().map(|pixel| pixel[0]).collect();
            assert_eq!(&actual, expected);
        }
    }

    #[test]
    fn worker_delivers_folder_and_image_results_asynchronously() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("test.png");
        image::RgbaImage::new(8, 4).save(&path).unwrap();
        let context = eframe::egui::Context::default();
        let loader = Loader::new(context.clone());
        let generation = loader.open_folder(folder.path().to_owned(), context);
        let event = loader
            .events
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(
            matches!(event, Event::Folder { result: Ok(paths), .. } if paths == vec![path.clone()])
        );
        loader.request(Request {
            generation,
            index: 0,
            path,
            kind: Kind::Preview,
        });
        let event = loader
            .events
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(matches!(event, Event::Image { result: Ok(_), .. }));
    }
}
