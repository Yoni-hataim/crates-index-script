use chrono::{Utc, DateTime};
use crates_index::SparseIndex;
use http;
use serde::Deserialize;
use serde_json::Value;
use std::{
    fs,
    io::{self, Write},
    path::Path,
    process::Command, time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Deserialize, Clone)]
struct Package {
    name: String,
    version: String,
}
fn main() {
    let utc: DateTime<Utc> = Utc::now();

    // Format the time as a string
    let formatted_time = utc.format("%Y-%m-%d %H:%M:%S").to_string();

    // Print the formatted time
    println!("Current time: {}", formatted_time);

    //// to run this you need to put the absolute path of the project you want to copy in the project path variable and change the project name to your project
    let project_path = Path::new("Cargo.toml");
    let project_name = "index-test";
    let index_dir: String = "crates-index".to_owned();
    let crates_dir: String = "crates".to_owned();
    self::remove_dir_if_exists(&index_dir);
    self::remove_dir_if_exists(&crates_dir);
    let mut index = SparseIndex::new_cargo_default().unwrap();

    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--format-version=1")
        .arg("--manifest-path")
        .arg(project_path.to_str().unwrap())
        .output()
        .expect("Failed to run cargo metadata.");

    let metadata: Value =
        serde_json::from_slice(&output.stdout).expect("Failed to parse metadata.");

    let dependencies: Vec<Package> = metadata["packages"]
        .as_array()
        .expect("Failed to get packages")
        .iter()
        .filter(|package| package["name"].as_str() != Some(&project_name))
        .map(|package| {
            serde_json::from_value(package.clone()).expect("Failed to deserialize package")
        })
        .collect();

    dependencies.iter().for_each(|_crate: &Package| {
        update(
            &mut index,
            _crate.clone(),
            index_dir.clone(),
            crates_dir.clone(),
        );
    });
    let utc: DateTime<Utc> = Utc::now();

    // Format the time as a string
    let formatted_time = utc.format("%Y-%m-%d %H:%M:%S").to_string();

    // Print the formatted time
    println!("Current time: {}", formatted_time);
}

fn remove_dir_if_exists(dir: &str) {
    if let Err(err) = fs::remove_dir_all(dir) {
        if err.kind() == std::io::ErrorKind::NotFound {
            println!("Directory not found: {}", dir);
        } else {
            eprintln!("Failed to delete directory: {}", err);
        }
    } else {
        println!("Directory deleted successfully: {}", dir);
    }
}

fn update(index: &mut SparseIndex, _crate: Package, index_dir: String, crates_dir: String) {
    let req = index
        .make_cache_request(&_crate.name)
        .unwrap()
        .body(())
        .unwrap();
    let index_path = req.uri().path_and_query().expect("failed parsing ");

    let formatted_index_path = format!("{dir}{path}", dir=index_dir, path=index_path.path());
    let formatted_crates_path = format!("{dir}{path}/{version}/{name}-{version}.crate", dir=crates_dir, path=index_path.path(),version=_crate.version,name=_crate.name);
    let (parts, _) = req.into_parts();
    let req = http::Request::from_parts(parts, vec![]);

    let req: reqwest::blocking::Request = req.try_into().unwrap();

    let client = reqwest::blocking::ClientBuilder::new()
        .gzip(true)
        .build()
        .unwrap();

    let res = client.execute(req).unwrap();

    let mut builder = http::Response::builder()
        .status(res.status())
        .version(res.version());

    builder
        .headers_mut()
        .unwrap()
        .extend(res.headers().iter().map(|(k, v)| (k.clone(), v.clone())));

    let body = res.bytes().unwrap();
    let res = builder.body(body.to_vec()).unwrap();

    let crate_option = index.parse_cache_response(&_crate.name, res, true).unwrap();
    match crate_option {
        Some(krate) => {
            if let Some(parent) = std::path::Path::new(&formatted_index_path).parent() {
                fs::create_dir_all(parent).expect("Failed to create directories");
            }
            if let Some(parent) = std::path::Path::new(&formatted_crates_path).parent() {
                fs::create_dir_all(parent).expect("Failed to create directories");
            }
            let download_path = format!(
                "https://crates.io/api/v1/crates/{crate_name}/{crate_version}/download",
                crate_name = &_crate.name,
                crate_version = &_crate.version
            );
            let mut response = reqwest::blocking::get(download_path)
                .expect(&format!("failed downloading crate {}", &_crate.name));
            let mut crate_file = fs::File::create(&formatted_crates_path).unwrap();
            io::copy(&mut response, &mut crate_file).unwrap();
            let mut index_file = fs::File::create(&formatted_index_path).unwrap();
            for item in krate.versions().to_vec() {
                let json_string = serde_json::to_string(&item).unwrap();
                index_file
                    .write_all(json_string.as_bytes())
                    .expect("Unable to write to file");
                index_file
                    .write_all(b"\n")
                    .expect("Unable to write newline to file");
            }
        }
        None => println!("no such crate {crate_name}", crate_name = &_crate.name),
    }
}
