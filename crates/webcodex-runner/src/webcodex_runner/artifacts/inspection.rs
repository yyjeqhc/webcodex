use flate2::read::DeflateDecoder;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use webcodex_core::artifact_policy::{
    ooxml_extension_for_mime, preferred_mime_for_path, DOCX_MIME, PPTX_MIME, XLSX_MIME,
};
use xml::reader::{EventReader, XmlEvent};

pub(super) const ARTIFACT_STREAM_BUFFER_BYTES: usize = 64 * 1024;
const ZIP_EOCD_MAX_SEARCH_BYTES: usize = 65_557;
const MAX_OOXML_ZIP_ENTRIES: usize = 4096;
const MAX_OOXML_CENTRAL_DIRECTORY_BYTES: usize = 2 * 1024 * 1024;
const MAX_OOXML_CONTENT_TYPES_BYTES: usize = 256 * 1024;
const MAX_OOXML_CONTENT_TYPE_EVENTS: usize = 4096;
const OOXML_CONTENT_TYPES_NAMESPACE: &str =
    "http://schemas.openxmlformats.org/package/2006/content-types";

pub(super) fn magic_mime(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if data.starts_with(b"\xff\xd8") {
        Some("image/jpeg")
    } else if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else if data.starts_with(b"%PDF-") {
        Some("application/pdf")
    } else if data.starts_with(b"PK\x03\x04") || data.starts_with(b"PK\x05\x06") {
        Some("application/zip")
    } else {
        None
    }
}

#[derive(Clone, Copy)]
struct ZipEntryMetadata {
    flags: u16,
    compression_method: u16,
    compressed_size: usize,
    uncompressed_size: usize,
    local_header_offset: usize,
}

fn le_u16(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        data.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn le_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn zip_eocd_offset(data: &[u8]) -> Option<usize> {
    const EOCD_LEN: usize = 22;
    if data.len() < EOCD_LEN {
        return None;
    }
    let search_start = data.len().saturating_sub(65_557);
    for offset in (search_start..=data.len() - EOCD_LEN).rev() {
        if data.get(offset..offset + 4)? != b"PK\x05\x06" {
            continue;
        }
        let comment_len = usize::from(le_u16(data, offset + 20)?);
        if offset.checked_add(EOCD_LEN)?.checked_add(comment_len)? == data.len() {
            return Some(offset);
        }
    }
    None
}

fn validated_zip_entry_payload<'a>(
    data: &'a [u8],
    central_directory_offset: usize,
    entry: ZipEntryMetadata,
    expected_name: &[u8],
) -> Option<&'a [u8]> {
    if entry.flags & 0x0001 != 0 || !matches!(entry.compression_method, 0 | 8) {
        return None;
    }
    let local = entry.local_header_offset;
    if data.get(local..local.checked_add(4)?)? != b"PK\x03\x04"
        || le_u16(data, local + 6)? != entry.flags
        || le_u16(data, local + 8)? != entry.compression_method
    {
        return None;
    }
    let local_compressed_size = usize::try_from(le_u32(data, local + 18)?).ok()?;
    let local_uncompressed_size = usize::try_from(le_u32(data, local + 22)?).ok()?;
    if entry.flags & 0x0008 == 0 {
        if local_compressed_size != entry.compressed_size
            || local_uncompressed_size != entry.uncompressed_size
        {
            return None;
        }
    } else if (local_compressed_size != 0 && local_compressed_size != entry.compressed_size)
        || (local_uncompressed_size != 0 && local_uncompressed_size != entry.uncompressed_size)
    {
        return None;
    }
    let name_len = usize::from(le_u16(data, local + 26)?);
    let extra_len = usize::from(le_u16(data, local + 28)?);
    let name_start = local.checked_add(30)?;
    let name_end = name_start.checked_add(name_len)?;
    if data.get(name_start..name_end)? != expected_name {
        return None;
    }
    let compressed_start = name_end.checked_add(extra_len)?;
    let compressed_end = compressed_start.checked_add(entry.compressed_size)?;
    if compressed_end > central_directory_offset {
        return None;
    }
    data.get(compressed_start..compressed_end)
}

