#[allow(unused_imports)]
use std::env;
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

const GIT_OBJECTS_FOLDER_PATH: &str = ".git/objects";

const GIT_OBJECT_TYPE_BLOB: &str = "blob";

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
    let (_object, size, content) =
        match git_cat_file_split_file(&decompress_file_content) {
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

fn git_cat_file_split_file(
    decompress_file_content: &str
) -> Result<(&str, usize, &str), String> {
    let split: Vec<&str> = decompress_file_content.split('\0').collect();

    if split.len() != 2 {
        return Err(format!("Invalid object type."));
    }

    let Some((object, size)) = split[0].split_once(' ') else {
        return Err(format!("Invalid header: {}", split[0]));
    };
    let Ok(size) = size.parse::<usize>() else {
        return Err(format!("Invalid size: {size}"));
    };
    let content = split[1];

    Ok((object, size, content))
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
    let object = format!("{GIT_OBJECT_TYPE_BLOB} {size}\0{content}");

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
