#[allow(unused_imports)]
use std::env;
#[allow(unused_imports)]
use std::fs;
use std::fs::File;

use std::io::prelude::*;
use flate2::read::ZlibDecoder;

const GIT_COMMAND_INIT: &str = "init";
const GIT_COMMAND_CAT_FILE: &str = "cat-file";

const GIT_OBJECTS_FOLDER_PATH: &str = ".git/objects/";

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

    let option: &str = args[2].as_str();
    match option {
        "-p" => {},
        _ => {
            println!("Invalid option.");
            return;
        }
    }

    let hash: &str = args[3].as_str();
    let file_path: String = hash_to_path(hash);

    // Open file.
    let Ok(file) = File::open(&file_path) else {
        println!("File does not exist: {file_path}");
        return;
    };

    // Uncompress file with flate 2(?).
    let mut d = ZlibDecoder::new(file);
    let mut decompress_file_content = String::new();
    let Ok(_) = d.read_to_string(&mut decompress_file_content) else {
        println!("Invalid UTF-8.");
        return;
    };

    dbg!(&decompress_file_content);

    // Read header and content.
    let split: Vec<&str> = decompress_file_content
        .split(|c: char| c.eq(&' ') || c.eq(&'\0'))
        .filter(|p| !p.is_empty())
        .collect();

    dbg!(&split);

    if split.len() != 3 {
        println!("Invalid git blob.");
        return;
    }

    let _header: &str = split[0];
    let Ok(size) = split[1].parse::<usize>() else {
        println!("Invalid size.");
        return;
    };
    let content: &str = split[2];

    println!("{_header}, {size}, {content}");

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

fn hash_to_path(hash: &str) -> String {
    let mut path: String = String::from(GIT_OBJECTS_FOLDER_PATH);

    path.push_str(&hash[..2]);
    path.push('/');
    path.push_str(&hash[2..]);

    path
}
