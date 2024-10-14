#![allow(dead_code)]

use std::env;

use std::fs;
use std::fs::File;

use std::io::Read;
use std::io::Write;

use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;

use flate2::Compression;

use crypto::digest::Digest;
use crypto::sha1::Sha1;

#[derive(Debug)]
enum GitError {
    FailedToReadGitObjectFile(String),
    InvalidGitObject,
    ZlibDecompressionFailed(String),
    InvalidDecompressSize,
    UnknownGitType,
    UnknownEntryMode,
    InvalidTreeEntry,
	CreateBlob(String),
	CreateTree(String),
}

struct GitObjectParts<T> {
    git_type: String,
    size: usize,
    content: T,
}

#[derive(Debug)]
enum GitObject {
    Blob { content: String },
    Tree { content: Vec<TreeEntry> },
    Commit,
}

impl GitObject {
    fn from_parts_string(parts: GitObjectParts<String>) -> Result<Self, GitError> {
        if parts.size != parts.content.len() {
            return Err(GitError::InvalidGitObject);
        }

        match parts.git_type.as_str() {
            "blob" => Ok(GitObject::Blob {
                content: parts.content,
            }),
            _ => Err(GitError::UnknownGitType),
        }
    }

    fn from_parts_bytes(parts: GitObjectParts<Vec<u8>>) -> Result<Self, GitError> {
        if parts.size != parts.content.len() {
            return Err(GitError::InvalidGitObject);
        }

        match parts.git_type.as_str() {
            "tree" => {
                let content: Vec<TreeEntry> = parse_str_tree_entry_vec(&parts.content)?;
                Ok(GitObject::Tree { content })
            }
            _ => Err(GitError::UnknownGitType),
        }
    }

    fn create_blob_with_content(content: String) -> Self {
        GitObject::Blob { content }
    }

    fn as_string(&self) -> String {
        match self {
            GitObject::Blob { content } => format!("blob {}\0{content}", content.len()),
            _ => unimplemented!(),
        }
    }

    fn get_type(&self) -> String {
        match self {
            GitObject::Blob { .. } => "blob".to_string(),
            GitObject::Tree { .. } => "tree".to_string(),
            GitObject::Commit => "commit".to_string(),
        }
    }

    fn get_size(&self) -> usize {
        match self {
            GitObject::Blob { content } => content.len(),
            _ => unimplemented!(),
        }
    }

    fn get_blob_content(&self) -> &str {
        match self {
            GitObject::Blob { content } => content,
            _ => unimplemented!(),
        }
    }

    fn get_tree_content(&self) -> &Vec<TreeEntry> {
        match self {
            GitObject::Tree { content } => content,
            _ => unimplemented!(),
        }
    }
}

#[derive(Debug)]
struct TreeEntry {
    mode: EntryMode,
    name: String,
    sha1_hash: String,
}

impl TreeEntry {
    fn from_bytes(bytes: &[u8]) -> Result<Self, GitError> {
        let Ok((mode, name, byte_sha_hex)) = parse_tree_entry_bytes(bytes) else {
            return Err(GitError::InvalidGitObject);
        };
        let mode = EntryMode::from_mode_value(mode)?;
        Ok(Self{ mode, name, sha1_hash: byte_sha_hex })
    }
}

fn parse_tree_entry_bytes(teb: &[u8]) -> Result<(usize, String, String), GitError> {
    let mut mode = String::new();

    let mut index: usize = 0;
    loop {
        if teb[index].eq(&b' ') {
            break;
        }

        mode.push(teb[index] as char);
        index += 1;
    }
    index += 1;

    let mode: usize = match mode.parse::<usize>() {
        Ok(value) => value,
        Err(_err) => return Err(GitError::InvalidTreeEntry),
    };

    let mut name = String::new();

    loop {
        if teb[index].eq(&b'\0') {
            break;
        }

        name.push(teb[index] as char);
        index += 1;
    }
    index += 1;

    let byte_sha: Vec<u8> = teb[index..teb.len()].to_vec();
    let byte_sha_hex: String = bytes_slice_to_hex(&byte_sha[..]);

    Ok((mode, name, byte_sha_hex))
}

fn bytes_slice_to_hex(slice: &[u8]) -> String {
    let hex: String = format!("{slice:02x?}");
    hex.replace(", ", "").replace(['[', ']'], "")
}

#[derive(Debug)]
enum EntryMode {
    RegularFile = 100644,
    ExecutableFile = 100755,
    SymbolicLink = 120000,
    Directory = 40000,
}

