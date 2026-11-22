use anyhow::Result;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

fn family(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".gguf").unwrap_or(&lower);
    stem.split(['-', '_', '.'])
        .filter(|token| {
            !matches!(
                *token,
                "mmproj"
                    | "projector"
                    | "f16"
                    | "bf16"
                    | "f32"
                    | "gguf"
                    | "k"
                    | "m"
                    | "s"
                    | "xs"
                    | "xxs"
                    | "vision"
                    | "proj"
                    | "of"
            ) && !(token.len() == 5 && token.bytes().all(|byte| byte.is_ascii_digit()))
                && !(token.starts_with('q')
                    && token.as_bytes().get(1).is_some_and(u8::is_ascii_digit))
                && !(token.starts_with("iq")
                    && token.as_bytes().get(2).is_some_and(u8::is_ascii_digit))
        })
        .collect()
}

pub fn projector(model: &Path, extra: &[String]) -> Result<Option<PathBuf>> {

    if extra.iter().any(|arg| {
        arg == "--mmproj"
            || arg == "--mmproj-url"
            || arg == "--no-mmproj"
            || arg.starts_with("--mmproj=")
            || arg.starts_with("--mmproj-url=")
    }) {
        return Ok(None);
    }
    let Some(parent) = model.parent() else {
        return Ok(None);
    };
    let fields = gguf_metadata(model).unwrap_or_default();
    let mut families = vec![family(
        &model.file_name().unwrap_or_default().to_string_lossy(),
    )];
    for (key, value) in &fields {
        if key == "general.name" || key.starts_with("general.base_model.") && key.ends_with(".name")
        {
            families.push(family(value));
        }
    }
    let mut candidates = Vec::new();
    for folder in [
        parent.to_path_buf(),
        parent.join("mmproj"),
        parent.join("projector"),
        parent.join("vision"),
    ] {
        let Ok(entries) = std::fs::read_dir(folder) else {
            continue;
        };
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if !name.ends_with(".gguf")
                || !(name.starts_with("mmproj-")
                    || name.starts_with("projector-")
                    || name.starts_with("vision-proj-")
                    || name.contains("-mmproj-"))
                || !entry.metadata()?.is_file()
            {
                continue;
            }
            if name.contains("-of-") && !name.contains("-00001-of-") {
                continue;
            }
            let key = family(&name);
            let matching = families.iter().any(|main| {
                !key.is_empty()
                    && (main == &key
                        || (main.contains(&key) || key.contains(main))
                            && key.len().min(main.len()) >= 6
                            && key.len().min(main.len()) * 100 / key.len().max(main.len()) >= 55)
            });
            if (matching || key.is_empty()) && complete_shards(&entry.path()) && is_vision_projector(&entry.path()) {
                candidates.push((matching, name.contains("-f16"), name, entry.path()));
            }
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
    Ok(candidates.into_iter().next().map(|entry| entry.3))
}

fn complete_shards(file: &Path) -> bool {
    let name=file.file_name().unwrap_or_default().to_string_lossy();
    let Some(stem)=name.strip_suffix(".gguf") else {return false};
    let Some((prefix,count))=stem.rsplit_once("-00001-of-") else {return true};
    let Ok(total)=count.parse::<usize>() else {return false};
    if total==0||total>9999{return false}
    let Some(parent)=file.parent() else {return false};
    (1..=total).all(|index|parent.join(format!("{prefix}-{index:05}-of-{count}.gguf")).is_file())
}


fn integer(reader: &mut std::fs::File, count: usize) -> Result<u64> {
    let mut bytes = [0u8; 8];
    reader.read_exact(&mut bytes[..count])?;
    Ok(u64::from_le_bytes(bytes))
}
fn field(
    reader: &mut std::fs::File,
    kind: u32,
    capture: bool,
    depth: usize,
) -> Result<Option<String>> {
    anyhow::ensure!(depth < 8, "GGUF metadata is nested too deeply");
    if kind == 8 {
        let size = integer(reader, 8)?;
        anyhow::ensure!(size <= 64 * 1024 * 1024, "GGUF metadata field is too large");
        if capture {
            let mut bytes = vec![0; size as usize];
            reader.read_exact(&mut bytes)?;
            return Ok(Some(String::from_utf8_lossy(&bytes).into_owned()));
        }
        reader.seek(SeekFrom::Current(size as i64))?;
        return Ok(None);
    }
    if kind == 9 {
        let item = integer(reader, 4)? as u32;
        let count = integer(reader, 8)?;
        anyhow::ensure!(count <= 1_000_000, "GGUF array is too large");
        for _ in 0..count {
            field(reader, item, false, depth + 1)?;
        }
        return Ok(None);
    }
    let size = match kind {
        0 | 1 | 7 => 1,
        2 | 3 => 2,
        4 | 5 | 6 => 4,
        10 | 11 | 12 => 8,
        _ => anyhow::bail!("Unknown GGUF metadata type"),
    };
    let value = integer(reader, size)?;
    Ok(if capture {
        Some(if kind == 7 {
            (value != 0).to_string()
        } else {
            value.to_string()
        })
    } else {
        None
    })
}
fn gguf_metadata(path: &Path) -> Result<HashMap<String, String>> {
    let mut file = std::fs::File::open(path)?;
    let mut magic = [0; 4];
    file.read_exact(&mut magic)?;
    anyhow::ensure!(&magic == b"GGUF", "Missing GGUF header");
    let version = integer(&mut file, 4)?;
    anyhow::ensure!((2..=4).contains(&version), "Unsupported GGUF version");
    anyhow::ensure!(integer(&mut file, 8)? > 0, "GGUF has no tensors");
    let count = integer(&mut file, 8)?;
    anyhow::ensure!(count <= 10000, "Too many GGUF metadata entries");
    let mut result = HashMap::new();
    for _ in 0..count {
        let key = field(&mut file, 8, true, 0)?.unwrap_or_default();
        let kind = integer(&mut file, 4)? as u32;
        let wanted = key == "general.architecture"
            || key == "general.name"
            || key.starts_with("general.base_model.")
            || key == "clip.has_vision_encoder"
            || key == "clip.has_audio_encoder";
        if let Some(value) = field(&mut file, kind, wanted, 0)? {
            result.insert(key, value);
        }
    }
    Ok(result)
}
fn is_vision_projector(path: &Path) -> bool {
    let Ok(fields) = gguf_metadata(path) else {
        return false;
    };
    if let Some(vision) = fields.get("clip.has_vision_encoder") {
        return vision == "true";
    }
    fields
        .get("general.architecture")
        .is_some_and(|value| value == "clip")
        && !fields
            .get("clip.has_audio_encoder")
            .is_some_and(|value| value == "true")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn projector_fixture() -> Vec<u8> {
        let mut value = b"GGUF".to_vec();
        value.extend(3u32.to_le_bytes());
        value.extend(1u64.to_le_bytes());
        value.extend(1u64.to_le_bytes());
        let key = b"general.architecture";
        value.extend((key.len() as u64).to_le_bytes());
        value.extend(key);
        value.extend(8u32.to_le_bytes());
        value.extend(4u64.to_le_bytes());
        value.extend(b"clip");
        value
    }
    #[test]
    fn swift_derivatives_load_matching_projector_without_overriding_user_args() {
        let dir = std::env::temp_dir().join(format!("jynx-vision-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let model = dir.join("Swift-1.5-Qwen3.8-27B-GSQ-RCO-IQ3_S.gguf");
        let matching = dir.join("mmproj-Swift-1.5-Qwen3.8-27B-F16.gguf");
        std::fs::write(&matching, projector_fixture()).unwrap();
        std::fs::write(dir.join("mmproj-Other-8B-F16.gguf"), projector_fixture()).unwrap();
        assert_eq!(projector(&model, &[]).unwrap(), Some(matching));
        assert!(
            projector(&model, &["--mmproj=explicit.gguf".into()])
                .unwrap()
                .is_none()
        );
        assert!(
            projector(&dir.join("Unrelated-32B.gguf"), &[])
                .unwrap()
                .is_none()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn arbitrary_model_families_and_nested_projectors_are_supported_but_audio_is_rejected() {
        let root = std::env::temp_dir().join(format!("jynx-general-vision-{}", std::process::id()));
        for family in [
            "NovelVision-4B",
            "Gemma-3-12B",
            "MiniCPM-V-8B",
            "Qwen3-VL-8B",
        ] {
            let dir = root.join(family);
            std::fs::create_dir_all(dir.join("vision")).unwrap();
            let model = dir.join(format!("{family}-Q4_K_M-00001-of-00002.gguf"));
            let projection = dir.join("vision").join(format!("mmproj-{family}-F16.gguf"));
            std::fs::write(&projection, projector_fixture()).unwrap();
            assert_eq!(projector(&model, &[]).unwrap(), Some(projection.clone()));
            let mut audio = projector_fixture();
            audio[16..24].copy_from_slice(&2u64.to_le_bytes());
            let key = b"clip.has_vision_encoder";
            audio.extend((key.len() as u64).to_le_bytes());
            audio.extend(key);
            audio.extend(7u32.to_le_bytes());
            audio.push(0);
            std::fs::write(&projection, audio).unwrap();
            assert!(projector(&model, &[]).unwrap().is_none());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
