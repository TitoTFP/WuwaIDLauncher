use repak::{PakBuilder, Version};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Component, Path, PathBuf};

const V12_MOUNT_POINT: &str = "../../../";
const V12_PATH_HASH_SEED: u64 = 0;

fn repak_error(context: &str, message: impl std::fmt::Display) -> String {
    format!("repak_{context}: {message}")
}

fn relative_entry_path(mount_point: &str, entry: &str) -> Result<PathBuf, String> {
    let mount = Path::new(mount_point);
    let full_path = mount.join(entry);
    let relative = full_path
        .strip_prefix(mount)
        .map_err(|_| repak_error("path_prefix", entry))?;

    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::ParentDir
            )
        })
    {
        return Err(repak_error("path_invalid", entry));
    }

    Ok(relative.to_path_buf())
}

fn collect_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(directory).map_err(|cause| repak_error("read_dir", cause))?;
    for entry in entries {
        let entry = entry.map_err(|cause| repak_error("read_dir_entry", cause))?;
        let file_type = entry
            .file_type()
            .map_err(|cause| repak_error("file_type", cause))?;
        let path = entry.path();
        if file_type.is_symlink() {
            return Err(repak_error("symlink_rejected", path.display()));
        }
        if file_type.is_dir() {
            collect_files(root, &path, files)?;
        } else if file_type.is_file() {
            if path.strip_prefix(root).is_err() {
                return Err(repak_error("file_outside_input", path.display()));
            }
            files.push(path);
        } else {
            return Err(repak_error("file_type_unsupported", path.display()));
        }
    }
    Ok(())
}

pub fn unpack_v12(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_file() {
        return Err(repak_error("source_missing", source.display()));
    }
    if destination.exists() {
        let mut entries =
            fs::read_dir(destination).map_err(|cause| repak_error("read_dir", cause))?;
        if entries
            .next()
            .transpose()
            .map_err(|cause| repak_error("read_dir_entry", cause))?
            .is_some()
        {
            return Err(repak_error("output_not_empty", destination.display()));
        }
    } else {
        fs::create_dir_all(destination).map_err(|cause| repak_error("create_output", cause))?;
    }

    let mut input =
        BufReader::new(File::open(source).map_err(|cause| repak_error("open_source", cause))?);
    let reader = PakBuilder::new()
        .reader_with_version(&mut input, Version::V12)
        .map_err(|cause| repak_error("read_v12", cause))?;
    let entries = reader.files();
    if entries.is_empty() {
        return Err(repak_error("empty_pak", source.display()));
    }

    for entry in entries {
        let relative = relative_entry_path(reader.mount_point(), &entry)?;
        let output = destination.join(&relative);
        let parent = output
            .parent()
            .ok_or_else(|| repak_error("output_parent", output.display()))?;
        fs::create_dir_all(parent).map_err(|cause| repak_error("create_parent", cause))?;
        let mut file = File::create(&output).map_err(|cause| repak_error("create_entry", cause))?;
        reader
            .read_file(&entry, &mut input, &mut file)
            .map_err(|cause| repak_error("read_entry", cause))?;
        file.sync_all()
            .map_err(|cause| repak_error("sync_entry", cause))?;
    }

    Ok(())
}

pub fn pack_v12(input_directory: &Path, destination: &Path) -> Result<(), String> {
    if !input_directory.is_dir() {
        return Err(repak_error("input_missing", input_directory.display()));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|cause| repak_error("create_destination_parent", cause))?;
    }

    let mut files = Vec::new();
    collect_files(input_directory, input_directory, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err(repak_error("input_empty", input_directory.display()));
    }

    let output = BufWriter::new(
        File::create(destination).map_err(|cause| repak_error("create_destination", cause))?,
    );
    let mut writer = PakBuilder::new().writer(
        output,
        Version::V12,
        V12_MOUNT_POINT.to_string(),
        Some(V12_PATH_HASH_SEED),
    );

    for path in files {
        let relative = path
            .strip_prefix(input_directory)
            .map_err(|cause| repak_error("relative_path", cause))?;
        let relative = relative.to_string_lossy().replace('\\', "/");
        if relative.is_empty() || relative.starts_with('/') || relative.contains("../") {
            return Err(repak_error("input_path_invalid", relative));
        }
        let bytes = fs::read(&path).map_err(|cause| repak_error("read_entry", cause))?;
        writer
            .write_file(&relative, false, bytes)
            .map_err(|cause| repak_error("write_entry", cause))?;
    }

    let mut output = writer
        .write_index()
        .map_err(|cause| repak_error("write_index", cause))?;
    output
        .flush()
        .map_err(|cause| repak_error("flush_destination", cause))?;
    output
        .get_ref()
        .sync_all()
        .map_err(|cause| repak_error("sync_destination", cause))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::installer;

    fn create_fixture(path: &Path) {
        let output = BufWriter::new(File::create(path).unwrap());
        let mut writer = PakBuilder::new().writer(
            output,
            Version::V12,
            V12_MOUNT_POINT.to_string(),
            Some(V12_PATH_HASH_SEED),
        );
        writer
            .write_file("Client/Content/example.txt", false, b"fixture")
            .unwrap();
        let mut output = writer.write_index().unwrap();
        output.flush().unwrap();
    }

    #[test]
    fn round_trips_v12_entries() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.pak");
        let unpacked = temp.path().join("unpacked");
        let packed = temp.path().join("packed.pak");
        create_fixture(&source);

        unpack_v12(&source, &unpacked).unwrap();
        assert_eq!(
            fs::read(unpacked.join("Client/Content/example.txt")).unwrap(),
            b"fixture"
        );
        pack_v12(&unpacked, &packed).unwrap();
        assert!(installer::validate_pak_file(&packed).unwrap());
    }
}
