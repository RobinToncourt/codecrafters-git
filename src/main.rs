#![allow(dead_code)]
#[allow(unused_imports)]
use std::env;
use std::fmt;

use std::fs;
use std::fs::File;
use std::fs::OpenOptions;

use std::io::prelude::*;
use flate2::read::ZlibDecoder;
use flate2::Compression;
use flate2::write::ZlibEncoder;

use crypto::digest::Digest;
use crypto::sha1::Sha1;

const GIT_COMMAND_INIT: &str = "init";
const GIT_COMMAND_CAT_FILE: &str = "cat-file";
const GIT_COMMAND_HASH_OBJECT: &str = "hash-object";
const GIT_COMMAND_LS_TREE: &str = "ls-tree";

const GIT_OBJECTS_FOLDER_PATH: &str = ".git/objects";

const GIT_OBJECT_TYPE_BLOB: &str = "blob";

enum ObjectType {
    Blob,
    Tree,
    Commit,
}

impl ObjectType {
    fn from_str(s: &str) -> Self {
        match s {
            "blob" => ObjectType::Blob,
            "tree" => ObjectType::Tree,
            "commit" => ObjectType::Commit,
            _ => panic!("Invalid object type: {s}"),
        }
    }
}

impl fmt::Display for ObjectType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Blob => write!(f, "blob"),
            Self::Tree => write!(f, "tree"),
            Self::Commit => write!(f, "commit"),
        }
    }
}

struct Object {
    typ: ObjectType,
    size: usize,
    content: String,
}

impl Object {
    fn from_file(_file_path: &str) -> Self {
        // let file = File::open(&file_path)?;
        //
        // let mut zlib_decoder = ZlibDecoder::new(file);
        // let mut decompress_file_content = String::new();
        // zlib_decoder.read_to_string(&mut decompress_file_content)?;
        //
        // let (object, size, content) = git_parse_file(&decompress_file_content)?;

        todo!()
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Need at least 1 argument.");
        return;
    }

