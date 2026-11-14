//! Retain only the selected face of a system TTC, keeping glyph IDs and tables.
//! SFNT layout/checksums: https://learn.microsoft.com/en-us/typography/opentype/spec/otff
use std::sync::Arc;
use zui_api::text::TextError;

#[cfg(feature = "system-fonts")]
pub(crate) fn standalone_face(
    data: &[u8],
    index: u32,
    budget: usize,
) -> Result<Arc<[u8]>, TextError> {
    let font = skrifa::FontRef::from_index(data, index).map_err(|_| TextError::InvalidFont)?;
    if font.ttc_index().is_none() {
        if data.len() > budget {
            return Err(TextError::BudgetExceeded);
        }
        return Ok(Arc::from(data));
    }
    let mut tables = Vec::new();
    for record in font.table_directory().table_records() {
        let tag = record.tag();
        // A signature over the original collection is invalid after extraction.
        if tag == skrifa::Tag::new(b"DSIG") {
            continue;
        }
        let table = font.table_data(tag).ok_or(TextError::InvalidFont)?;
        tables.push((tag, table.as_bytes()));
    }
    tables.sort_by_key(|(tag, _)| *tag);
    if tables.is_empty()
        || tables.len() > 4095
        || tables.windows(2).any(|pair| pair[0].0 == pair[1].0)
    {
        return Err(TextError::InvalidFont);
    }
    let mut total = 12 + tables.len() * 16;
    for (_, bytes) in &tables {
        total = total
            .checked_add(
                bytes
                    .len()
                    .checked_add(3)
                    .ok_or(TextError::BudgetExceeded)?
                    & !3,
            )
            .ok_or(TextError::BudgetExceeded)?;
    }
    if total > budget || total > u32::MAX as usize {
        return Err(TextError::BudgetExceeded);
    }
    let count = tables.len() as u16;
    let power = count.ilog2() as u16;
    let search_range = (1u16 << power) * 16;
    let mut output = vec![0; total];
    output[..4].copy_from_slice(&font.table_directory().sfnt_version().to_be_bytes());
    output[4..6].copy_from_slice(&count.to_be_bytes());
    output[6..8].copy_from_slice(&search_range.to_be_bytes());
    output[8..10].copy_from_slice(&power.to_be_bytes());
    output[10..12].copy_from_slice(&(count * 16 - search_range).to_be_bytes());
    let mut offset = 12 + tables.len() * 16;
    let mut head = None;
    for (i, (tag, bytes)) in tables.iter().enumerate() {
        let end = offset + bytes.len();
        output[offset..end].copy_from_slice(bytes);
        if *tag == skrifa::Tag::new(b"head") {
            if bytes.len() < 12 {
                return Err(TextError::InvalidFont);
            }
            head = Some(offset);
            output[offset + 8..offset + 12].fill(0);
        }
        let sum = checksum(&output[offset..end]);
        let record = 12 + i * 16;
        output[record..record + 4].copy_from_slice(&tag.to_be_bytes());
        output[record + 4..record + 8].copy_from_slice(&sum.to_be_bytes());
        output[record + 8..record + 12].copy_from_slice(&(offset as u32).to_be_bytes());
        output[record + 12..record + 16].copy_from_slice(&(bytes.len() as u32).to_be_bytes());
        offset = (end + 3) & !3;
    }
    if let Some(head) = head {
        let adjustment = 0xB1B0AFBAu32.wrapping_sub(checksum(&output));
        output[head + 8..head + 12].copy_from_slice(&adjustment.to_be_bytes());
    }
    Ok(output.into())
}

#[cfg(feature = "system-fonts")]
fn checksum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

#[cfg(all(test, feature = "system-fonts"))]
mod tests {
    use super::*;

    #[test]
    fn collection_extraction_preserves_tables_and_obeys_budget() {
        let original = include_bytes!("../../../assets/fonts/NotoSansSC.ttf");
        // Two faces share the same directory/tables; bytes after them are unrelated.
        let mut ttc = b"ttcf\0\x01\0\0\0\0\0\x02\0\0\0\x14\0\0\0\x14".to_vec();
        ttc.extend_from_slice(original);
        let count = u16::from_be_bytes([original[4], original[5]]) as usize;
        for i in 0..count {
            let at = 20 + 12 + i * 16 + 8;
            let offset = u32::from_be_bytes(ttc[at..at + 4].try_into().unwrap());
            ttc[at..at + 4].copy_from_slice(&(offset + 20).to_be_bytes());
        }
        ttc.extend_from_slice(&[0; 128]);
        let extracted = standalone_face(&ttc, 1, original.len()).unwrap();
        assert_eq!(checksum(&extracted), 0xB1B0AFBA);
        let before = skrifa::FontRef::new(original).unwrap();
        let after = skrifa::FontRef::new(&extracted).unwrap();
        for record in before.table_directory().table_records() {
            if record.tag() == skrifa::Tag::new(b"DSIG") {
                continue;
            }
            let a = before.table_data(record.tag()).unwrap();
            let b = after.table_data(record.tag()).unwrap();
            if record.tag() == skrifa::Tag::new(b"head") {
                assert_eq!(&a.as_bytes()[..8], &b.as_bytes()[..8]);
                assert_eq!(&a.as_bytes()[12..], &b.as_bytes()[12..]);
            } else {
                assert_eq!(a.as_bytes(), b.as_bytes());
            }
        }
        assert_eq!(
            standalone_face(&ttc, 1, extracted.len() - 1),
            Err(TextError::BudgetExceeded)
        );
        assert_eq!(
            standalone_face(&ttc, 2, usize::MAX),
            Err(TextError::InvalidFont)
        );
        assert_eq!(
            standalone_face(&ttc[..30], 0, usize::MAX),
            Err(TextError::InvalidFont)
        );
    }
}

#[cfg(not(feature = "system-fonts"))]
pub(crate) fn standalone_face(_: &[u8], _: u32, _: usize) -> Result<Arc<[u8]>, TextError> {
    Err(TextError::InvalidFont)
}
