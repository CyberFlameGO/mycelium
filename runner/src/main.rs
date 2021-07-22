use std::env;
use toml_edit::{Document, Table, Array, value};
use std::fs::{read_to_string, create_dir_all};
use std::fs::File;
use std::io::{Write, Error};
use std::path::{Path};
use std::process::{Command, Stdio};
use yaml_rust::{YamlLoader, YamlEmitter, Yaml};
use linked_hash_map::LinkedHashMap;


fn main() -> Result<(), Error> {
    let config_path = env::var("MYCELIUM_CONFIG_PATH").unwrap_or(String::from("/config"));
    let data_path = env::var("MYCELIUM_DATA_PATH").unwrap_or(String::from("/data"));
    let fw_token = env::var("MYCELIUM_FW_TOKEN").unwrap();
    let server_kind = env::var("MYCELIUM_RUNNER_KIND").unwrap();

    // create paths from env vars
    let config_path: &Path = Path::new(&config_path);
    let data_path: &Path = Path::new(&data_path);

    assert!(config_path.is_dir());
    assert!(data_path.is_dir());

    // copy all the files from config_path to data_path
    // TODO: rewrite properly without Command
    Command::new("sh")
        .args(&["-c", &format!("cp {}/* {}", config_path.to_str().unwrap(), data_path.to_str().unwrap())])
        .output()
        .expect("failed to copy configuration");

    // configure the server
    match server_kind.as_str() {
        "game" => configure_game(fw_token, data_path),
        "proxy" => configure_proxy(fw_token, data_path),
        _ => panic!("env::var(MYCELIUM_RUNNER_KIND) must be 'game' or 'proxy'")
    }?;

    // download plugins
    download_plugins(data_path);

    // start server
    match server_kind.as_str() {
        "game" => download_run_game(data_path),
        "proxy" => download_run_proxy(data_path),
        _ => panic!("if you're seeing this, all hope is lost, the end times are here")
    }?;

    Ok(())
}

fn download_file(url: &str, path: &str) {
    // TODO: rewrite properly without Command
    Command::new("curl")
        .args(&[url, "--output", path])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("exec download")
        .wait()
        .expect("wait for download");
}

fn run_jar(cwd: &str, file: &str) {
    // TODO: rewrite properly without Command
    Command::new("java")
        .args(&["-jar", file])
        .current_dir(cwd)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("run jar")
        .wait()
        .expect("wait for jar");
}

fn download_plugins(data_path: &Path) -> Result<(), Error> {
    let plugins_str = env::var("MYCELIUM_PLUGINS").unwrap();
    let plugins = plugins_str.split(",");
    let plugin_dir_path = data_path.join("plugins/");
    let plugin_dir = plugin_dir_path.to_str().unwrap();
    create_dir_all(plugin_dir)?;
    for p in plugins {
        println!("downloading {}", p);
        let file = p.split("/").last().unwrap();
        let plugin_path_str = plugin_dir_path.join(file);
        download_file(p, plugin_path_str.to_str().unwrap());
    }
    Ok(())
}

fn download_run_game(data_path: &Path) -> Result<(), Error> {
    // TODO: fetch the URL from the api
    // TODO: download this from the jar cache
    let data_path_str = data_path.to_str().unwrap();
    let paper_jar_path = data_path.join("paper.jar");
    let paper_jar_str = paper_jar_path.to_str().unwrap();
    let url = "https://papermc.io/api/v2/projects/paper/versions/1.17.1/builds/110/downloads/paper-1.17.1-110.jar";
    download_file(url, paper_jar_str);
    run_jar(data_path_str, "paper.jar");

    Ok(())
}

fn download_run_proxy(data_path: &Path) -> Result<(), Error> {
    // TODO: fetch the URL from the api
    // TODO: download this from the jar cache
    let data_path_str = data_path.to_str().unwrap();
    let velocity_jar_path = data_path.join("velocity.jar");
    let velocity_jar_str = velocity_jar_path.to_str().unwrap();
    let url = "https://versions.velocitypowered.com/download/3.0.0.jar";
    download_file(url, velocity_jar_str);
    run_jar(data_path_str, "velocity.jar");

    Ok(())
}

// the yaml parsing and modification in this function is horrifying
// maybe I should've just written go
fn configure_game(token: String, data_path: &Path) -> Result<(), Error> {
    let paper_yaml_path = data_path.join("paper.yml");
    let paper_yaml: String = match read_to_string(paper_yaml_path.clone()) {
        Ok(file) => file,
        Err(_error) => r#"config-version: 20
settings:
    velocity-support:
        enabled: true"#.to_string(),
    };
    let loaded = YamlLoader::load_from_str(&paper_yaml).expect("YAML parse");
    let mut yaml_doc = loaded[0].as_hash().unwrap().clone();
    
    // modify the config
    let mut settings = yaml_doc[&Yaml::from_str("settings")].as_hash().unwrap().clone();
    let mut velocity_map = LinkedHashMap::new();
    velocity_map.insert(Yaml::from_str("enabled"), Yaml::Boolean(true));
    velocity_map.insert(Yaml::from_str("online-mode"), Yaml::Boolean(true));
    velocity_map.insert(Yaml::from_str("secret"), Yaml::from_str(&token));
    settings[&Yaml::from_str("velocity-support")] = Yaml::Hash(velocity_map);
    yaml_doc[&Yaml::from_str("settings")] = Yaml::Hash(settings);
    let yamled = Yaml::Hash(yaml_doc);

    // accept the EULA
    let eula_txt_path = data_path.join("eula.txt");
    let mut f = File::create(eula_txt_path)?;
    f.write_all("eula=true".as_bytes())?;

    // write the modified config
    let mut f = File::create(paper_yaml_path)?;
    let mut out_str = String::new();
    let mut emitter = YamlEmitter::new(&mut out_str);
    emitter.dump(&yamled).unwrap();
    f.write_all(out_str.as_bytes())?;
    Ok(())
}

fn configure_proxy(token: String, data_path: &Path) -> Result<(), Error> {
    // read and parse velocity.toml
    let velocity_toml_path = data_path.join("velocity.toml");
    let velocity_toml: String = match read_to_string(velocity_toml_path.clone()) {
        Ok(file) => file,
        Err(_error) => r#"config-version = "1.0"
bind = "0.0.0.0:25577"
player-info-forwarding-mode = "modern"
forwarding-secret = "secret_here"

[servers]
try = []

[forced-hosts]"#.to_string(),
    };
    let mut toml_doc = velocity_toml.parse::<Document>().expect("TOML parse");

    // modify the config
    toml_doc["forwarding-secret"] = value(token);
    let mut servers = Table::default();
    servers["try"] = value(Array::default());
    toml_doc["servers"] = toml_edit::Item::Table(servers);

    // write the modified config
    let mut f = File::create(velocity_toml_path)?;
    f.write_all(toml_doc.to_string().as_bytes())?;
    Ok(())
}