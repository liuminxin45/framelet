use anyhow::{Context, Result, bail, ensure};
use std::time::{Duration, Instant};
use windows::{
    Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap},
    Media::Ocr::OcrEngine,
    Security::Cryptography::CryptographicBuffer,
};

pub fn contains(rgb: &[u8], width: u32, height: u32, query: &str) -> Result<bool> {
    let max = OcrEngine::MaxImageDimension()?;
    ensure!(
        width <= max && height <= max,
        "GIF frame exceeds Windows OCR dimensions ({max} px per side); crop the source first"
    );
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .context("Windows OCR unavailable; check the installed profile OCR language")?;
    let mut bgra = Vec::with_capacity(rgb.len() / 3 * 4);
    for pixel in rgb.as_chunks::<3>().0 {
        bgra.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
    }
    let buffer = CryptographicBuffer::CreateFromByteArray(&bgra)?;
    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        width as i32,
        height as i32,
    )?;
    let operation = engine.RecognizeAsync(&bitmap)?;
    let start = Instant::now();
    while operation.Status()?.0 == 0 {
        if start.elapsed() >= Duration::from_secs(3) {
            let _ = operation.Cancel();
            bail!("Windows OCR timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let text = operation.GetResults()?.Text()?.to_string();
    Ok(text.to_lowercase().contains(&query.to_lowercase()))
}