impl EntryMode {
    fn from_mode_value(value: usize) -> Result<Self, GitError> {
        match value {
            100644 => Ok(EntryMode::RegularFile),
            100755 => Ok(EntryMode::ExecutableFile),
            120000 => Ok(EntryMode::SymbolicLink),
            40000 => Ok(EntryMode::Directory),
            _ => Err(GitError::UnknownEntryMode)
        }
    }
}

const GIT_COMMAND_INIT: &str = "init";
const GIT_COMMAND_CAT_FILE: &str = "cat-file";
const GIT_COMMAND_HASH_OBJECT: &str = "hash-object";
const GIT_COMMAND_LS_TREE: &str = "ls-tree";

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Need at least 1 argument.");
        return;
    }

    match args[1].as_str() {
        GIT_COMMAND_INIT => git_init(),
        GIT_COMMAND_CAT_FILE => git_cat_file(&args[..]),
        GIT_COMMAND_HASH_OBJECT => git_hash_object(&args[..]),
        GIT_COMMAND_LS_TREE => git_ls_tree(&args[..]),
        _ => println!("unknown command: {}", args[1]),
    }
}

fn git_init() {
    fs::create_dir(".git").unwrap();
    fs::create_dir(".git/objects").unwrap();
    fs::create_dir(".git/refs").unwrap();
    fs::write(".git/HEAD", "ref: refs/heads/main\n").unwrap();
    println!("Initialized git directory");
}

fn git_cat_file(args: &[String]) {
    if args.len() < 3 {
        println!("git cat-file needs 2 arguments.");
        return;
    }

    let (option, blob_sha): (Option<&str>, &str) = if args.len() == 3 {
        (None, args[2].as_str())
    } else {
        (Some(args[2].as_str()), args[3].as_str())
    };

    let (folder_path, file_name): (String, String) = sha1_to_file_path(blob_sha);
    let file_path: String = format!("{folder_path}/{file_name}");

    let file: File = match File::open(file_path) {
        Ok(file) => file,
        Err(err) => {
            println!("File::open: {err}");
            return;
        }
    };

    let bytes: Vec<u8> = match get_file_bytes(file) {
        Ok(bytes) => bytes,
        Err(err) => {
            println!("get_file_bytes: {err}");
            return;
        }
    };

    let decompressed_bytes: Vec<u8> = match zlib_decompression(&bytes[..]) {
        Ok(s) => s,
        Err(err) => {
            println!("zlib_decompression: {err}");
            return;
        }
    };

    let content: String = match String::from_utf8(decompressed_bytes) {
        Ok(s) => s,
        Err(err) => {
            println!("String::from_utf8: {err}");
            return;
        }
    };

    let git_object_parts: GitObjectParts<String> =
        match parse_str_to_git_object_parts_string(&content) {
            Ok(parts) => parts,
            Err(err) => {
                println!("parse_str_to_git_object_parts: {err:?}");
                return;
            }
        };

    let git_object: GitObject = match GitObject::from_parts_string(git_object_parts) {
        Ok(git_object) => git_object,
        Err(err) => {
            println!("GitObject::from_parts: {err:?}");
            return;
        }
    };

    if let Some(option) = option {
        if option.eq("-p") {
            print!("{}", git_object.get_blob_content());
        }
    }
}

fn git_hash_object(args: &[String]) {
    if args.len() < 3 {
        println!("git hash-object needs 2 arguments.");
        return;
    }

    let (option, file_path): (Option<&str>, &str) = if args.len() == 3 {
        (None, args[2].as_str())
    } else {
        (Some(args[2].as_str()), args[3].as_str())
    };

    let mut file: File = match File::open(file_path) {
        Ok(file) => file,
        Err(err) => {
            println!("File::open: {err}");
            return;
        }
    };

    let mut content: String = String::new();
    let _read_bytes: usize = match file.read_to_string(&mut content) {
        Ok(read_bytes) => read_bytes,
        Err(err) => {
            println!("File::read_to_string: {err}");
            return;
        }
    };

    let git_object = GitObject::create_blob_with_content(content);
    let str_git_object: String = git_object.as_string();
    let sha1_hash: String = compute_sha1_hash(&str_git_object);
    let bytes: Vec<u8> = match zlib_compression(&str_git_object) {
        Ok(bytes) => bytes,
        Err(err) => {
            println!("zlib_compression: {err}");
            return;
        }
    };

    if let Some(option) = option {
        if option.eq("-w") {
            let (folder_path, file_name): (String, String) = sha1_to_file_path(&sha1_hash);
            match write_bytes_to_file(&folder_path, &file_name, &bytes[..]) {
                Ok(()) => {}
                Err(err) => {
                    println!("write_bytes_to_file: {err}");
                }
            }
        }
    }

    println!("{sha1_hash}");
}

