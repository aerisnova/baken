//! Artwork on the stick (issue #235), from the picture embedded in each audio
//! file: the XML carries none, and rekordbox ties its own thumbnails to tracks
//! only in its database. The thumbnails are made the way rekordbox makes its
//! `artwork_s.jpg` and `artwork_m.jpg` (1091 of them read on the owner's
//! library): 80 and 240 pixels square, the picture scaled to fit and centred
//! on black, baseline JPEG at quality 85 with 4:2:0 chroma.

use crate::layout::Root;
use image::imageops::{self, FilterType};
use image::{Rgb, RgbImage};
use jpeg_encoder::{ColorType, Encoder, SamplingFactor};
use std::path::Path;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, StandardVisualKey, Visual};

pub const SMALL: u32 = 80;
pub const MEDIUM: u32 = 240;
const QUALITY: u8 = 85;

/// `/PIONEER/Artwork/00001/a1.jpg`, under the library folder `root`.
/// Folders are numbered `id / 20 + 1`, so ids 1 to 19 share `00001` and every
/// later folder holds twenty, as on the reference stick. `b` files hold the
/// same bytes as `a` files: `export.pdb` names the `a` file, OneLibrary the
/// `b` one.
pub fn path(root: Root, id: u32, copy: char, medium: bool) -> String {
    let size = if medium { "_m" } else { "" };
    format!(
        "/{}/Artwork/{:05}/{copy}{id}{size}.jpg",
        root.name(),
        id / 20 + 1
    )
}

/// The four files rekordbox writes for image `id`, each with whether it
/// holds the medium thumbnail.
pub fn files(root: Root, id: u32) -> [(String, bool); 4] {
    [
        (path(root, id, 'a', false), false),
        (path(root, id, 'a', true), true),
        (path(root, id, 'b', false), false),
        (path(root, id, 'b', true), true),
    ]
}

/// A track's two thumbnails, JPEG encoded.
#[derive(Debug, Clone)]
pub struct Thumbnails {
    pub small: Vec<u8>,
    pub medium: Vec<u8>,
}

/// The thumbnails of the picture embedded in `audio`; `None` when it has
/// none or it cannot be decoded.
pub fn for_file(audio: &Path) -> Option<Thumbnails> {
    thumbnails(&embedded(audio)?)
}

/// The embedded picture, the front cover when there are several: ID3v2
/// `APIC` (MP3, and the ID3 chunk of AIFF and WAV), FLAC `PICTURE`, MP4
/// `covr`.
pub fn embedded(audio: &Path) -> Option<Vec<u8>> {
    probed(audio).or_else(|| wav_picture(audio))
}

/// What symphonia reads, which leaves out the ID3 chunk of a WAV file.
fn probed(audio: &Path) -> Option<Vec<u8>> {
    let file = std::fs::File::open(audio).ok()?;
    let mut hint = Hint::new();
    if let Some(ext) = audio.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            MediaSourceStream::new(Box::new(file), Default::default()),
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .ok()?;
    let mut visuals: Vec<Visual> = Vec::new();
    let mut log = format.metadata();
    while let Some(rev) = log.pop() {
        visuals.extend(rev.media.visuals);
    }
    if let Some(rev) = log.current() {
        visuals.extend(rev.media.visuals.iter().cloned());
    }
    let front = visuals
        .iter()
        .position(|v| v.usage == Some(StandardVisualKey::FrontCover))
        .unwrap_or(0);
    visuals.into_iter().nth(front).map(|v| v.data.into_vec())
}

/// The `APIC` of a WAV file's ID3 chunk, the front cover when there are
/// several.
fn wav_picture(audio: &Path) -> Option<Vec<u8>> {
    let ext = audio.extension()?.to_str()?.to_ascii_lowercase();
    if ext != "wav" && ext != "wave" {
        return None;
    }
    let tag = id3::Tag::read_from_path(audio).ok()?;
    let pictures: Vec<&id3::frame::Picture> = tag.pictures().collect();
    let front = pictures
        .iter()
        .position(|p| p.picture_type == id3::frame::PictureType::CoverFront)
        .unwrap_or(0);
    pictures.get(front).map(|p| p.data.clone())
}

/// `picture` (JPEG, PNG or BMP) as rekordbox's two thumbnails.
pub fn thumbnails(picture: &[u8]) -> Option<Thumbnails> {
    let image = image::load_from_memory(picture).ok()?.to_rgb8();
    Some(Thumbnails {
        small: square(&image, SMALL)?,
        medium: square(&image, MEDIUM)?,
    })
}