fn read_ooxml_content_types_entry(
    data: &[u8],
    central_directory_offset: usize,
    entry: ZipEntryMetadata,
) -> Option<Vec<u8>> {
    if entry.compressed_size > MAX_OOXML_CONTENT_TYPES_BYTES
        || entry.uncompressed_size > MAX_OOXML_CONTENT_TYPES_BYTES
    {
        return None;
    }
    let compressed = validated_zip_entry_payload(
        data,
        central_directory_offset,
        entry,
        b"[Content_Types].xml",
    )?;
    let decoded = match entry.compression_method {
        0 => {
            if entry.compressed_size != entry.uncompressed_size {
                return None;
            }
            compressed.to_vec()
        }
        8 => {
            let decoder = DeflateDecoder::new(compressed);
            let mut limited = decoder.take((MAX_OOXML_CONTENT_TYPES_BYTES + 1) as u64);
            let mut decoded = Vec::new();
            limited.read_to_end(&mut decoded).ok()?;
            if decoded.len() > MAX_OOXML_CONTENT_TYPES_BYTES {
                return None;
            }
            decoded
        }
        _ => return None,
    };
    if decoded.len() != entry.uncompressed_size {
        return None;
    }
    Some(decoded)
}

fn ooxml_content_type_mime(content_types: &[u8]) -> Option<&'static str> {
    let parser = EventReader::new(content_types);
    let mut root_seen = false;
    let mut detected = None;
    let mut event_count = 0usize;
    for event in parser {
        event_count = event_count.checked_add(1)?;
        if event_count > MAX_OOXML_CONTENT_TYPE_EVENTS {
            return None;
        }
        let event = event.ok()?;
        if let XmlEvent::StartElement {
            name, attributes, ..
        } = event
        {
            if !root_seen {
                if name.local_name != "Types"
                    || name.namespace.as_deref() != Some(OOXML_CONTENT_TYPES_NAMESPACE)
                {
                    return None;
                }
                root_seen = true;
                continue;
            }
            if name.local_name != "Override"
                || name.namespace.as_deref() != Some(OOXML_CONTENT_TYPES_NAMESPACE)
            {
                continue;
            }
            let mut part_name = None;
            let mut content_type = None;
            for attribute in attributes {
                match attribute.name.local_name.as_str() {
                    "PartName" => part_name = Some(attribute.value),
                    "ContentType" => content_type = Some(attribute.value),
                    _ => {}
                }
            }
            let candidate = match (part_name.as_deref(), content_type.as_deref()) {
                (
                    Some("/word/document.xml"),
                    Some(
                        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
                    ),
                ) => Some(DOCX_MIME),
                (
                    Some("/ppt/presentation.xml"),
                    Some(
                        "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
                    ),
                ) => Some(PPTX_MIME),
                (
                    Some("/xl/workbook.xml"),
                    Some(
                        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
                    ),
                ) => Some(XLSX_MIME),
                _ => None,
            };
            if let Some(candidate) = candidate {
                if detected.is_some_and(|mime| mime != candidate) {
                    return None;
                }
                detected = Some(candidate);
            }
        }
    }
    root_seen.then_some(detected).flatten()
}

