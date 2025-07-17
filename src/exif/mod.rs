use color_eyre::eyre::Result;
use exif::Tag;
use png::text_metadata::TEXtChunk;

pub fn create_exif_from_png_chunks(text_chunks: &[TEXtChunk]) -> Result<Option<Vec<u8>>> {
    let mut entries: Vec<(Tag, Vec<u8>)> = Vec::new();

    for chunk in text_chunks {
        match chunk.keyword.as_str() {
            "prompt" => {
                entries.push((Tag::Make, format!("Prompt: {}", chunk.text).into_bytes()));
            }
            "workflow" => {
                entries.push((
                    Tag::ImageDescription,
                    format!("Workflow: {}", chunk.text).into_bytes(),
                ));
            }
            _ => {
                // Store other chunks as UserComment
                let comment = format!("{}: {}", chunk.keyword, chunk.text);
                entries.push((Tag::UserComment, comment.into_bytes()));
            }
        }
    }

    if entries.is_empty() {
        return Ok(None);
    }

    let mut exif_data: Vec<u8> = Vec::new();

    // 1. TIFF Header
    exif_data.extend_from_slice(&[0x4D, 0x4D, 0x00, 0x2A, 0x00, 0x00, 0x00, 0x08]); // Little-Endian, Offset to IFD

    // 2. IFD
    let num_ifd_entries = entries.len() as u16;
    exif_data.extend_from_slice(&num_ifd_entries.to_be_bytes());

    // Calculate the base offset for the tag values (after the IFD entries and next IFD offset)
    let value_offset_base = 8 + 2 + (12 * num_ifd_entries as usize) + 4;

    let mut current_value_offset = value_offset_base;

    for (tag, value) in &entries {
        // Entry: Tag, Type, Count, ValueOffset
        exif_data.extend_from_slice(&tag.1.to_be_bytes()); // Tag
        exif_data.extend_from_slice(&[0x00, 0x02]); // Type: ASCII (0x0002)
        exif_data.extend_from_slice(&(value.len() as u32 + 1).to_be_bytes()); // Count (String length + null terminator)
        exif_data.extend_from_slice(&(current_value_offset as u32).to_be_bytes()); // ValueOffset
        current_value_offset += value.len() + 1;
    }

    exif_data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // Next IFD offset (0 for none)

    // 3. Tag Values
    for (_, value) in &entries {
        exif_data.extend_from_slice(value);
        exif_data.push(0x00); // Null terminator
    }

    // 4. EXIF Header
    let mut exif_header: Vec<u8> = vec![0x45, 0x78, 0x69, 0x66, 0x00, 0x00]; // "Exif\0\0"
    let len_bytes = 6u32.to_be_bytes(); // Length of "Exif\0\0" + offset

    exif_header.extend(exif_data);
    exif_header.splice(0..0, len_bytes.iter().copied());

    Ok(Some(exif_header))
}