fn git_ls_tree(args: &[String]) {
    if args.len() < 3 {
        println!("git ls-tree needs at least 2 arguments.");
        return;
    }

    let (option, blob_sha): (Option<&str>, &str) = if args.len() == 3 {
        (None, args[2].as_str())
    } else {
        (Some(args[2].as_str()), args[3].as_str())
    };

    let (folder_path, file_name): (String, String) = sha1_to_file_path(blob_sha);
    let file_path: String = format!("{folder_path}/{file_name}");

    let file: File = match File::open(file_path) {
        Ok(file) => file,
        Err(err) => {
            println!("File::open: {err}");
            return;
        }
    };

    let bytes: Vec<u8> = match get_file_bytes(file) {
        Ok(bytes) => bytes,
        Err(err) => {
            println!("get_file_bytes: {err}");
            return;
        }
    };

    let decompressed_bytes: Vec<u8> = match zlib_decompression(&bytes[..]) {
        Ok(s) => s,
        Err(err) => {
            println!("zlib_decompression: {err}");
            return;
        }
    };

    let git_object_parts: GitObjectParts<Vec<u8>> =
        match parse_str_to_git_object_parts_bytes(&decompressed_bytes) {
            Ok(parts) => parts,
            Err(err) => {
                println!("parse_str_to_git_object_parts: {err:?}");
                return;
            }
        };

    let git_object: GitObject = match GitObject::from_parts_bytes(git_object_parts) {
        Ok(git_object) => git_object,
        Err(err) => {
            println!("GitObject::from_parts: {err:?}");
            return;
        }
    };

    if let Some(option) = option {
        if option.eq("--name-only") {
            let tree_entry: &Vec<TreeEntry> = git_object.get_tree_content();
            tree_entry.iter().for_each(|te| println!("{}", te.name));
        }
        else {
            println!("Unknow option {option}.");
        }
    }
}

fn git_write_tree() -> String {
    todo!()
}

use std::path::Path;
use std::fs::ReadDir;

fn create_tree_object(dir: &Path) -> Result<String, GitError> {
    if !dir.is_dir() {
        return Err(GitError::CreateTree("Tree dir is not a directory.".to_string()));
    }

    let entries: ReadDir = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(err) => {
            return Err(GitError::CreateTree(format!("fs::read_dir: {err}.")));
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                return Err(GitError::CreateTree(format!("entry: {err}.")));
            }
        };

        let path = entry.path();
        if path.is_dir() {
            let tree_sha: String = create_tree_object(&path)?;
        }
        else {
            let Some(filepath) = path.to_str() else {
                return Err(GitError::CreateTree("path.to_str.".to_string()));
            };
            let blob_sha: String = create_blob_object(filepath)?;
        }
    }

    todo!()
}

fn create_blob_object(file_path: &str) -> Result<String, GitError> {
	let mut file: File = match File::open(file_path) {
        Ok(file) => file,
        Err(err) => {
            return Err(GitError::CreateBlob(format!("File::open: {err}")));
        }
    };

    let mut content: String = String::new();
    let _read_bytes: usize = match file.read_to_string(&mut content) {
        Ok(read_bytes) => read_bytes,
        Err(err) => {
            return Err(GitError::CreateBlob(format!("File::read_to_string: {err}")));
        }
    };

    let git_object = GitObject::create_blob_with_content(content);
    let str_git_object: String = git_object.as_string();
    let sha1_hash: String = compute_sha1_hash(&str_git_object);
    let bytes: Vec<u8> = match zlib_compression(&str_git_object) {
        Ok(bytes) => bytes,
        Err(err) => {
            return Err(GitError::CreateBlob(format!("zlib_compression: {err}")));
        }
    };

	let (folder_path, file_name): (String, String) = sha1_to_file_path(&sha1_hash);
	match write_bytes_to_file(&folder_path, &file_name, &bytes[..]) {
		Ok(()) => {}
		Err(err) => {
			return Err(GitError::CreateBlob(format!("write_bytes_to_file: {err}")));
		}
	}

    Ok(sha1_hash)
}