fn square(image: &RgbImage, side: u32) -> Option<Vec<u8>> {
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let scale = f64::from(side) / f64::from(w.max(h));
    let fit = |n: u32| ((f64::from(n) * scale).round() as u32).clamp(1, side);
    let scaled = imageops::resize(image, fit(w), fit(h), FilterType::Lanczos3);
    let mut canvas = RgbImage::from_pixel(side, side, Rgb([0, 0, 0]));
    let (x, y) = ((side - scaled.width()) / 2, (side - scaled.height()) / 2);
    imageops::overlay(&mut canvas, &scaled, i64::from(x), i64::from(y));
    let mut jpeg = Vec::new();
    let mut encoder = Encoder::new(&mut jpeg, QUALITY);
    encoder.set_sampling_factor(SamplingFactor::R_4_2_0);
    encoder
        .encode(canvas.as_raw(), side as u16, side as u16, ColorType::Rgb)
        .ok()?;
    Some(jpeg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use id3::frame::{Picture, PictureType};
    use id3::TagLike;

    #[test]
    fn paths_follow_the_reference_stick() {
        // OneLibrary of the reference stick: b18/b19 in 00001, b20 and b39 in 00002, b40 in 00003
        assert_eq!(
            path(Root::Pioneer, 1, 'a', false),
            "/PIONEER/Artwork/00001/a1.jpg"
        );
        assert_eq!(
            path(Root::Pioneer, 19, 'b', false),
            "/PIONEER/Artwork/00001/b19.jpg"
        );
        assert_eq!(
            path(Root::Pioneer, 20, 'b', false),
            "/PIONEER/Artwork/00002/b20.jpg"
        );
        assert_eq!(
            path(Root::Pioneer, 39, 'a', true),
            "/PIONEER/Artwork/00002/a39_m.jpg"
        );
        assert_eq!(
            path(Root::Pioneer, 40, 'b', false),
            "/PIONEER/Artwork/00003/b40.jpg"
        );
        // as rekordbox wrote them on an HFS+ stick (#287)
        assert_eq!(
            path(Root::Hidden, 1, 'b', false),
            "/.PIONEER/Artwork/00001/b1.jpg"
        );
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbImage::from_pixel(w, h, Rgb([200, 40, 120]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn a_wide_picture_is_letterboxed_on_black() {
        let t = thumbnails(&png(500, 250)).unwrap();
        for (jpeg, side) in [(&t.small, SMALL), (&t.medium, MEDIUM)] {
            let img = image::load_from_memory(jpeg).unwrap().to_rgb8();
            assert_eq!(img.dimensions(), (side, side));
            let dark = |p: &Rgb<u8>| p.0.iter().all(|&c| c < 16);
            assert!(dark(img.get_pixel(side / 2, 1)), "black band above");
            assert!(dark(img.get_pixel(side / 2, side - 2)), "black band below");
            let mid = img.get_pixel(side / 2, side / 2).0;
            assert!(
                mid[0] > 150 && mid[1] < 90,
                "the picture in the middle: {mid:?}"
            );
        }
        assert!(thumbnails(b"not a picture").is_none());
    }

    /// Half a second of 16-bit stereo silence at 44.1 kHz.
    fn wav(path: &Path) {
        let data_len: u32 = 22050 * 4;
        let mut b = b"RIFF".to_vec();
        b.extend_from_slice(&(36 + data_len).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&44100u32.to_le_bytes());
        b.extend_from_slice(&(44100u32 * 4).to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_len.to_le_bytes());
        b.resize(b.len() + data_len as usize, 0);
        std::fs::write(path, b).unwrap();
    }

    fn picture(picture_type: PictureType, data: &[u8]) -> Picture {
        Picture {
            mime_type: "image/png".into(),
            picture_type,
            description: String::new(),
            data: data.to_vec(),
        }
    }

    #[test]
    fn a_wav_gives_the_front_cover_of_its_id3_chunk() {
        let dir = std::env::temp_dir().join(format!("baken-wav-art-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.wav");
        wav(&path);
        assert_eq!(embedded(&path), None);

        let mut tag = id3::Tag::new();
        tag.add_frame(picture(PictureType::CoverBack, b"back"));
        tag.add_frame(picture(PictureType::CoverFront, b"front"));
        tag.write_to_path(&path, id3::Version::Id3v24).unwrap();
        assert_eq!(embedded(&path).as_deref(), Some(&b"front"[..]));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
