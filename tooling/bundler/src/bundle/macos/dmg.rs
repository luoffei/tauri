// Copyright 2016-2019 Cargo-Bundle developers <https://github.com/burtonageo/cargo-bundle>
// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use super::{app, icon::create_icns_file};
use crate::{
  bundle::{common::CommandExt, Bundle},
  PackageType::MacOsBundle,
  Settings,
};

use anyhow::Context;
use log::info;

use std::{
  env,
  fs::{self, write},
  path::PathBuf,
  process::{Command, Stdio},
};

/// Bundles the project.
/// Returns a vector of PathBuf that shows where the DMG was created.
pub fn bundle_project(settings: &Settings, bundles: &[Bundle]) -> crate::Result<Vec<PathBuf>> {
  // generate the .app bundle if needed
  if bundles
    .iter()
    .filter(|bundle| bundle.package_type == MacOsBundle)
    .count()
    == 0
  {
    app::bundle_project(settings)?;
  }

  // get the target path
  let output_path = settings.project_out_directory().join("bundle/dmg");

  let package_base_name = format!(
    "{}_{}_{}",
    settings.main_binary_name(),
    settings.version_string(),
    match settings.binary_arch() {
      "x86_64" => "x64",
      other => other,
    }
  );
  let dmg_name = format!("{}.dmg", &package_base_name);
  let dmg_path = output_path.join(&dmg_name);

  let product_name = settings.main_binary_name();
  let bundle_file_name = format!("{}.app", product_name);
  let bundle_dir = settings.project_out_directory().join("bundle/macos");

  let support_directory_path = output_path.join("support");
  if output_path.exists() {
    fs::remove_dir_all(&output_path)
      .with_context(|| format!("Failed to remove old {}", dmg_name))?;
  }
  fs::create_dir_all(&support_directory_path).with_context(|| {
    format!(
      "Failed to create output directory at {:?}",
      support_directory_path
    )
  })?;

  // create paths for script
  let bundle_script_path = output_path.join("bundle_dmg.sh");

  info!(action = "Bundling"; "{} ({})", dmg_name, dmg_path.display());

  // write the scripts
  write(
    &bundle_script_path,
    include_str!("templates/dmg/bundle_dmg"),
  )?;
  write(
    support_directory_path.join("template.applescript"),
    include_str!("templates/dmg/template.applescript"),
  )?;
  write(
    support_directory_path.join("eula-resources-template.xml"),
    include_str!("templates/dmg/eula-resources-template.xml"),
  )?;

  // chmod script for execution
  Command::new("chmod")
    .arg("777")
    .arg(&bundle_script_path)
    .current_dir(&output_path)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .output()
    .expect("Failed to chmod script");

  let mut args = vec![
    "--no-internet-enable".to_owned(),
    "--volname".to_owned(),
    product_name.to_owned(),
    "--icon-size".to_owned(),
    "100".to_owned(),
    "--icon".to_owned(),
    bundle_file_name.clone(),
    "75".to_owned(),
    "64".to_owned(),
    "--app-drop-link".to_owned(),
    "396".to_owned(),
    "64".to_owned(),
    "--window-size".to_owned(),
    "571".to_owned(),
    "375".to_owned(),
    "--hide-extension".to_owned(),
    bundle_file_name.clone(),
  ];

  if let Some(attachments) = &settings.macos().attachments {

    for (index, pair) in attachments.chunks(2).enumerate() {
      let first_name = pair[0].file_name().unwrap().to_str().unwrap();

      args.push("--add-file".to_owned());
      args.push(first_name.to_owned());
      args.push(first_name.to_owned());

      args.push("75".to_owned());
      let y = 64 + (index + 1) * (100 + 60);
      let y = y.to_string();
      args.push(y.clone());
      if pair.len() == 2 {
        let second_name = pair[1].file_name().unwrap().to_str().unwrap();

        args.push("--add-file".to_owned());
        args.push(second_name.to_owned());
        args.push(second_name.to_owned());

        args.push("396".to_owned());
        args.push(y);
      }
    }
  }

  let icns_icon_path =
    create_icns_file(&output_path, settings)?.map(|path| path.to_string_lossy().to_string());
  if let Some(icon) = &icns_icon_path {
    args.push("--volicon".to_owned());
    args.push(icon.to_owned());
  }

  if let Some(background) = &settings.macos().background {
    args.push("--background".to_owned());
    args.push(background.file_name().unwrap().to_str().unwrap().to_owned());
  }

  #[allow(unused_assignments)]
  let mut license_path_ref = "".to_string();
  if let Some(license_path) = &settings.macos().license {
    args.push("--eula".to_owned());
    license_path_ref = env::current_dir()?
      .join(license_path)
      .to_string_lossy()
      .to_string();
    args.push(license_path_ref);
  }

  // Issue #592 - Building MacOS dmg files on CI
  // https://github.com/tauri-apps/tauri/issues/592
  if let Some(value) = env::var_os("CI") {
    if value == "true" {
      args.push("--skip-jenkins".to_owned());
    }
  }

  info!(action = "Running"; "bundle_dmg.sh");

  // execute the bundle script
  Command::new(&bundle_script_path)
    .current_dir(bundle_dir.clone())
    .args(args)
    .args(vec![dmg_name.as_str(), bundle_file_name.as_str()])
    .output_ok()
    .context("error running bundle_dmg.sh")?;

  fs::rename(bundle_dir.join(dmg_name), dmg_path.clone())?;

  // Sign DMG if needed
  if let Some(identity) = &settings.macos().signing_identity {
    super::sign::sign(dmg_path.clone(), identity, settings, false)?;
  }
  Ok(vec![dmg_path])
}