const GIT_OBJECT_FOLDER_PATH: &str = ".git/objects";

fn sha1_to_file_path(hash: &str) -> (String, String) {
    let folder_path = format!("{GIT_OBJECT_FOLDER_PATH}/{}", &hash[..2]);
    let file_name = (hash[2..]).to_string();
    (folder_path, file_name)
}

fn get_file_bytes(mut file: File) -> std::io::Result<Vec<u8>> {
    let mut buffer: Vec<u8> = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(buffer)
}

fn zlib_decompression(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut zlib_decoder = ZlibDecoder::new(bytes);
    let mut content: Vec<u8> = Vec::new();
    zlib_decoder.read_to_end(&mut content)?;
    Ok(content)
}

fn zlib_compression(content: &str) -> std::io::Result<Vec<u8>> {
    let mut zlib_encode = ZlibEncoder::new(Vec::new(), Compression::default());
    zlib_encode.write_all(content.as_bytes())?;
    zlib_encode.finish()
}

fn parse_str_to_git_object_parts_string(s: &str) -> Result<GitObjectParts<String>, GitError> {
    let Some((first, content)): Option<(&str, &str)> = s.split_once("\0") else {
        return Err(GitError::InvalidGitObject);
    };

    let Some((git_type, size)): Option<(&str, &str)> = first.split_once(" ") else {
        return Err(GitError::InvalidGitObject);
    };

    let git_type: String = git_type.to_string();
    let Ok(size): Result<usize, _> = size.parse::<usize>() else {
        return Err(GitError::InvalidGitObject);
    };
    let content: String = content.to_string();

    Ok(GitObjectParts {
        git_type,
        size,
        content,
    })
}

fn parse_str_to_git_object_parts_bytes(s: &[u8]) -> Result<GitObjectParts<Vec<u8>>, GitError> {
    let mut git_type = String::new();

    let mut index: usize = 0;
    loop {
        if s[index].eq(&b' ') {
            break;
        }

        git_type.push(s[index] as char);
        index += 1;
    }
    index += 1;

    let mut size_bytes: Vec<u8> = Vec::new();

    loop {
        if s[index].eq(&b'\0') {
            break;
        }

        size_bytes.push(s[index]);
        index += 1;
    }
    index += 1;

    let size: String = String::from_utf8(size_bytes).unwrap();

    let Ok(size): Result<usize, _> = size.parse::<usize>() else {
        return Err(GitError::InvalidGitObject);
    };

    let content: Vec<u8> = s[index..s.len()].to_vec();

    Ok(GitObjectParts {
        git_type,
        size,
        content,
    })
}

fn parse_str_tree_entry_vec(content: &[u8]) -> Result<Vec<TreeEntry>, GitError> {
    let pos: Vec<usize> = tree_entry_end_pos(content);
    let tree_entry_bytes: Vec<&[u8]> = extract_from_vec_at(content, &pos[..]);

    let mut tree_entry: Vec<TreeEntry> = Vec::new();

    for teb in tree_entry_bytes {
        match TreeEntry::from_bytes(teb) {
            Ok(res) => tree_entry.push(res),
            Err(err) => return Err(err),
        }
    }

    Ok(tree_entry)
}

fn tree_entry_end_pos(v: &[u8]) -> Vec<usize> {
    v.iter()
        .enumerate()
        .filter(|(_, &byte)| byte == b'\0')
        .map(|(i, _)| i + 21)
        .collect::<Vec<usize>>()
}

fn extract_from_vec_at<'a>(vec: &'a [u8], pos: &[usize]) -> Vec<&'a [u8]> {
    let mut extract: Vec<&[u8]> = Vec::new();

    let mut prev_pos: usize = 0;
    for p in pos {
        let tmp: &[u8] = &vec[prev_pos..*p];
        extract.push(tmp);
        prev_pos = *p;
    }

    extract
}

fn compute_sha1_hash(content: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.input_str(content);
    hasher.result_str()
}

fn write_bytes_to_file(folder_path: &str, file_name: &str, content: &[u8]) -> std::io::Result<()> {
    fs::create_dir_all(folder_path)?;
    let mut file = File::create_new(format!("{folder_path}/{file_name}"))?;
    file.write_all(content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_type_fmt() {
        let expected: String = String::from("blob");
        let blob: GitObject = GitObject::Blob {
            content: String::from("Content."),
        };

        assert_eq!(expected, blob.get_type());
    }
}
