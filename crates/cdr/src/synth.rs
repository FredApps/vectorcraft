//! Original, tiny RIFF fixtures generated from the public record grammar.
//! Available only for tests or when the `synth` feature is enabled.

/// One 100 × 80 point CDR 12 page with a 40 × 20 point rectangle,
/// solid red RGB fill and a one-point black outline. Geometry starts at the
/// page centre (50, 40 points), extending right and down.
pub fn rectangle() -> Vec<u8> {
    /// Encode little-endian words for generated record fields.
    fn u32s(values: &[u32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }
    /// Encode a point measurement as a signed CDR coordinate stored in a word.
    fn coord(points: f64) -> u32 {
        (points / 72.0 * 254000.0).round() as i32 as u32
    }
    /// Encode a RIFF chunk and pad odd payload lengths.
    fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = id.to_vec();
        v.extend_from_slice(&(body.len() as u32).to_le_bytes());
        v.extend_from_slice(body);
        if !body.len().is_multiple_of(2) {
            v.push(0);
        }
        v
    }
    /// Wrap generated child chunks in a typed RIFF LIST.
    fn list(kind: &[u8; 4], children: &[u8]) -> Vec<u8> {
        let mut b = kind.to_vec();
        b.extend_from_slice(children);
        chunk(b"LIST", &b)
    }
    let rgb = |r: u8, g: u8, b: u8| {
        let mut v = vec![5, 0, 0, 0, 0, 0, 0, 0];
        v.extend_from_slice(&[b, g, r, 0]);
        v
    };
    let mut fill = vec![1, 0, 0, 0, 1, 0, 0, 0];
    fill.extend(rgb(255, 0, 0));
    let mut outline = vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    outline.extend(u32s(&[coord(1.0)]));
    outline.extend_from_slice(&[100, 0, 0, 0]);
    outline.extend_from_slice(&[0; 56]);
    outline.extend(rgb(0, 0, 0));
    outline.extend_from_slice(&[0; 48]);
    let shape = u32s(&[coord(40.0), coord(-20.0), 0, 0, 0, 0]);
    let mut loda = u32s(&[84, 3, 60, 72, 1]);
    loda.extend_from_slice(&1u32.to_le_bytes());
    loda.extend_from_slice(&2u32.to_le_bytes());
    loda.extend_from_slice(&shape);
    loda.extend_from_slice(&[0; 8]);
    loda.extend(u32s(&[20, 24, 28, 30, 10, 20]));
    let object = list(b"obj ", &chunk(b"loda", &loda));
    let mut page = chunk(b"flgs", &[0, 0, 0, 0x90]);
    page.extend(object);
    let mut body = chunk(b"vrsn", &1200u16.to_le_bytes());
    let mut config = vec![0; 4];
    config.extend(u32s(&[coord(100.0), coord(80.0)]));
    body.extend(chunk(b"mcfg", &config));
    body.extend(chunk(b"fild", &fill));
    body.extend(chunk(b"outl", &outline));
    body.extend(list(b"page", &page));
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&((body.len() + 4) as u32).to_le_bytes());
    out.extend_from_slice(b"CDRC");
    out.extend(body);
    out
}

/// Original 2 × 2 RGB bitmap on a 100 × 80 point CDR 12 page, with a triangular crop and hole.
/// Its stored bottom row is blue/white, its top row red/green; a separate mask gives every
/// pixel alpha 128. The image has no preview.
pub fn clipped_bitmap() -> Vec<u8> {
    /// Encode fixture words without borrowing any producer-authored file bytes.
    fn words(values: &[u32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }
    /// Convert document points to the public CDR integer coordinate units.
    fn coord(points: i32) -> u32 {
        (points as f64 * 254000.0 / 72.0).round() as i32 as u32
    }
    /// Encode one padded RIFF fixture chunk.
    fn chunk(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut result = tag.to_vec();
        result.extend(words(&[body.len() as u32]));
        result.extend(body);
        if !body.len().is_multiple_of(2) {
            result.push(0);
        }
        result
    }
    /// Wrap fixture chunks in a typed RIFF list.
    fn list(kind: &[u8; 4], children: &[u8]) -> Vec<u8> {
        let mut body = kind.to_vec();
        body.extend(children);
        chunk(b"LIST", &body)
    }
    let mut image = words(&[7, 1, 0x4955, 212, 0, 32, 126, 0, 2, 255]);
    image.extend(b"RI");
    image.extend(words(&[94, 0, 78, 1, 1, 2, 2, 1, 24, 8, 16, 0, 0, 0]));
    image.extend([0; 20]);
    image.extend([255, 0, 0, 255, 255, 255, 0, 0, 0, 0, 255, 0, 255, 0, 0, 0]);
    image.extend(b"RI");
    image.extend(words(&[86, 0, 78, 99, 1, 2, 2, 1, 8, 4, 8, 0, 0, 0]));
    image.extend([0; 20]);
    image.extend([128, 128, 0, 0, 128, 128, 0, 0]);
    let corners = [0, 0, coord(40), coord(-20)];
    let mut geometry = words(&corners);
    geometry.extend(words(&corners));
    geometry.extend(words(&[0x0018_0002, 2, 2, 0, 7]));
    geometry.extend([0; 20]);
    geometry.extend(words(&[6, 0, 0, coord(40), 0, 0, coord(-20), coord(4), coord(-2), coord(12), coord(-2), coord(4), coord(-6)]));
    geometry.extend([0, 64, 72, 0, 64, 72]);
    let table = 20 + geometry.len() as u32;
    let mut object = words(&[table + 8, 1, table, table + 4, 5]);
    object.extend(geometry);
    object.extend(words(&[20, 30]));
    let mut page = chunk(b"flgs", &[0, 0, 0, 0x90]);
    page.extend(list(b"obj ", &chunk(b"loda", &object)));
    let mut body = chunk(b"vrsn", &1200u16.to_le_bytes());
    body.extend(chunk(b"mcfg", &words(&[0, coord(100), coord(80)])));
    body.extend(chunk(b"bmp ", &image));
    body.extend(list(b"page", &page));
    let mut result = b"RIFF".to_vec();
    result.extend(words(&[body.len() as u32 + 4]));
    result.extend(b"CDRC");
    result.extend(body);
    result
}
