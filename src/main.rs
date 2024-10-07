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
}

struct GitObjectParts {
    git_type: String,
    size: usize,
    content: String,
}

#[derive(Debug)]
enum GitObject {
    Blob { content: String },
    Tree { content: Vec<TreeEntry> },
    Commit,
}

impl GitObject {
    fn from_parts(parts: GitObjectParts) -> Result<Self, GitError> {
        if parts.size != parts.content.len() {
            return Err(GitError::InvalidGitObject);
        }

        match parts.git_type.as_str() {
            "blob" => Ok(GitObject::Blob {
                content: parts.content,
            }),
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

    fn get_content(&self) -> &str {
        match self {
            GitObject::Blob { content } => content,
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

#[derive(Debug)]
enum EntryMode {
    RegularFile = 100644,
    ExecutableFile = 100755,
    SymbolicLink = 120000,
    Directory = 40000,
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

    let decompressed_file_content: String = match zlib_decompression(&bytes[..]) {
        Ok(s) => s,
        Err(err) => {
            println!("zlib_decompression: {err}");
            return;
        }
    };

    let git_object_parts: GitObjectParts =
        match parse_str_to_git_object_parts(&decompressed_file_content) {
            Ok(parts) => parts,
            Err(err) => {
                println!("parse_str_to_git_object_parts: {err:?}");
                return;
            }
        };

    let git_object: GitObject = match GitObject::from_parts(git_object_parts) {
        Ok(git_object) => git_object,
        Err(err) => {
            println!("GitObject::from_parts: {err:?}");
            return;
        }
    };

    if let Some(option) = option {
        if option.eq("-p") {
            print!("{}", git_object.get_content());
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

    let decompressed_file_content: String = match zlib_decompression(&bytes[..]) {
        Ok(s) => s,
        Err(err) => {
            println!("zlib_decompression: {err}");
            return;
        }
    };

    let git_object_parts: GitObjectParts =
    match parse_str_to_git_object_parts(&decompressed_file_content) {
        Ok(parts) => parts,
        Err(err) => {
            println!("parse_str_to_git_object_parts: {err:?}");
            return;
        }
    };

    let git_object: GitObject = match GitObject::from_parts(git_object_parts) {
        Ok(git_object) => git_object,
        Err(err) => {
            println!("GitObject::from_parts: {err:?}");
            return;
        }
    };

    todo!()
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

fn zlib_decompression(bytes: &[u8]) -> std::io::Result<String> {
    let mut zlib_decoder = ZlibDecoder::new(bytes);
    let mut content: String = String::new();
    zlib_decoder.read_to_string(&mut content)?;
    Ok(content)
}

fn zlib_compression(content: &str) -> std::io::Result<Vec<u8>> {
    let mut zlib_encode = ZlibEncoder::new(Vec::new(), Compression::default());
    zlib_encode.write_all(content.as_bytes())?;
    zlib_encode.finish()
}

fn parse_str_to_git_object_parts(s: &str) -> Result<GitObjectParts, GitError> {
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

fn parse_str_tree_entry_vec(content: &str) -> Result<Vec<TreeEntry>, GitError> {
    todo!()
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