fn ooxml_mime(data: &[u8]) -> Option<&'static str> {
    if !data.starts_with(b"PK\x03\x04") {
        return None;
    }
    let eocd = zip_eocd_offset(data)?;
    if le_u16(data, eocd + 4)? != 0 || le_u16(data, eocd + 6)? != 0 {
        return None;
    }
    let entries_on_disk = le_u16(data, eocd + 8)?;
    let total_entries = le_u16(data, eocd + 10)?;
    if entries_on_disk != total_entries || total_entries == u16::MAX {
        return None;
    }
    let entry_count = usize::from(total_entries);
    if entry_count == 0 || entry_count > MAX_OOXML_ZIP_ENTRIES {
        return None;
    }
    let central_directory_size = usize::try_from(le_u32(data, eocd + 12)?).ok()?;
    let central_directory_offset = usize::try_from(le_u32(data, eocd + 16)?).ok()?;
    if central_directory_size > MAX_OOXML_CENTRAL_DIRECTORY_BYTES
        || central_directory_size == u32::MAX as usize
        || central_directory_offset == u32::MAX as usize
    {
        return None;
    }
    let central_directory_end = central_directory_offset.checked_add(central_directory_size)?;
    if central_directory_end != eocd || central_directory_end > data.len() {
        return None;
    }

    let mut content_types_entry = None;
    let mut word_document_entry = None;
    let mut presentation_entry = None;
    let mut workbook_entry = None;
    let mut cursor = central_directory_offset;
    for _ in 0..entry_count {
        if data.get(cursor..cursor.checked_add(4)?)? != b"PK\x01\x02" {
            return None;
        }
        let flags = le_u16(data, cursor + 8)?;
        let compression_method = le_u16(data, cursor + 10)?;
        let compressed_size = usize::try_from(le_u32(data, cursor + 20)?).ok()?;
        let uncompressed_size = usize::try_from(le_u32(data, cursor + 24)?).ok()?;
        let name_len = usize::from(le_u16(data, cursor + 28)?);
        let extra_len = usize::from(le_u16(data, cursor + 30)?);
        let comment_len = usize::from(le_u16(data, cursor + 32)?);
        if le_u16(data, cursor + 34)? != 0 {
            return None;
        }
        let local_header_offset = usize::try_from(le_u32(data, cursor + 42)?).ok()?;
        if compressed_size == u32::MAX as usize
            || uncompressed_size == u32::MAX as usize
            || local_header_offset == u32::MAX as usize
        {
            return None;
        }
        let name_start = cursor.checked_add(46)?;
        let name_end = name_start.checked_add(name_len)?;
        let next = name_end.checked_add(extra_len)?.checked_add(comment_len)?;
        if next > central_directory_end {
            return None;
        }
        let entry = ZipEntryMetadata {
            flags,
            compression_method,
            compressed_size,
            uncompressed_size,
            local_header_offset,
        };
        match data.get(name_start..name_end)? {
            b"[Content_Types].xml" => {
                if content_types_entry.replace(entry).is_some() {
                    return None;
                }
            }
            b"word/document.xml" => {
                if word_document_entry.replace(entry).is_some() {
                    return None;
                }
            }
            b"ppt/presentation.xml" => {
                if presentation_entry.replace(entry).is_some() {
                    return None;
                }
            }
            b"xl/workbook.xml" => {
                if workbook_entry.replace(entry).is_some() {
                    return None;
                }
            }
            _ => {}
        }
        cursor = next;
    }
    if cursor != central_directory_end {
        return None;
    }

    let content_types =
        read_ooxml_content_types_entry(data, central_directory_offset, content_types_entry?)?;
    let (mime, main_part_entry, main_part_name) = match ooxml_content_type_mime(&content_types)? {
        DOCX_MIME => (
            DOCX_MIME,
            word_document_entry?,
            b"word/document.xml".as_slice(),
        ),
        PPTX_MIME => (
            PPTX_MIME,
            presentation_entry?,
            b"ppt/presentation.xml".as_slice(),
        ),
        XLSX_MIME => (XLSX_MIME, workbook_entry?, b"xl/workbook.xml".as_slice()),
        _ => return None,
    };
    validated_zip_entry_payload(
        data,
        central_directory_offset,
        main_part_entry,
        main_part_name,
    )?;
    Some(mime)
}

fn read_file_range(file: &mut File, offset: u64, length: usize) -> Option<Vec<u8>> {
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut data = vec![0_u8; length];
    file.read_exact(&mut data).ok()?;
    Some(data)
}

fn validated_zip_entry_payload_from_file(
    file: &mut File,
    central_directory_offset: usize,
    entry: ZipEntryMetadata,
    expected_name: &[u8],
    read_payload: bool,
) -> Option<Vec<u8>> {
    if entry.flags & 0x0001 != 0 || !matches!(entry.compression_method, 0 | 8) {
        return None;
    }
    let local = entry.local_header_offset;
    let header = read_file_range(file, u64::try_from(local).ok()?, 30)?;
    if header.get(0..4)? != b"PK\x03\x04"
        || le_u16(&header, 6)? != entry.flags
        || le_u16(&header, 8)? != entry.compression_method
    {
        return None;
    }
    let local_compressed_size = usize::try_from(le_u32(&header, 18)?).ok()?;
    let local_uncompressed_size = usize::try_from(le_u32(&header, 22)?).ok()?;
    if entry.flags & 0x0008 == 0 {
        if local_compressed_size != entry.compressed_size
            || local_uncompressed_size != entry.uncompressed_size
        {
            return None;
        }
    } else if (local_compressed_size != 0 && local_compressed_size != entry.compressed_size)
        || (local_uncompressed_size != 0 && local_uncompressed_size != entry.uncompressed_size)
    {
        return None;
    }
    let name_len = usize::from(le_u16(&header, 26)?);
    let extra_len = usize::from(le_u16(&header, 28)?);
    let name_start = local.checked_add(30)?;
    let name_end = name_start.checked_add(name_len)?;
    let compressed_start = name_end.checked_add(extra_len)?;
    let compressed_end = compressed_start.checked_add(entry.compressed_size)?;
    if compressed_end > central_directory_offset {
        return None;
    }
    let name = read_file_range(file, u64::try_from(name_start).ok()?, name_len)?;
    if name != expected_name {
        return None;
    }
    if !read_payload {
        return Some(Vec::new());
    }
    read_file_range(
        file,
        u64::try_from(compressed_start).ok()?,
        entry.compressed_size,
    )
}

