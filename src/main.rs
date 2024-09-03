#[allow(unused_imports)]
use std::env;
#[allow(unused_imports)]
use std::fs;

use std::io::prelude::*;
use flate2::read::GzDecoder;

const GIT_COMMAND_INIT: &str = "init";
const GIT_COMMAND_CAT_FILE: &str = "cat-file";

const GIT_OBJECTS_FOLDER_PATH: &str = "./.git/objects";

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Need at least 1 argument.");
        return;
    }

    match args[1].as_str() {
        GIT_COMMAND_INIT => git_init(),
        GIT_COMMAND_CAT_FILE => git_cat_file(&args),
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

    let _option: &str = args[2].as_str();
    let hash: &str = args[3].as_str();
    let file_path: String = hash_to_path(hash);

    // Open file.
    let compress_file_content: String = fs::read_to_string(file_path).unwrap();
    // TODO: check result.

    // Uncompress file with flate 2(?).
    let mut d = GzDecoder::new(compress_file_content.as_bytes());
    let mut decompress_file_content = String::new();
    d.read_to_string(&mut decompress_file_content).unwrap();
    // TODO: check result.

    // Read header and content.
    let split: Vec<&str> = decompress_file_content.split('\0').collect();
    // TODO: check split.
    let _object_and_size: &str = split[0];
    let content: &str = split[1];

    // TODO: Oppose content size and size.
    // TODO: Do action based on option.

    print!("{content}");
}

fn hash_to_path(hash: &str) -> String {
    let mut path: String = String::from(GIT_OBJECTS_FOLDER_PATH);

    path.push_str(&hash[..2]);
    path.push('/');
    path.push_str(&hash[2..]);

    path
}
