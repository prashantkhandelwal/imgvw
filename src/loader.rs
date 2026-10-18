use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use image::{DynamicImage, GenericImageView};

pub const THUMBNAIL_SIZE: u32 = 160;
const PREVIEW_SIZE: u32 = 4096;
const SCAN_PROGRESS_BATCH_SIZE: usize = 64;
const SCAN_PROGRESS_INTERVAL: Duration = Duration::from_millis(50);

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScanProgress {
    pub files_read: usize,
    pub images_found: usize,
}

pub enum Event {
    ScanProgress {
        generation: u64,
        progress: ScanProgress,
    },
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

    pub fn open_folder(
        &self,
        path: PathBuf,
        include_hidden: bool,
        context: eframe::egui::Context,
    ) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        self.queue
            .0
            .lock()
            .expect("image queue poisoned")
            .requests
            .clear();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let mut latest = ScanProgress::default();
            let mut reported = ScanProgress::default();
            let mut last_update = Instant::now();
            let result = scan_folder(&path, include_hidden, |progress| {
                latest = progress;
                if progress.files_read == 1
                    || progress.files_read - reported.files_read >= SCAN_PROGRESS_BATCH_SIZE
                    || last_update.elapsed() >= SCAN_PROGRESS_INTERVAL
                {
                    let _ = sender.send(Event::ScanProgress {
                        generation,
                        progress,
                    });
                    context.request_repaint();
                    reported = progress;
                    last_update = Instant::now();
                }
            });
            if latest != reported {
                let _ = sender.send(Event::ScanProgress {
                    generation,
                    progress: latest,
                });
            }
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

pub fn scan_folder(
    path: &Path,
    include_hidden: bool,
    mut on_progress: impl FnMut(ScanProgress),
) -> Result<Vec<PathBuf>, String> {
    let mut directories = vec![(path.to_owned(), false)];
    let mut paths = Vec::new();
    let mut progress = ScanProgress::default();
    while let Some((directory, hidden_parent)) = directories.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("Cannot read folder {}: {error}", directory.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                format!(
                    "Cannot read folder entry in {}: {error}",
                    directory.display()
                )
            })?;
            let entry_path = entry.path();
            let hidden = hidden_parent || entry.file_name().as_encoded_bytes().starts_with(b".");
            if hidden && !include_hidden {
                continue;
            }
            let file_type = entry
                .file_type()
                .map_err(|error| format!("Cannot inspect {}: {error}", entry_path.display()))?;
            if file_type.is_symlink() {
                continue;
            }
            #[cfg(windows)]
            let hidden = {
                use std::os::windows::fs::MetadataExt;

                let metadata = entry
                    .metadata()
                    .map_err(|error| format!("Cannot inspect {}: {error}", entry_path.display()))?;
                hidden || metadata.file_attributes() & 0x2 != 0
            };
            if hidden && !include_hidden {
                continue;
            }
            if file_type.is_dir() {
                directories.push((entry_path, hidden));
            } else if file_type.is_file() {
                progress.files_read += 1;
                if supported(&entry_path) {
                    let validation = if hidden {
                        decode(&entry_path, Kind::Preview).map(|_| ())
                    } else {
                        Ok(())
                    };
                    match validation {
                        Ok(()) => {
                            paths.push(entry_path);
                            progress.images_found += 1;
                        }
                        Err(error) => {
                            eprintln!("Skipping hidden image {}: {error}", entry_path.display());
                        }
                    }
                }
                on_progress(progress);
            }
        }
    }
    paths.sort_by_cached_key(|path| {
        (
            path.file_name().unwrap_or_default().to_ascii_lowercase(),
            path.clone(),
        )
    });
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
        let names: Vec<_> = scan_folder(folder.path(), false, |_| {})
            .unwrap()
            .into_iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["A.jpg", "z.PNG"]);
        assert!(scan_folder(&folder.path().join("missing"), false, |_| {}).is_err());
    }

    #[test]
    fn recursive_scan_skips_dot_files_and_folders_without_counting_them() {
        let folder = tempfile::tempdir().unwrap();
        let nested = folder.path().join("nested");
        let hidden = folder.path().join(".hidden");
        fs::create_dir(&nested).unwrap();
        fs::create_dir(&hidden).unwrap();
        for path in [
            folder.path().join(".hidden.png"),
            folder.path().join("._photo.png"),
            folder.path().join(".notes.txt"),
            nested.join(".hidden.jpg"),
            hidden.join("photo.png"),
        ] {
            fs::write(path, []).unwrap();
        }
        let visible = nested.join("photo.png");
        fs::write(&visible, []).unwrap();
        fs::write(nested.join("notes.txt"), []).unwrap();
        let mut updates = Vec::new();
        assert_eq!(
            scan_folder(folder.path(), false, |progress| updates.push(progress)).unwrap(),
            vec![visible]
        );
        assert_eq!(updates.len(), 2);
        assert_eq!(
            updates.last(),
            Some(&ScanProgress {
                files_read: 2,
                images_found: 1,
            })
        );
    }

    #[cfg(windows)]
    #[test]
    fn recursive_scan_skips_windows_hidden_files_and_folders_without_counting_them() {
        use std::os::windows::ffi::OsStrExt;
        use std::os::windows::fs::MetadataExt;

        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn SetFileAttributesW(name: *const u16, attributes: u32) -> i32;
        }

        let folder = tempfile::tempdir().unwrap();
        let hidden_folder = folder.path().join("hidden");
        fs::create_dir(&hidden_folder).unwrap();
        fs::write(hidden_folder.join("photo.png"), []).unwrap();
        let hidden_image = folder.path().join("hidden.png");
        let hidden_text = folder.path().join("hidden.txt");
        fs::write(&hidden_image, []).unwrap();
        fs::write(&hidden_text, []).unwrap();
        for path in [&hidden_folder, &hidden_image, &hidden_text] {
            let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            let attributes = fs::metadata(path).unwrap().file_attributes() | 0x2;
            // The path is null-terminated and remains valid throughout the call.
            let result = unsafe { SetFileAttributesW(name.as_ptr(), attributes) };
            assert_ne!(result, 0, "{}", std::io::Error::last_os_error());
        }
        let visible = folder.path().join("visible.png");
        fs::write(&visible, []).unwrap();
        let mut updates = Vec::new();
        assert_eq!(
            scan_folder(folder.path(), false, |progress| updates.push(progress)).unwrap(),
            vec![visible.clone()]
        );
        assert_eq!(
            updates,
            vec![ScanProgress {
                files_read: 1,
                images_found: 1,
            }]
        );
        image::RgbaImage::new(8, 4).save(&hidden_image).unwrap();
        let hidden_child = hidden_folder.join("photo.png");
        image::RgbaImage::new(8, 4).save(&hidden_child).unwrap();
        fs::write(hidden_folder.join("invalid.png"), b"not an image").unwrap();
        let mut updates = Vec::new();
        assert_eq!(
            scan_folder(folder.path(), true, |progress| updates.push(progress)).unwrap(),
            vec![hidden_image, hidden_child, visible]
        );
        assert_eq!(
            updates.last(),
            Some(&ScanProgress {
                files_read: 5,
                images_found: 3,
            })
        );
    }

    #[test]
    fn hidden_search_includes_valid_images_and_skips_invalid_hidden_images() {
        let folder = tempfile::tempdir().unwrap();
        let hidden_folder = folder.path().join(".hidden");
        fs::create_dir(&hidden_folder).unwrap();
        let hidden_image = folder.path().join(".photo.png");
        let hidden_child = hidden_folder.join("child.png");
        for path in [&hidden_image, &hidden_child] {
            image::RgbaImage::new(8, 4).save(path).unwrap();
        }
        for path in [
            folder.path().join("._photo.png"),
            hidden_folder.join("invalid.png"),
            hidden_folder.join("notes.txt"),
        ] {
            fs::write(path, b"not an image").unwrap();
        }
        let visible = folder.path().join("visible.png");
        fs::write(&visible, b"not an image").unwrap();
        let mut updates = Vec::new();
        assert_eq!(
            scan_folder(folder.path(), true, |progress| updates.push(progress)).unwrap(),
            vec![hidden_image, hidden_child, visible.clone()]
        );
        assert_eq!(
            updates.last(),
            Some(&ScanProgress {
                files_read: 6,
                images_found: 3,
            })
        );
        assert_eq!(
            scan_folder(folder.path(), false, |_| {}).unwrap(),
            vec![visible],
            "Visible files must retain existing lazy decoding behavior"
        );
    }

    #[test]
    fn scans_nested_folders_and_preserves_duplicate_filenames() {
        let folder = tempfile::tempdir().unwrap();
        let nested = folder.path().join("nested.jpg");
        let deep = nested.join("deeper");
        fs::create_dir_all(&deep).unwrap();
        fs::create_dir(folder.path().join("empty")).unwrap();
        let expected = vec![
            folder.path().join("A.jpg"),
            nested.join("A.jpg"),
            nested.join("b.WeBp"),
            deep.join("c.PNG"),
            folder.path().join("z.PNG"),
        ];
        for path in &expected {
            fs::write(path, []).unwrap();
        }
        fs::write(deep.join("notes.txt"), []).unwrap();
        fs::write(deep.join("no-extension"), []).unwrap();
        assert_eq!(scan_folder(folder.path(), false, |_| {}).unwrap(), expected);
        assert!(
            scan_folder(&deep.join("notes.txt"), false, |_| {})
                .unwrap_err()
                .contains(&deep.join("notes.txt").display().to_string())
        );
    }

    #[cfg(unix)]
    #[test]
    fn recursive_scan_does_not_follow_symbolic_links() {
        use std::os::unix::fs::symlink;

        let folder = tempfile::tempdir().unwrap();
        let nested = folder.path().join("nested");
        fs::create_dir(&nested).unwrap();
        let image = nested.join("photo.png");
        fs::write(&image, []).unwrap();
        symlink(folder.path(), nested.join("parent")).unwrap();
        symlink(&image, folder.path().join("linked.png")).unwrap();
        assert_eq!(scan_folder(folder.path(), false, |_| {}).unwrap(), vec![image]);
    }

    #[test]
    fn recursive_scan_reports_files_checked_and_images_found() {
        let folder = tempfile::tempdir().unwrap();
        let nested = folder.path().join("nested");
        fs::create_dir(&nested).unwrap();
        for path in [
            folder.path().join("root.png"),
            folder.path().join("notes.txt"),
            nested.join("child.JPG"),
            nested.join("no-extension"),
        ] {
            fs::write(path, []).unwrap();
        }
        let mut updates = Vec::new();
        let images = scan_folder(folder.path(), false, |progress| updates.push(progress)).unwrap();
        assert_eq!(images.len(), 2);
        assert_eq!(updates.len(), 4);
        for (index, progress) in updates.iter().enumerate() {
            assert_eq!(progress.files_read, index + 1);
            assert!(progress.images_found <= progress.files_read);
        }
        assert_eq!(
            updates.last(),
            Some(&ScanProgress {
                files_read: 4,
                images_found: 2
            })
        );
        let empty = tempfile::tempdir().unwrap();
        assert!(
            scan_folder(empty.path(), false, |_| panic!("No files should be reported"))
                .unwrap()
                .is_empty()
        );
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
        fs::write(folder.path().join("notes.txt"), b"not an image").unwrap();
        let context = eframe::egui::Context::default();
        let loader = Loader::new(context.clone());
        let generation = loader.open_folder(folder.path().to_owned(), false, context);
        let mut progress_updates = Vec::new();
        let event = loop {
            let event = loader.events.recv_timeout(Duration::from_secs(5)).unwrap();
            if let Event::ScanProgress {
                generation: event_generation,
                progress,
            } = event
            {
                assert_eq!(event_generation, generation);
                progress_updates.push(progress);
            } else {
                break event;
            }
        };
        assert_eq!(
            progress_updates.last(),
            Some(&ScanProgress {
                files_read: 2,
                images_found: 1
            })
        );
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