fn ooxml_mime_from_file(path: &Path) -> Option<&'static str> {
    let mut file = File::open(path).ok()?;
    let file_bytes = usize::try_from(file.metadata().ok()?.len()).ok()?;
    if file_bytes < 4 || read_file_range(&mut file, 0, 4)?.as_slice() != b"PK\x03\x04" {
        return None;
    }

    let tail_len = file_bytes.min(ZIP_EOCD_MAX_SEARCH_BYTES);
    let tail_start = file_bytes.checked_sub(tail_len)?;
    let tail = read_file_range(&mut file, u64::try_from(tail_start).ok()?, tail_len)?;
    let eocd_in_tail = zip_eocd_offset(&tail)?;
    let eocd = tail_start.checked_add(eocd_in_tail)?;
    if le_u16(&tail, eocd_in_tail + 4)? != 0 || le_u16(&tail, eocd_in_tail + 6)? != 0 {
        return None;
    }
    let entries_on_disk = le_u16(&tail, eocd_in_tail + 8)?;
    let total_entries = le_u16(&tail, eocd_in_tail + 10)?;
    if entries_on_disk != total_entries || total_entries == u16::MAX {
        return None;
    }
    let entry_count = usize::from(total_entries);
    if entry_count == 0 || entry_count > MAX_OOXML_ZIP_ENTRIES {
        return None;
    }
    let central_directory_size = usize::try_from(le_u32(&tail, eocd_in_tail + 12)?).ok()?;
    let central_directory_offset = usize::try_from(le_u32(&tail, eocd_in_tail + 16)?).ok()?;
    if central_directory_size > MAX_OOXML_CENTRAL_DIRECTORY_BYTES
        || central_directory_size == u32::MAX as usize
        || central_directory_offset == u32::MAX as usize
    {
        return None;
    }
    let central_directory_end = central_directory_offset.checked_add(central_directory_size)?;
    if central_directory_end != eocd || central_directory_end > file_bytes {
        return None;
    }
    let central_directory = read_file_range(
        &mut file,
        u64::try_from(central_directory_offset).ok()?,
        central_directory_size,
    )?;

    let mut content_types_entry = None;
    let mut word_document_entry = None;
    let mut presentation_entry = None;
    let mut workbook_entry = None;
    let mut cursor = 0usize;
    for _ in 0..entry_count {
        if central_directory.get(cursor..cursor.checked_add(4)?)? != b"PK\x01\x02" {
            return None;
        }
        let flags = le_u16(&central_directory, cursor + 8)?;
        let compression_method = le_u16(&central_directory, cursor + 10)?;
        let compressed_size = usize::try_from(le_u32(&central_directory, cursor + 20)?).ok()?;
        let uncompressed_size = usize::try_from(le_u32(&central_directory, cursor + 24)?).ok()?;
        let name_len = usize::from(le_u16(&central_directory, cursor + 28)?);
        let extra_len = usize::from(le_u16(&central_directory, cursor + 30)?);
        let comment_len = usize::from(le_u16(&central_directory, cursor + 32)?);
        if le_u16(&central_directory, cursor + 34)? != 0 {
            return None;
        }
        let local_header_offset = usize::try_from(le_u32(&central_directory, cursor + 42)?).ok()?;
        if compressed_size == u32::MAX as usize
            || uncompressed_size == u32::MAX as usize
            || local_header_offset == u32::MAX as usize
        {
            return None;
        }
        let name_start = cursor.checked_add(46)?;
        let name_end = name_start.checked_add(name_len)?;
        let next = name_end.checked_add(extra_len)?.checked_add(comment_len)?;
        if next > central_directory.len() {
            return None;
        }
        let entry = ZipEntryMetadata {
            flags,
            compression_method,
            compressed_size,
            uncompressed_size,
            local_header_offset,
        };
        match central_directory.get(name_start..name_end)? {
            b"[Content_Types].xml" => {
                if content_types_entry.replace(entry).is_some() {
                    return None;
                }
            }
            b"word/document.xml" => {
                if word_document_entry.replace(entry).is_some() {
                    return None;
                }
            }
            b"ppt/presentation.xml" => {
                if presentation_entry.replace(entry).is_some() {
                    return None;
                }
            }
            b"xl/workbook.xml" => {
                if workbook_entry.replace(entry).is_some() {
                    return None;
                }
            }
            _ => {}
        }
        cursor = next;
    }
    if cursor != central_directory.len() {
        return None;
    }

    let content_types_entry = content_types_entry?;
    if content_types_entry.compressed_size > MAX_OOXML_CONTENT_TYPES_BYTES
        || content_types_entry.uncompressed_size > MAX_OOXML_CONTENT_TYPES_BYTES
    {
        return None;
    }
    let compressed = validated_zip_entry_payload_from_file(
        &mut file,
        central_directory_offset,
        content_types_entry,
        b"[Content_Types].xml",
        true,
    )?;
    let content_types = match content_types_entry.compression_method {
        0 => {
            if content_types_entry.compressed_size != content_types_entry.uncompressed_size {
                return None;
            }
            compressed
        }
        8 => {
            let decoder = DeflateDecoder::new(compressed.as_slice());
            let mut limited = decoder.take((MAX_OOXML_CONTENT_TYPES_BYTES + 1) as u64);
            let mut decoded = Vec::new();
            limited.read_to_end(&mut decoded).ok()?;
            if decoded.len() > MAX_OOXML_CONTENT_TYPES_BYTES {
                return None;
            }
            decoded
        }
        _ => return None,
    };
    if content_types.len() != content_types_entry.uncompressed_size {
        return None;
    }
    let (mime, main_part_entry, main_part_name) = match ooxml_content_type_mime(&content_types)? {
        DOCX_MIME => (
            DOCX_MIME,
            word_document_entry?,
            b"word/document.xml".as_slice(),
        ),
        PPTX_MIME => (
            PPTX_MIME,
            presentation_entry?,
            b"ppt/presentation.xml".as_slice(),
        ),
        XLSX_MIME => (XLSX_MIME, workbook_entry?, b"xl/workbook.xml".as_slice()),
        _ => return None,
    };
    validated_zip_entry_payload_from_file(
        &mut file,
        central_directory_offset,
        main_part_entry,
        main_part_name,
        false,
    )?;
    Some(mime)
}

