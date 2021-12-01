use anyhow::{bail, Context, Result};
use bytesize::ByteSize;
use configparser::ini::Ini;
use fs_err as fs;
use maturin::{upload, Registry, UploadError};
use reqwest::Url;
use std::env;
use std::io;
use std::path::PathBuf;
use structopt::StructOpt;

/// An account with a registry, possibly incomplete
#[derive(Debug, StructOpt)]
pub struct PublishOpt {
    #[structopt(
        short = "r",
        long = "repository-url",
        default_value = "https://upload.pypi.org/legacy/"
    )]
    /// The url of registry where the wheels are uploaded to
    pub registry: String,
    #[structopt(short, long)]
    /// Username for pypi or your custom registry
    pub username: Option<String>,
    #[structopt(short, long)]
    /// Password for pypi or your custom registry. Note that you can also pass the password
    /// through MATURIN_PASSWORD
    pub password: Option<String>,
    /// Continue uploading files if one already exists.
    /// (Only valid when uploading to PyPI. Other implementations may not support this.)
    #[structopt(long = "skip-existing")]
    pub skip_existing: bool,
}

/// Returns the password and a bool that states whether to ask for re-entering the password
/// after a failed authentication
///
/// Precedence:
/// 1. MATURIN_PASSWORD
/// 2. keyring
/// 3. stdin
fn get_password(_username: &str) -> String {
    if let Ok(password) = env::var("MATURIN_PASSWORD") {
        return password;
    };

    #[cfg(feature = "keyring")]
    {
        let service = env!("CARGO_PKG_NAME");
        let keyring = keyring::Entry::new(service, _username);
        if let Ok(password) = keyring.get_password() {
            return password;
        };
    }

    rpassword::prompt_password_stdout("Please enter your password: ").unwrap_or_else(|_| {
        // So we need this fallback for pycharm on windows
        let mut password = String::new();
        io::stdin()
            .read_line(&mut password)
            .expect("Failed to read line");
        password.trim().to_string()
    })
}

fn get_username() -> String {
    println!("Please enter your username:");
    let mut line = String::new();
    io::stdin().read_line(&mut line).unwrap();
    line.trim().to_string()
}

fn load_pypirc() -> Ini {
    let mut config = Ini::new();
    if let Some(mut config_path) = dirs::home_dir() {
        config_path.push(".pypirc");
        if let Ok(pypirc) = fs::read_to_string(config_path.as_path()) {
            let _ = config.read(pypirc);
        }
    }
    config
}

fn load_pypi_cred_from_config(config: &Ini, registry_name: &str) -> Option<(String, String)> {
    if let (Some(username), Some(password)) = (
        config.get(registry_name, "username"),
        config.get(registry_name, "password"),
    ) {
        return Some((username, password));
    }
    None
}

fn resolve_pypi_cred(
    opt: &PublishOpt,
    config: &Ini,
    registry_name: Option<&str>,
) -> (String, String) {
    // API token from environment variable takes priority
    if let Ok(token) = env::var("MATURIN_PYPI_TOKEN") {
        return ("__token__".to_string(), token);
    }

    if let Some((username, password)) =
        registry_name.and_then(|name| load_pypi_cred_from_config(config, name))
    {
        println!("🔐 Using credential in pypirc for upload");
        return (username, password);
    }

    // fallback to username and password
    let username = opt.username.clone().unwrap_or_else(get_username);
    let password = match opt.password {
        Some(ref password) => password.clone(),
        None => get_password(&username),
    };

    (username, password)
}

/// Asks for username and password for a registry account where missing.
fn complete_registry(opt: &PublishOpt) -> Result<Registry> {
    // load creds from pypirc if found
    let pypirc = load_pypirc();
    let (register_name, registry_url) =
        if !opt.registry.starts_with("http://") && !opt.registry.starts_with("https://") {
            if let Some(url) = pypirc.get(&opt.registry, "repository") {
                (Some(opt.registry.as_str()), url)
            } else {
                bail!(
                    "Failed to get registry {} in .pypirc. \
                    Note: Your index didn't start with http:// or https://, \
                    which is required for non-pypirc indices.",
                    opt.registry
                );
            }
        } else if opt.registry == "https://upload.pypi.org/legacy/" {
            (Some("pypi"), opt.registry.clone())
        } else {
            (None, opt.registry.clone())
        };
    let (username, password) = resolve_pypi_cred(opt, &pypirc, register_name);
    let registry = Registry::new(username, password, Url::parse(&registry_url)?);

    Ok(registry)
}

/// Handles authentication/keyring integration and retrying of the publish subcommand
pub fn upload_ui(items: &[PathBuf], publish: &PublishOpt) -> Result<()> {
    let registry = complete_registry(publish)?;

    println!("🚀 Uploading {} packages", items.len());

    for i in items {
        let upload_result = upload(&registry, i);

        match upload_result {
            Ok(()) => (),
            Err(UploadError::AuthenticationError) => {
                println!("⛔ Username and/or password are wrong");

                #[cfg(feature = "keyring")]
                {
                    // Delete the wrong password from the keyring
                    let old_username = registry.username;
                    let keyring = keyring::Entry::new(env!("CARGO_PKG_NAME"), &old_username);
                    match keyring.delete_password() {
                        Ok(()) => {
                            println!("🔑 Removed wrong password from keyring")
                        }
                        Err(keyring::Error::NoEntry) | Err(keyring::Error::NoStorageAccess(_)) => {}
                        Err(err) => {
                            eprintln!("⚠️ Warning: Failed to remove password from keyring: {}", err)
                        }
                    }
                }

                bail!("Username and/or password are wrong");
            }
            Err(err) => {
                let filename = i.file_name().unwrap_or_else(|| i.as_os_str());
                if let UploadError::FileExistsError(_) = err {
                    if publish.skip_existing {
                        println!(
                            "⚠️ Note: Skipping {:?} because it appears to already exist",
                            filename
                        );
                        continue;
                    }
                }
                let filesize = fs::metadata(&i)
                    .map(|x| ByteSize(x.len()).to_string())
                    .unwrap_or_else(|e| format!("Failed to get the filesize of {:?}: {}", &i, e));
                return Err(err)
                    .context(format!("💥 Failed to upload {:?} ({})", filename, filesize));
            }
        }
    }

    println!("✨ Packages uploaded successfully");

    #[cfg(feature = "keyring")]
    {
        // We know the password is correct, so we can save it in the keyring
        let username = registry.username.clone();
        let keyring = keyring::Entry::new(env!("CARGO_PKG_NAME"), &username);
        let password = registry.password;
        match keyring.set_password(&password) {
            Ok(()) => {}
            Err(err) => {
                eprintln!(
                    "⚠️ Warning: Failed to store the password in the keyring: {:?}",
                    err
                );
            }
        }
    }

    Ok(())
}