    match args[1].as_str() {
        GIT_COMMAND_INIT => git_init(),
        GIT_COMMAND_CAT_FILE => git_cat_file(&args),
        GIT_COMMAND_HASH_OBJECT => git_hash_object(&args),
        GIT_COMMAND_LS_TREE => git_ls_tree(&args),
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

fn git_cat_file(args: &Vec<String>) {
    if args.len() < 4 {
        println!("git cat-file needs 2 arguments.");
        return;
    }

    let option: &str = &args[2];
    match option {
        "-p" => {},
        _ => {
            println!("Invalid option.");
            return;
        },
    }

    let hash: &str = &args[3];
    let file_path: String = hash_to_path(hash);

    // Open file.
    let Ok(file) = File::open(&file_path) else {
        println!("File does not exist: {file_path}");
        return;
    };

    // Uncompress file with flate 2(?).
    let mut zlib_decoder = ZlibDecoder::new(file);
    let mut decompress_file_content = String::new();
    let Ok(_) = zlib_decoder.read_to_string(&mut decompress_file_content) else {
        println!("Invalid UTF-8.");
        return;
    };

    // Read header, size and content.
    let (_object_type, size, content) =
        match git_parse_file(&decompress_file_content) {
            Ok((header, size, content)) => (header, size, content),
            Err(err_message) => {
                println!("{err_message}");
                return;
            }
        };

    if size != content.len() {
        println!(
            "Sizes do not match, size={size}, content size={}", content.len()
        );
    }

    match option {
        "-p" => print!("{content}"),
        _ => panic!("Impossible, option checked before."),
    }
}

fn git_parse_file(
    decompress_file_content: &str
) -> Result<(&str, usize, &str), String> {
    let split: Vec<&str> = decompress_file_content.split('\0').collect();

    if split.len() != 2 {
        return Err(format!("Invalid object type."));
    }

    let Some((object_type, size)) = split[0].split_once(' ') else {
        return Err(format!("Invalid header: {}", split[0]));
    };
    let Ok(size) = size.parse::<usize>() else {
        return Err(format!("Invalid size: {size}"));
    };
    let content = split[1];

    Ok((object_type, size, content))
}

fn hash_to_path(hash: &str) -> String {
    format!("{GIT_OBJECTS_FOLDER_PATH}/{}/{}", &hash[..2], &hash[2..])
}

fn git_hash_object(args: &Vec<String>) {
    if args.len() < 3 {
        println!("git hash-objects needs 1 argument.");
        return;
    }

    let option: &str = &args[2];
    match option {
        "-w" => {},
        _ => {
            println!("Invalid option.");
            return;
        },
    }

    let file_path = &args[3];
    let Ok(content) = fs::read_to_string(file_path) else {
        println!("File does not exist: {file_path}");
        return;
    };

    let size = content.len();
    let object = format!("{} {size}\0{content}", ObjectType::Blob);

    let mut hasher = Sha1::new();
    hasher.input_str(&object);
    let sha_hash = hasher.result_str();

    println!("{sha_hash}");

    let mut zlib_encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    match zlib_encoder.write_all(object.into_bytes().as_slice()) {
        Ok(()) => {},
        Err(error) => {
            println!("An error occured: {error}");
            return;
        },
    }

    let compress_object = match zlib_encoder.finish() {
        Ok(compress) => compress,
        Err(error) => {
            println!("An error occured: {error}");
            return;
        },
    };

    match option {
        "-w" => write_object(&sha_hash, compress_object),
        _ => panic!("Impossible, option checked before."),
    }
}

fn write_object(sha_hash: &str, compress_object: Vec<u8>) {
    let folder: &str = &sha_hash[..2];
    let file_name: &str = &sha_hash[2..];

    match fs::create_dir_all(&format!("{GIT_OBJECTS_FOLDER_PATH}/{folder}")) {
        Ok(()) => {},
        Err(error) => {
            println!("An error occured: {error}");
            return;
        },
    }

    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&format!("{GIT_OBJECTS_FOLDER_PATH}/{folder}/{file_name}"));

    let mut file = match file {
        Ok(file) => file,
        Err(error) => {
            println!("An error occured: {error}");
            return;
        },
    };

    match file.write_all(&compress_object) {
        Ok(()) => {},
        Err(error) => {
            println!("An error occured: {error}");
            return;
        },
    }
}

fn git_ls_tree(args: &Vec<String>) {
    if args.len() < 3 {
        println!("git ls-tree needs 1 argument.");
        return;
    }

    let (option, tree_sha): (Option<&str>, &str) = if args.len() == 3 {
        (None, &args[2])
    }
    else {
        (Some(&args[2]), &args[3])
    };

    let file_path = hash_to_path(tree_sha);

    let Ok(file) = File::open(&file_path) else {
        println!("File does not exist: {file_path}");
        return;
    };

    let mut zlib_decoder = ZlibDecoder::new(file);
    let mut decompress_file_content = String::new();
    let Ok(_) = zlib_decoder.read_to_string(&mut decompress_file_content) else {
        println!("Invalid UTF-8.");
        return;
    };

    let (object_type, size, content): (&str, usize, &str) =
        match git_parse_file(&decompress_file_content) {
            Ok((header, size, content)) => (header, size, content),
            Err(err_message) => {
                println!("{err_message}");
                return;
            }
        };

    if !object_type.eq("tree") {
        println!("Not a tree object.");
        return;
    }

    if size != content.len() {
        println!(
            "Sizes do not match, size={size}, content size={}", content.len()
        );
    }

    let tree_object_entry_list = tfcttoel(content);

    let only_name: bool = match option {
        Some(flag) => flag.eq("--name-only"),
        None => false,
    };

    for entry in tree_object_entry_list {
        if only_name {
            println!("{}", entry.name);
        }
        else {
            println!(
                "{} {} {}  {}",
                entry.mode, entry.typ, entry.sha, entry.name
            );
        }
    }
}

struct TreeObjectEntry {
    mode: String,
    typ: String,
    sha: String,
    name: String,
}

enum TreeObjectMode {
    RegularFile     = 100664,
    ExecutableFile  = 100755,
    SymbolicFile    = 120000,

    Directory       = 040000,
}

use tree_file_content_to_tree_object_entry_list as tfcttoel;
fn tree_file_content_to_tree_object_entry_list(
    content: &str,
) -> Vec<TreeObjectEntry> {
    let entry_pos_list: Vec<usize> =
        content.match_indices("\0").map(|(pos, _)| pos+20).collect();

    let mut entry_str_list: Vec<&str> = Vec::new();
    let mut content_remain = content;
    for index in entry_pos_list {
        let (first, last) = content_remain.split_at(index);
        entry_str_list.push(first);
        content_remain = last;
    }

    entry_str_list.into_iter().map(str_entry_to_tree_object_entry).collect()
}

fn str_entry_to_tree_object_entry(str_entry: &str) -> TreeObjectEntry {
    // <mode> <name>\0<20_byte_sha>
    let split: Vec<&str> = str_entry.split('\0').collect();

    let (mode, name) = split[0].split_once(' ').unwrap();
    let sha = split[1];

    let (typ, _, _) = git_parse_file(sha).unwrap();

    TreeObjectEntry {
        mode: String::from(mode),
        typ: String::from(typ),
        sha: String::from(sha),
        name: String::from(name),
    }
}