pub(super) fn artifact_mime(path: &str, data: &[u8], sniff_json: bool) -> Option<String> {
    if let Some(mime) = ooxml_mime(data) {
        return Some(mime.to_string());
    }
    let mut mime =
        preferred_mime_for_path(path).filter(|mime| ooxml_extension_for_mime(mime).is_none());
    if let Some(magic) = magic_mime(data) {
        mime = Some(magic);
    } else if sniff_json {
        let first = data.iter().copied().find(|b| !b.is_ascii_whitespace());
        if matches!(first, Some(b'{') | Some(b'[')) {
            mime = Some("application/json");
        }
    }
    mime.map(str::to_string)
}

pub(super) fn artifact_mime_from_file(
    path: &str,
    file_path: &Path,
    sniff_json: bool,
) -> Option<String> {
    if let Some(mime) = ooxml_mime_from_file(file_path) {
        return Some(mime.to_string());
    }
    let mut file = File::open(file_path).ok()?;
    let prefix_len = usize::try_from(file.metadata().ok()?.len()).ok()?.min(32);
    let prefix = read_file_range(&mut file, 0, prefix_len)?;
    let mut mime =
        preferred_mime_for_path(path).filter(|mime| ooxml_extension_for_mime(mime).is_none());
    if let Some(magic) = magic_mime(&prefix) {
        mime = Some(magic);
    } else if sniff_json {
        let mut first = prefix
            .iter()
            .copied()
            .find(|byte| !byte.is_ascii_whitespace());
        if first.is_none() {
            let mut buffer = [0_u8; ARTIFACT_STREAM_BUFFER_BYTES];
            loop {
                let read = file.read(&mut buffer).ok()?;
                if read == 0 {
                    break;
                }
                first = buffer[..read]
                    .iter()
                    .copied()
                    .find(|byte| !byte.is_ascii_whitespace());
                if first.is_some() {
                    break;
                }
            }
        }
        if matches!(first, Some(b'{') | Some(b'[')) {
            mime = Some("application/json");
        }
    }
    mime.map(str::to_string)
}

