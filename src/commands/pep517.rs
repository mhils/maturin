use anyhow::{Context, Result};
use cargo_metadata::MetadataCommand;
use maturin::{
    source_distribution, write_dist_info, BridgeModel, BuildOptions, CargoToml, Metadata21,
    PathWriter, PlatformTag,
};
use std::path::PathBuf;
use structopt::StructOpt;

/// Backend for the PEP 517 integration. Not for human consumption
///
/// The commands are meant to be called from the python PEP 517
#[derive(Debug, StructOpt)]
pub enum Pep517Command {
    /// The implementation of prepare_metadata_for_build_wheel
    #[structopt(name = "write-dist-info")]
    WriteDistInfo {
        #[structopt(flatten)]
        build_options: BuildOptions,
        /// The metadata_directory argument to prepare_metadata_for_build_wheel
        #[structopt(long = "metadata-directory", parse(from_os_str))]
        metadata_directory: PathBuf,
        /// Strip the library for minimum file size
        #[structopt(long)]
        strip: bool,
    },
    #[structopt(name = "build-wheel")]
    /// Implementation of build_wheel
    ///
    /// --release and --strip are currently unused by the PEP 517 implementation
    BuildWheel {
        #[structopt(flatten)]
        build_options: BuildOptions,
        /// Strip the library for minimum file size
        #[structopt(long)]
        strip: bool,
        /// Build editable wheels
        #[structopt(long)]
        editable: bool,
    },
    /// The implementation of build_sdist
    #[structopt(name = "write-sdist")]
    WriteSDist {
        /// The sdist_directory argument to build_sdist
        #[structopt(long = "sdist-directory", parse(from_os_str))]
        sdist_directory: PathBuf,
        #[structopt(
            short = "m",
            long = "manifest-path",
            parse(from_os_str),
            default_value = "Cargo.toml",
            name = "PATH"
        )]
        /// The path to the Cargo.toml
        manifest_path: PathBuf,
    },
}

/// Dispatches into the native implementations of the PEP 517 functions
///
/// The last line of stdout is used as return value from the python part of the implementation
pub fn pep517(subcommand: Pep517Command) -> Result<()> {
    match subcommand {
        Pep517Command::WriteDistInfo {
            build_options,
            metadata_directory,
            strip,
        } => {
            assert!(matches!(
                build_options.interpreter.as_ref(),
                Some(version) if version.len() == 1
            ));
            let context = build_options.into_build_context(true, strip, false)?;

            // Since afaik all other PEP 517 backends also return linux tagged wheels, we do so too
            let tags = match context.bridge {
                BridgeModel::Bindings(_) => {
                    vec![context.interpreter[0].get_tag(PlatformTag::Linux, context.universal2)?]
                }
                BridgeModel::BindingsAbi3(major, minor) => {
                    let platform = context
                        .target
                        .get_platform_tag(PlatformTag::Linux, context.universal2)?;
                    vec![format!("cp{}{}-abi3-{}", major, minor, platform)]
                }
                BridgeModel::Bin | BridgeModel::Cffi => {
                    context
                        .target
                        .get_universal_tags(PlatformTag::Linux, context.universal2)?
                        .1
                }
            };

            let mut writer = PathWriter::from_path(metadata_directory);
            write_dist_info(&mut writer, &context.metadata21, &tags)?;
            println!("{}", context.metadata21.get_dist_info_dir().display());
        }
        Pep517Command::BuildWheel {
            build_options,
            strip,
            editable,
        } => {
            let build_context = build_options.into_build_context(true, strip, editable)?;
            let wheels = build_context.build_wheels()?;
            assert_eq!(wheels.len(), 1);
            println!("{}", wheels[0].0.to_str().unwrap());
        }
        Pep517Command::WriteSDist {
            sdist_directory,
            manifest_path,
        } => {
            let cargo_toml = CargoToml::from_path(&manifest_path)?;
            let manifest_dir = manifest_path.parent().unwrap();
            let metadata21 = Metadata21::from_cargo_toml(&cargo_toml, &manifest_dir)
                .context("Failed to parse Cargo.toml into python metadata")?;
            let cargo_metadata = MetadataCommand::new()
                .manifest_path(&manifest_path)
                .exec()
                .context("Cargo metadata failed. Do you have cargo in your PATH?")?;

            let path = source_distribution(
                sdist_directory,
                &metadata21,
                &manifest_path,
                &cargo_metadata,
                None,
            )
            .context("Failed to build source distribution")?;
            println!("{}", path.file_name().unwrap().to_str().unwrap());
        }
    };

    Ok(())
}