pub(super) fn read_file_range_with_digest(
    path: &Path,
    max_bytes: usize,
    offset: usize,
    length: usize,
) -> Result<(usize, String, Vec<u8>), String> {
    let requested_end = offset
        .checked_add(length)
        .ok_or_else(|| "offset + length overflow".to_string())?;
    let mut file = File::open(path).map_err(|e| format!("read failed: {e}"))?;
    let mut buffer = [0_u8; ARTIFACT_STREAM_BUFFER_BYTES];
    let mut bytes = 0usize;
    let mut sha256 = Sha256::new();
    let mut segment = Vec::with_capacity(length);
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|e| format!("read failed: {e}"))?;
        if read == 0 {
            break;
        }
        let chunk_start = bytes;
        bytes = bytes
            .checked_add(read)
            .ok_or_else(|| "artifact size overflow".to_string())?;
        if bytes > max_bytes {
            return Err("artifact too large to inspect".to_string());
        }
        sha256.update(&buffer[..read]);

        let overlap_start = offset.max(chunk_start);
        let overlap_end = requested_end.min(bytes);
        if overlap_start < overlap_end {
            let local_start = overlap_start - chunk_start;
            let local_end = overlap_end - chunk_start;
            segment.extend_from_slice(&buffer[local_start..local_end]);
        }
    }
    Ok((bytes, format!("{:x}", sha256.finalize()), segment))
}

pub(super) fn verify_upload_file(path: &Path, max_bytes: usize) -> Result<(usize, String), String> {
    let mut file = File::open(path).map_err(|e| format!("read failed: {e}"))?;
    let mut buffer = [0_u8; ARTIFACT_STREAM_BUFFER_BYTES];
    let mut bytes = 0usize;
    let mut sha256 = Sha256::new();
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|e| format!("read failed: {e}"))?;
        if read == 0 {
            break;
        }
        bytes = bytes
            .checked_add(read)
            .ok_or_else(|| "artifact size overflow".to_string())?;
        if bytes > max_bytes {
            return Err("artifact too large to inspect".to_string());
        }
        sha256.update(&buffer[..read]);
    }
    Ok((bytes, format!("{:x}", sha256.finalize())))
}

fn png_size(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() >= 24 && data.starts_with(b"\x89PNG\r\n\x1a\n") {
        let width = u32::from_be_bytes(data[16..20].try_into().ok()?);
        let height = u32::from_be_bytes(data[20..24].try_into().ok()?);
        Some((width, height))
    } else {
        None
    }
}

fn webp_size(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() >= 30
        && data.starts_with(b"RIFF")
        && &data[8..12] == b"WEBP"
        && &data[12..16] == b"VP8X"
    {
        let width =
            1 + u32::from(data[24]) + (u32::from(data[25]) << 8) + (u32::from(data[26]) << 16);
        let height =
            1 + u32::from(data[27]) + (u32::from(data[28]) << 8) + (u32::from(data[29]) << 16);
        Some((width, height))
    } else {
        None
    }
}

fn jpeg_size(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 4 || !data.starts_with(b"\xff\xd8") {
        return None;
    }
    let mut i = 2;
    while i + 9 < data.len() {
        if data[i] != 0xff {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        i += 2;
        if matches!(
            marker,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        ) {
            let height = u16::from_be_bytes(data[i + 3..i + 5].try_into().ok()?);
            let width = u16::from_be_bytes(data[i + 5..i + 7].try_into().ok()?);
            return Some((u32::from(width), u32::from(height)));
        }
        if i + 2 > data.len() {
            break;
        }
        let segment_len = usize::from(u16::from_be_bytes(data[i..i + 2].try_into().ok()?));
        if segment_len < 2 {
            break;
        }
        i = i.saturating_add(segment_len);
    }
    None
}

pub(super) fn image_size(data: &[u8]) -> Option<(u32, u32)> {
    png_size(data)
        .or_else(|| jpeg_size(data))
        .or_else(|| webp_size(data))
}

pub(super) fn zip_entry_count(data: &[u8]) -> Option<u16> {
    let eocd = zip_eocd_offset(data)?;
    le_u16(data, eocd + 10)
}

pub(super) fn read_limited(path: &Path, max_bytes: usize) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("read failed: {}", e))?;
    let mut limited = file.take(max_bytes.saturating_add(1) as u64);
    let mut data = Vec::new();
    limited
        .read_to_end(&mut data)
        .map_err(|e| format!("read failed: {}", e))?;
    if data.len() > max_bytes {
        return Err("artifact too large to inspect".to_string());
    }
    Ok(data)
}
